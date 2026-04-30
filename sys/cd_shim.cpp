// CD-ROM shim: bridges Rust to MAME's cdrom_file + a port of chdman's
// `chd_cd_compressor` (which is tool-side code, not in MAME's lib). All
// CD-format logic — CUE parsing, track padding, ECC/EDC, audio
// byte-swap, metadata records — lives in MAME and is called through
// here without any Rust-side reimplementation.
//
// The chd_cd_compressor body is a near-verbatim port of chdman.cpp:419
// (chd_cd_compressor) so byte-for-byte parity with chdman is preserved
// for the same input.

#include "chd.h"
#include "cdrom.h"
#include "corefile.h"
#include "ioprocs.h"
#include "chd_shim.h"

#include <cstring>
#include <memory>
#include <string>
#include <system_error>

namespace {

// Convert MAME's std::error_condition to our shim error_t. Mirrors the
// helper in chd_shim.cpp.
static chd_error_t to_chd_error(std::error_condition err) {
    if (!err) return 0;
    if (err.category() == chd_category()) return (chd_error_t)err.value();
    return (chd_error_t)chd_file::error::INVALID_FILE;
}

// Port of chdman.cpp's `chd_cd_compressor`. See chdman.cpp:419.
class CdCompressor : public chd_file_compressor {
public:
    CdCompressor(cdrom_file::toc &toc, cdrom_file::track_input_info &info)
        : m_file()
        , m_toc(toc)
        , m_info(info) {}

    ~CdCompressor() override {}

    uint32_t read_data(void *_dest, uint64_t offset, uint32_t length) override {
        // verify assumptions made below
        if (offset % cdrom_file::FRAME_SIZE != 0) return 0;
        if (length % cdrom_file::FRAME_SIZE != 0) return 0;

        uint8_t *dest = reinterpret_cast<uint8_t *>(_dest);
        memset(dest, 0, length);

        uint64_t startoffs = 0;
        uint32_t length_remaining = length;
        for (int tracknum = 0; tracknum < int(m_toc.numtrks); tracknum++) {
            const cdrom_file::track_info &trackinfo = m_toc.tracks[tracknum];
            uint64_t endoffs = startoffs + (uint64_t)(trackinfo.frames + trackinfo.extraframes) * cdrom_file::FRAME_SIZE;

            if (offset >= startoffs && offset < endoffs) {
                if (!m_file || m_lastfile.compare(m_info.track[tracknum].fname) != 0) {
                    m_file.reset();
                    m_lastfile = m_info.track[tracknum].fname;
                    std::error_condition const filerr = util::core_file::open(m_lastfile, OPEN_FLAG_READ, m_file);
                    if (filerr) return length - length_remaining;
                }

                uint64_t bytesperframe = trackinfo.datasize + trackinfo.subsize;
                uint64_t src_track_start = m_info.track[tracknum].offset;
                uint64_t src_track_end = src_track_start + bytesperframe * (uint64_t)trackinfo.frames;
                uint64_t split_track_start = src_track_end - ((uint64_t)trackinfo.splitframes * bytesperframe);
                uint64_t pad_track_start = split_track_start - ((uint64_t)trackinfo.padframes * bytesperframe);

                if ((uint64_t)trackinfo.splitframes == 0L)
                    split_track_start = UINT64_MAX;

                while (length_remaining != 0 && offset < endoffs) {
                    uint64_t src_frame_start = src_track_start + ((offset - startoffs) / cdrom_file::FRAME_SIZE) * bytesperframe;

                    if (src_frame_start >= split_track_start && src_frame_start < src_track_end &&
                        m_lastfile.compare(m_info.track[tracknum + 1].fname) != 0) {
                        m_file.reset();
                        m_lastfile = m_info.track[tracknum + 1].fname;
                        std::error_condition const filerr = util::core_file::open(m_lastfile, OPEN_FLAG_READ, m_file);
                        if (filerr) return length - length_remaining;
                    }

                    if (src_frame_start < src_track_end) {
                        if (src_frame_start >= pad_track_start && src_frame_start < split_track_start) {
                            memset(dest, 0, bytesperframe);
                        } else {
                            std::error_condition err = m_file->seek(
                                (src_frame_start >= split_track_start)
                                    ? src_frame_start - split_track_start
                                    : src_frame_start,
                                SEEK_SET);
                            std::size_t count = 0;
                            if (!err) {
                                std::tie(err, count) = util::read(*m_file, dest, bytesperframe);
                            }
                            (void)count;
                            // On read error: zero-fill (already done) and continue.
                        }

                        if (m_info.track[tracknum].swap) {
                            for (uint32_t swapindex = 0; swapindex < 2352; swapindex += 2) {
                                uint8_t temp = dest[swapindex];
                                dest[swapindex] = dest[swapindex + 1];
                                dest[swapindex + 1] = temp;
                            }
                        }
                    }

                    offset += cdrom_file::FRAME_SIZE;
                    dest += cdrom_file::FRAME_SIZE;
                    length_remaining -= cdrom_file::FRAME_SIZE;
                    if (length_remaining == 0) break;
                }
            }

            startoffs = endoffs;
        }
        return length - length_remaining;
    }

private:
    std::string m_lastfile;
    util::core_file::ptr m_file;
    cdrom_file::toc &m_toc;
    cdrom_file::track_input_info &m_info;
};

} // namespace

// Concrete shim handle types. Exposed only as opaque to Rust.
struct chd_shim_toc_t {
    cdrom_file::toc toc;
    cdrom_file::track_input_info info;
};

