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
