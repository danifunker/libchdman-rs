#ifndef CHD_SHIM_H
#define CHD_SHIM_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct chd_file_t chd_file_t;
typedef struct chd_file_compressor_t chd_file_compressor_t;

typedef int32_t chd_error_t;

// Opaque handle for Rust-backed I/O
typedef void* chd_rust_io_t;

typedef struct {
    int (*read)(chd_rust_io_t handle, uint64_t offset, void* buffer, uint32_t length, uint32_t* actual);
    int (*write)(chd_rust_io_t handle, uint64_t offset, const void* buffer, uint32_t length, uint32_t* actual);
    int (*length)(chd_rust_io_t handle, uint64_t* result);
    void (*close)(chd_rust_io_t handle);
} chd_rust_io_ops_t;

// Handle for Rust-backed compression source
typedef void* chd_rust_compressor_t;
typedef struct {
    uint32_t (*read_data)(chd_rust_compressor_t handle, void* dest, uint64_t offset, uint32_t length);
} chd_rust_compressor_ops_t;

// Allocation and Lifecycle
chd_file_t* chd_shim_alloc();
void chd_shim_free(chd_file_t* chd);

// File Operations
chd_error_t chd_shim_open_file(chd_file_t* chd, const char* filename, int writeable, chd_file_t* parent);
chd_error_t chd_shim_open_custom(chd_file_t* chd, chd_rust_io_t handle, chd_rust_io_ops_t ops, int writeable, chd_file_t* parent);
chd_error_t chd_shim_create_file(chd_file_t* chd, const char* filename, uint64_t logicalbytes, uint32_t hunkbytes, uint32_t unitbytes, const uint32_t compression[4]);
// Create a child CHD that diffs against `parent`. Unit size, logical size,
// and (for compressed parents) hunk size are inherited from the parent —
// MAME's create(filename, logicalbytes, hunkbytes, compression, parent)
// overload (chd.h:326). Pass compression=[0,0,0,0] for an uncompressed diff
// (the typical "writeable child of a compressed parent" case, matching what
// MAME does at runtime when an emulated machine writes to a compressed CHD).
chd_error_t chd_shim_create_file_with_parent(chd_file_t* chd, const char* filename, uint64_t logicalbytes, uint32_t hunkbytes, const uint32_t compression[4], chd_file_t* parent);
void chd_shim_close(chd_file_t* chd);

// Header / Info
uint32_t chd_shim_version(chd_file_t* chd);
uint32_t chd_shim_hunk_bytes(chd_file_t* chd);
uint32_t chd_shim_hunk_count(chd_file_t* chd);
uint32_t chd_shim_unit_bytes(chd_file_t* chd);
uint64_t chd_shim_unit_count(chd_file_t* chd);
uint64_t chd_shim_logical_bytes(chd_file_t* chd);
void chd_shim_get_sha1(chd_file_t* chd, uint8_t* sha1);
void chd_shim_get_raw_sha1(chd_file_t* chd, uint8_t* sha1);
void chd_shim_get_parent_sha1(chd_file_t* chd, uint8_t* sha1);
chd_error_t chd_shim_hunk_info(chd_file_t* chd, uint32_t hunknum, uint32_t* compressor, uint32_t* compbytes);

// Data I/O
chd_error_t chd_shim_read_hunk(chd_file_t* chd, uint32_t hunknum, void* buffer);
chd_error_t chd_shim_write_hunk(chd_file_t* chd, uint32_t hunknum, const void* buffer);
chd_error_t chd_shim_read_bytes(chd_file_t* chd, uint64_t offset, void* buffer, uint32_t bytes);
chd_error_t chd_shim_write_bytes(chd_file_t* chd, uint64_t offset, const void* buffer, uint32_t bytes);

// Metadata
chd_error_t chd_shim_read_metadata(chd_file_t* chd, uint32_t tag, uint32_t index, void* buffer, uint32_t buffer_len, uint32_t* result_len);
chd_error_t chd_shim_write_metadata(chd_file_t* chd, uint32_t tag, uint32_t index, const void* buffer, uint32_t length, uint8_t flags);
chd_error_t chd_shim_delete_metadata(chd_file_t* chd, uint32_t tag, uint32_t index);

// Compressor
chd_file_compressor_t* chd_shim_compressor_alloc(chd_rust_compressor_t handle, chd_rust_compressor_ops_t ops);
void chd_shim_compressor_free(chd_file_compressor_t* compressor);
chd_error_t chd_shim_compressor_create_file(chd_file_compressor_t* compressor, const char* filename, uint64_t logicalbytes, uint32_t hunkbytes, uint32_t unitbytes, const uint32_t compression[4]);
void chd_shim_compressor_begin(chd_file_compressor_t* compressor);
chd_error_t chd_shim_compressor_continue(chd_file_compressor_t* compressor, double* progress, double* ratio);

// Codec Helpers
int chd_shim_codec_exists(uint32_t type);
const char* chd_shim_codec_name(uint32_t type);

// Header introspection (chd-rs feature parity)
int chd_shim_compressed(chd_file_t* chd);
uint32_t chd_shim_compression(chd_file_t* chd, int index);
int chd_shim_has_parent(chd_file_t* chd);
int chd_shim_check_is_hd(chd_file_t* chd);
int chd_shim_check_is_cd(chd_file_t* chd);
int chd_shim_check_is_gd(chd_file_t* chd);
int chd_shim_check_is_dvd(chd_file_t* chd);
int chd_shim_check_is_av(chd_file_t* chd);