struct chd_shim_cdrom_t {
    std::unique_ptr<cdrom_file> cd;
};

extern "C" {

chd_shim_toc_t* chd_shim_toc_alloc() {
    auto* t = new chd_shim_toc_t();
    memset(&t->toc, 0, sizeof(t->toc));
    t->info.reset();
    return t;
}

void chd_shim_toc_free(chd_shim_toc_t* toc) {
    delete toc;
}

chd_error_t chd_shim_toc_parse(chd_shim_toc_t* toc, const char* path) {
    return to_chd_error(cdrom_file::parse_toc(path, toc->toc, toc->info));
}

uint32_t chd_shim_toc_num_tracks(const chd_shim_toc_t* toc) {
    return toc->toc.numtrks;
}

uint32_t chd_shim_toc_num_sessions(const chd_shim_toc_t* toc) {
    return toc->toc.numsessions;
}

uint32_t chd_shim_toc_flags(const chd_shim_toc_t* toc) {
    return toc->toc.flags;
}

static void copy_track(const cdrom_file::track_info& src, chd_shim_track_t* out) {
    out->trktype = src.trktype;
    out->subtype = src.subtype;
    out->datasize = src.datasize;
    out->subsize = src.subsize;
    out->frames = src.frames;
    out->extraframes = src.extraframes;
    out->pregap = src.pregap;
    out->postgap = src.postgap;
    out->pgtype = src.pgtype;
    out->pgsub = src.pgsub;
    out->pgdatasize = src.pgdatasize;
    out->pgsubsize = src.pgsubsize;
    out->padframes = src.padframes;
    out->splitframes = src.splitframes;
}

void chd_shim_toc_get_track(const chd_shim_toc_t* toc, uint32_t i, chd_shim_track_t* out) {
    if (i >= toc->toc.numtrks) {
        memset(out, 0, sizeof(*out));
        return;
    }
    copy_track(toc->toc.tracks[i], out);
}

const char* chd_shim_toc_get_track_fname(const chd_shim_toc_t* toc, uint32_t i) {
    if (i >= cdrom_file::MAX_TRACKS) return nullptr;
    return toc->info.track[i].fname.c_str();
}

uint32_t chd_shim_toc_get_track_offset(const chd_shim_toc_t* toc, uint32_t i) {
    if (i >= cdrom_file::MAX_TRACKS) return 0;
    return toc->info.track[i].offset;
}

int chd_shim_toc_get_track_swap(const chd_shim_toc_t* toc, uint32_t i) {
    if (i >= cdrom_file::MAX_TRACKS) return 0;
    return toc->info.track[i].swap ? 1 : 0;
}

void chd_shim_toc_pad_tracks(chd_shim_toc_t* toc) {
    // Mirror chdman.cpp:2192 — round each track up to TRACK_PADDING (4) frames.
    for (uint32_t tracknum = 0; tracknum < toc->toc.numtrks; tracknum++) {
        cdrom_file::track_info& t = toc->toc.tracks[tracknum];
        uint32_t padded = (t.frames + cdrom_file::TRACK_PADDING - 1) / cdrom_file::TRACK_PADDING;
        t.extraframes = padded * cdrom_file::TRACK_PADDING - t.frames;
    }
}

uint64_t chd_shim_toc_logical_bytes(const chd_shim_toc_t* toc) {
    uint64_t total = 0;
    for (uint32_t i = 0; i < toc->toc.numtrks; i++) {
        const cdrom_file::track_info& t = toc->toc.tracks[i];
        total += (uint64_t)(t.frames + t.extraframes) * cdrom_file::FRAME_SIZE;
    }
    return total;
}

chd_file_compressor_t* chd_shim_cd_compressor_alloc(chd_shim_toc_t* toc) {
    return reinterpret_cast<chd_file_compressor_t*>(new CdCompressor(toc->toc, toc->info));
}

chd_error_t chd_shim_cd_write_metadata(chd_file_t* chd, const chd_shim_toc_t* toc) {
    return to_chd_error(cdrom_file::write_metadata(reinterpret_cast<chd_file*>(chd), toc->toc));
}

chd_shim_cdrom_t* chd_shim_cdrom_open(chd_file_t* chd) {
    auto* w = new chd_shim_cdrom_t();
    w->cd.reset(new cdrom_file(reinterpret_cast<chd_file*>(chd)));
    return w;
}

void chd_shim_cdrom_free(chd_shim_cdrom_t* c) {
    delete c;
}

uint32_t chd_shim_cdrom_num_tracks(const chd_shim_cdrom_t* c) {
    return c->cd->get_last_track();
}

void chd_shim_cdrom_get_track(const chd_shim_cdrom_t* c, uint32_t i, chd_shim_track_t* out) {
    const cdrom_file::toc& toc = c->cd->get_toc();
    if (i >= toc.numtrks) {
        memset(out, 0, sizeof(*out));
        return;
    }
    copy_track(toc.tracks[i], out);
}

uint32_t chd_shim_cdrom_get_track_start(const chd_shim_cdrom_t* c, uint32_t track) {
    return c->cd->get_track_start(track);
}

int chd_shim_cdrom_read_data(chd_shim_cdrom_t* c, uint32_t lba, void* buffer, uint32_t datatype, int phys) {
    return c->cd->read_data(lba, buffer, datatype, phys != 0) ? 1 : 0;
}

int chd_shim_cdrom_read_subcode(chd_shim_cdrom_t* c, uint32_t lba, void* buffer, int phys) {
    return c->cd->read_subcode(lba, buffer, phys != 0) ? 1 : 0;
}

}
