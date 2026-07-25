#include "chd.h"
#include "ioprocs.h"
#include "chdcodec.h"
#include "chd_shim.h"
#include "shim_guard.h"
#include <memory>
#include <string_view>
#include <system_error>
#include <cstring>

class RustRandomReadWrite : public util::random_read_write {
public:
     RustRandomReadWrite(chd_rust_io_t handle, chd_rust_io_ops_t ops)
        : m_handle(handle), m_ops(ops), m_offset(0) {}

    virtual ~RustRandomReadWrite() {
        if (m_ops.close) m_ops.close(m_handle);
    }

    virtual std::error_condition seek(std::int64_t offset, int whence) noexcept override {
        uint64_t new_offset;
        switch (whence) {
            case SEEK_SET: new_offset = (uint64_t)offset; break;
            case SEEK_CUR: new_offset = m_offset + offset; break;
            case SEEK_END: {
                uint64_t len;
                auto err = length(len);
                if (err) return err;
                new_offset = len + offset;
                break;
            }
            default: return std::make_error_condition(std::errc::invalid_argument);
        }
        m_offset = new_offset;
        return std::error_condition();
    }

    virtual std::error_condition tell(std::uint64_t &result) noexcept override {
        result = m_offset;
        return std::error_condition();
    }

    virtual std::error_condition length(std::uint64_t &result) noexcept override {
        auto res = m_ops.length(m_handle, &result);
        if (res != 0) return std::make_error_condition(std::errc::io_error);
        return std::error_condition();
    }

    virtual std::error_condition read_some(void *buffer, std::size_t length, std::size_t &actual) noexcept override {
        uint32_t u_actual;
        auto res = m_ops.read(m_handle, m_offset, buffer, (uint32_t)length, &u_actual);
        actual = u_actual;
        m_offset += actual;
        if (res != 0) return std::make_error_condition(std::errc::io_error);
        return std::error_condition();
    }

    virtual std::error_condition read_some_at(std::uint64_t offset, void *buffer, std::size_t length, std::size_t &actual) noexcept override {
        uint32_t u_actual;
        auto res = m_ops.read(m_handle, offset, buffer, (uint32_t)length, &u_actual);
        actual = u_actual;
        if (res != 0) return std::make_error_condition(std::errc::io_error);
        return std::error_condition();
    }

    virtual std::error_condition write_some(void const *buffer, std::size_t length, std::size_t &actual) noexcept override {
        uint32_t u_actual;
        auto res = m_ops.write(m_handle, m_offset, buffer, (uint32_t)length, &u_actual);
        actual = u_actual;
        m_offset += actual;
        if (res != 0) return std::make_error_condition(std::errc::io_error);
        return std::error_condition();
    }

    virtual std::error_condition write_some_at(std::uint64_t offset, void const *buffer, std::size_t length, std::size_t &actual) noexcept override {
        uint32_t u_actual;
        auto res = m_ops.write(m_handle, offset, buffer, (uint32_t)length, &u_actual);
        actual = u_actual;
        if (res != 0) return std::make_error_condition(std::errc::io_error);
        return std::error_condition();
    }

    virtual std::error_condition finalize() noexcept override { return std::error_condition(); }
    virtual std::error_condition flush() noexcept override { return std::error_condition(); }

private:
    chd_rust_io_t m_handle;
    chd_rust_io_ops_t m_ops;
    uint64_t m_offset;
};

class RustChdCompressor : public chd_file_compressor {
public:
    RustChdCompressor(chd_rust_compressor_t handle, chd_rust_compressor_ops_t ops)
        : m_handle(handle), m_ops(ops) {}

protected:
    virtual uint32_t read_data(void *dest, uint64_t offset, uint32_t length) override {
        return m_ops.read_data(m_handle, dest, offset, length);
    }

private:
    chd_rust_compressor_t m_handle;
    chd_rust_compressor_ops_t m_ops;
};

static chd_error_t to_chd_error(std::error_condition err) {
    if (!err) return 0;
    if (err.category() == chd_category()) return (chd_error_t)err.value();
    return (chd_error_t)chd_file::error::INVALID_FILE;
}

// What a chd_error_t entry point returns when an exception escaped its
// body. Same code to_chd_error uses for errors outside chd_category —
// there is no MAME error value that means "C++ threw".
static constexpr chd_error_t SHIM_ERR_EXCEPTION = (chd_error_t)chd_file::error::INVALID_FILE;