// Metadata enumeration: returns the entry at ordinal `index` across all tags.
// On success fills *out_tag, *out_flags, copies up to buffer_len bytes into
// buffer, and writes the full size into *result_len. Pass buffer=NULL,
// buffer_len=0 to query size only.
chd_error_t chd_shim_metadata_enum(chd_file_t* chd, uint32_t index,
                                    uint32_t* out_tag, uint8_t* out_flags,
                                    void* buffer, uint32_t buffer_len,
                                    uint32_t* result_len);

// === CD-ROM support ===
//
// All CD logic is delegated to MAME's `cdrom_file` (parse_toc, ECC/EDC,
// audio byte-swap, write_metadata) and a port of chdman's
// `chd_cd_compressor` (which is in chdman.cpp, not in MAME's lib). No
// CD-format logic is reimplemented in Rust.
typedef struct chd_shim_toc_t chd_shim_toc_t;
typedef struct chd_shim_cdrom_t chd_shim_cdrom_t;

// Mirrors `cdrom_file::track_info` for the fields chdman uses on create.
typedef struct {
    uint32_t trktype;
    uint32_t subtype;
    uint32_t datasize;
    uint32_t subsize;
    uint32_t frames;
    uint32_t extraframes;
    uint32_t pregap;
    uint32_t postgap;
    uint32_t pgtype;
    uint32_t pgsub;
    uint32_t pgdatasize;
    uint32_t pgsubsize;
    uint32_t padframes;
    uint32_t splitframes;
} chd_shim_track_t;

// TOC parse + per-track introspection.
chd_shim_toc_t* chd_shim_toc_alloc();
void chd_shim_toc_free(chd_shim_toc_t* toc);
chd_error_t chd_shim_toc_parse(chd_shim_toc_t* toc, const char* path);
uint32_t chd_shim_toc_num_tracks(const chd_shim_toc_t* toc);
uint32_t chd_shim_toc_num_sessions(const chd_shim_toc_t* toc);
uint32_t chd_shim_toc_flags(const chd_shim_toc_t* toc);
void chd_shim_toc_get_track(const chd_shim_toc_t* toc, uint32_t i, chd_shim_track_t* out);
const char* chd_shim_toc_get_track_fname(const chd_shim_toc_t* toc, uint32_t i);
uint32_t chd_shim_toc_get_track_offset(const chd_shim_toc_t* toc, uint32_t i);
int chd_shim_toc_get_track_swap(const chd_shim_toc_t* toc, uint32_t i);
// After CHD creation we pad each track to TRACK_PADDING frames; chdman
// writes the padding count back into trackinfo.extraframes before the
// metadata is written. Call this before chd_shim_cd_write_metadata.
void chd_shim_toc_pad_tracks(chd_shim_toc_t* toc);
// Total logical bytes implied by the (possibly padded) toc.
uint64_t chd_shim_toc_logical_bytes(const chd_shim_toc_t* toc);

// CD compressor: a chd_file_compressor subclass implementing read_data
// the way chdman's `chd_cd_compressor` does (track lookup + audio
// byte-swap + ECC/EDC handled via MAME's frame layout). Returns a
// `chd_file_compressor_t*` so the existing compressor shims work.
chd_file_compressor_t* chd_shim_cd_compressor_alloc(chd_shim_toc_t* toc);

// Writes CHT2 records using `cdrom_file::write_metadata`. Pass the
// compressor's underlying chd_file pointer (as_chd_file_ptr in Rust).
chd_error_t chd_shim_cd_write_metadata(chd_file_t* chd, const chd_shim_toc_t* toc);

// Read-side: open a CHD as a cdrom_file for extract.
chd_shim_cdrom_t* chd_shim_cdrom_open(chd_file_t* chd);
void chd_shim_cdrom_free(chd_shim_cdrom_t* c);
uint32_t chd_shim_cdrom_num_tracks(const chd_shim_cdrom_t* c);
void chd_shim_cdrom_get_track(const chd_shim_cdrom_t* c, uint32_t i, chd_shim_track_t* out);
uint32_t chd_shim_cdrom_get_track_start(const chd_shim_cdrom_t* c, uint32_t track);
// Read a single LBA sector. `buffer` must be sized for `datatype`
// (e.g. 2352 for raw, 2048 for cooked MODE1). `phys` flag matches
// cdrom_file::read_data's `phys` param.
int chd_shim_cdrom_read_data(chd_shim_cdrom_t* c, uint32_t lba, void* buffer, uint32_t datatype, int phys);
int chd_shim_cdrom_read_subcode(chd_shim_cdrom_t* c, uint32_t lba, void* buffer, int phys);

#ifdef __cplusplus
}
#endif

#endif

#ifdef __cplusplus
extern "C" {
#endif
typedef struct chd_sha1_t chd_sha1_t;
chd_sha1_t* chd_shim_sha1_alloc();
void chd_shim_sha1_free(chd_sha1_t* s);
void chd_shim_sha1_append(chd_sha1_t* s, const void* data, uint32_t length);
void chd_shim_sha1_finish(chd_sha1_t* s, uint8_t* sha1);
#ifdef __cplusplus
}
#endif