// Every function below routes through chd_shim::guard / guard_void. MAME's
// C++ throws (std::bad_alloc, cdrom_file's `throw nullptr`) and a foreign
// exception reaching Rust aborts the process — see shim_guard.h.
extern "C" {

chd_file_t* chd_shim_alloc() {
    return chd_shim::guard([] { return (chd_file_t*)(new chd_file()); }, nullptr);
}

void chd_shim_free(chd_file_t* chd) {
    chd_shim::guard_void([&] { delete (chd_file*)(chd); });
}

chd_error_t chd_shim_open_file(chd_file_t* chd, const char* filename, int writeable, chd_file_t* parent) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->open(filename, writeable != 0, (chd_file*)parent));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_open_custom(chd_file_t* chd, chd_rust_io_t handle, chd_rust_io_ops_t ops, int writeable, chd_file_t* parent) {
    return chd_shim::guard([&] {
        // Once the wrapper exists it owns `handle` and its destructor calls
        // ops.close, including on the throwing paths below. The only case
        // that leaks the Rust handle is make_unique itself throwing, which
        // means the allocation failed before anything took ownership.
        auto io = std::make_unique<RustRandomReadWrite>(handle, ops);
        return to_chd_error(((chd_file*)chd)->open(std::move(io), writeable != 0, (chd_file*)parent));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_create_file(chd_file_t* chd, const char* filename, uint64_t logicalbytes, uint32_t hunkbytes, uint32_t unitbytes, const uint32_t compression[4]) {
    return chd_shim::guard([&] {
        chd_codec_type comp[4];
        for (int i = 0; i < 4; i++) comp[i] = compression[i];
        return to_chd_error(((chd_file*)chd)->create(filename, logicalbytes, hunkbytes, unitbytes, comp));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_create_file_with_parent(chd_file_t* chd, const char* filename, uint64_t logicalbytes, uint32_t hunkbytes, const uint32_t compression[4], chd_file_t* parent) {
    return chd_shim::guard([&] {
        chd_codec_type comp[4];
        for (int i = 0; i < 4; i++) comp[i] = compression[i];
        return to_chd_error(((chd_file*)chd)->create(filename, logicalbytes, hunkbytes, comp, *(chd_file*)parent));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_clone_all_metadata(chd_file_t* chd, chd_file_t* source) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->clone_all_metadata(*(chd_file*)source));
    }, SHIM_ERR_EXCEPTION);
}

void chd_shim_close(chd_file_t* chd) {
    chd_shim::guard_void([&] { ((chd_file*)chd)->close(); });
}

uint32_t chd_shim_version(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->version(); }, 0);
}

uint32_t chd_shim_hunk_bytes(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->hunk_bytes(); }, 0);
}

uint32_t chd_shim_hunk_count(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->hunk_count(); }, 0);
}

uint32_t chd_shim_unit_bytes(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->unit_bytes(); }, 0);
}

uint64_t chd_shim_unit_count(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->unit_count(); }, 0);
}

uint64_t chd_shim_logical_bytes(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->logical_bytes(); }, 0);
}

chd_error_t chd_shim_read_hunk(chd_file_t* chd, uint32_t hunknum, void* buffer) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->read_hunk(hunknum, buffer));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_write_hunk(chd_file_t* chd, uint32_t hunknum, const void* buffer) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->write_hunk(hunknum, buffer));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_read_bytes(chd_file_t* chd, uint64_t offset, void* buffer, uint32_t bytes) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->read_bytes(offset, buffer, bytes));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_write_bytes(chd_file_t* chd, uint64_t offset, const void* buffer, uint32_t bytes) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->write_bytes(offset, buffer, bytes));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_read_metadata(chd_file_t* chd, uint32_t tag, uint32_t index, void* buffer, uint32_t buffer_len, uint32_t* result_len) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->read_metadata(tag, index, buffer, buffer_len, *result_len));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_write_metadata(chd_file_t* chd, uint32_t tag, uint32_t index, const void* buffer, uint32_t length, uint8_t flags) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->write_metadata(tag, index, buffer, length, flags));
    }, SHIM_ERR_EXCEPTION);
}

chd_error_t chd_shim_delete_metadata(chd_file_t* chd, uint32_t tag, uint32_t index) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file*)chd)->delete_metadata(tag, index));
    }, SHIM_ERR_EXCEPTION);
}

void chd_shim_get_sha1(chd_file_t* chd, uint8_t* sha1) {
    chd_shim::guard_void([&] {
        util::sha1_t s = ((chd_file*)chd)->sha1();
        memcpy(sha1, s.m_raw, 20);
    });
}

void chd_shim_get_raw_sha1(chd_file_t* chd, uint8_t* sha1) {
    chd_shim::guard_void([&] {
        util::sha1_t s = ((chd_file*)chd)->raw_sha1();
        memcpy(sha1, s.m_raw, 20);
    });
}

void chd_shim_get_parent_sha1(chd_file_t* chd, uint8_t* sha1) {
    chd_shim::guard_void([&] {
        util::sha1_t s = ((chd_file*)chd)->parent_sha1();
        memcpy(sha1, s.m_raw, 20);
    });
}

chd_error_t chd_shim_hunk_info(chd_file_t* chd, uint32_t hunknum, uint32_t* compressor, uint32_t* compbytes) {
    return chd_shim::guard([&] {
        chd_codec_type type;
        uint32_t cb;
        auto err = ((chd_file*)chd)->hunk_info(hunknum, type, cb);
        *compressor = (uint32_t)type;
        *compbytes = cb;
        return to_chd_error(err);
    }, SHIM_ERR_EXCEPTION);
}

chd_file_compressor_t* chd_shim_compressor_alloc(chd_rust_compressor_t handle, chd_rust_compressor_ops_t ops) {
    return chd_shim::guard([&] {
        return (chd_file_compressor_t*)(new RustChdCompressor(handle, ops));
    }, nullptr);
}

void chd_shim_compressor_free(chd_file_compressor_t* compressor) {
    chd_shim::guard_void([&] { delete (chd_file_compressor*)(compressor); });
}

chd_error_t chd_shim_compressor_create_file(chd_file_compressor_t* compressor, const char* filename, uint64_t logicalbytes, uint32_t hunkbytes, uint32_t unitbytes, const uint32_t compression[4]) {
    return chd_shim::guard([&] {
        chd_codec_type comp[4];
        for (int i = 0; i < 4; i++) comp[i] = compression[i];
        return to_chd_error(((chd_file_compressor*)compressor)->create(filename, logicalbytes, hunkbytes, unitbytes, comp));
    }, SHIM_ERR_EXCEPTION);
}

void chd_shim_compressor_begin(chd_file_compressor_t* compressor) {
    chd_shim::guard_void([&] { ((chd_file_compressor*)compressor)->compress_begin(); });
}

chd_error_t chd_shim_compressor_continue(chd_file_compressor_t* compressor, double* progress, double* ratio) {
    return chd_shim::guard([&] {
        return to_chd_error(((chd_file_compressor*)compressor)->compress_continue(*progress, *ratio));
    }, SHIM_ERR_EXCEPTION);
}

int chd_shim_codec_exists(uint32_t type) {
    return chd_shim::guard([&] {
        return chd_codec_list::codec_exists((chd_codec_type)type) ? 1 : 0;
    }, 0);
}

const char* chd_shim_codec_name(uint32_t type) {
    return chd_shim::guard([&] {
        return chd_codec_list::codec_name((chd_codec_type)type);
    }, nullptr);
}

int chd_shim_compressed(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->compressed() ? 1 : 0; }, 0);
}

uint32_t chd_shim_compression(chd_file_t* chd, int index) {
    return chd_shim::guard([&]() -> uint32_t {
        if (index < 0 || index >= 4) return 0;
        return (uint32_t)((chd_file*)chd)->compression(index);
    }, 0);
}

int chd_shim_has_parent(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->parent() != nullptr ? 1 : 0; }, 0);
}

// MAME's check_is_*() return std::error_condition: empty (falsy) means
// success ("yes, this CHD is of that kind"), non-empty (truthy) means
// "no". The ternary inverts that back to the more conventional shim
// convention of 1 = yes, 0 = no — which is what the Rust side reads via
// `!= 0`. Counter-intuitive at first glance; intentional.
// Fallback 0 for all five: on an exception we report "not that kind of
// CHD", the same answer a failed metadata lookup gives.
int chd_shim_check_is_hd(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->check_is_hd() ? 0 : 1; }, 0);
}
int chd_shim_check_is_cd(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->check_is_cd() ? 0 : 1; }, 0);
}
int chd_shim_check_is_gd(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->check_is_gd() ? 0 : 1; }, 0);
}
int chd_shim_check_is_dvd(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->check_is_dvd() ? 0 : 1; }, 0);
}
int chd_shim_check_is_av(chd_file_t* chd) {
    return chd_shim::guard([&] { return ((chd_file*)chd)->check_is_av() ? 0 : 1; }, 0);
}

chd_error_t chd_shim_metadata_enum(chd_file_t* chd, uint32_t index,
                                    uint32_t* out_tag, uint8_t* out_flags,
                                    void* buffer, uint32_t buffer_len,
                                    uint32_t* result_len) {
    return chd_shim::guard([&]() -> chd_error_t {
        std::vector<uint8_t> output;
        chd_metadata_tag tag = 0;
        uint8_t flags = 0;
        auto err = ((chd_file*)chd)->read_metadata(CHDMETATAG_WILDCARD, index, output, tag, flags);
        if (err) return to_chd_error(err);
        if (out_tag) *out_tag = (uint32_t)tag;
        if (out_flags) *out_flags = flags;
        if (result_len) *result_len = (uint32_t)output.size();
        if (buffer && buffer_len > 0) {
            uint32_t to_copy = (uint32_t)output.size();
            if (to_copy > buffer_len) to_copy = buffer_len;
            memcpy(buffer, output.data(), to_copy);
        }
        return 0;
    }, SHIM_ERR_EXCEPTION);
}

}

extern "C" {
    struct chd_sha1_t { util::sha1_creator creator; };
    chd_sha1_t* chd_shim_sha1_alloc() {
        return chd_shim::guard([] { return new chd_sha1_t(); }, nullptr);
    }
    void chd_shim_sha1_free(chd_sha1_t* s) {
        chd_shim::guard_void([&] { delete s; });
    }
    void chd_shim_sha1_append(chd_sha1_t* s, const void* data, uint32_t length) {
        chd_shim::guard_void([&] { s->creator.append(data, length); });
    }
    void chd_shim_sha1_finish(chd_sha1_t* s, uint8_t* sha1) {
        chd_shim::guard_void([&] {
            util::sha1_t res = s->creator.finish();
            memcpy(sha1, res.m_raw, 20);
        });
    }
}
