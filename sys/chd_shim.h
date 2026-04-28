#ifndef CHD_SHIM_H
#define CHD_SHIM_H

#include <cstdint>
#include <cstddef>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct chd_file chd_file_t;

typedef int32_t chd_error_t;

// Opaque handle for Rust-backed I/O
typedef void* chd_rust_io_t;

typedef struct {
    int (*read)(chd_rust_io_t handle, uint64_t offset, void* buffer, uint32_t length, uint32_t* actual);
    int (*write)(chd_rust_io_t handle, uint64_t offset, const void* buffer, uint32_t length, uint32_t* actual);
    int (*length)(chd_rust_io_t handle, uint64_t* result);
    void (*close)(chd_rust_io_t handle);
} chd_rust_io_ops_t;

chd_file_t* chd_shim_alloc();
void chd_shim_free(chd_file_t* chd);

chd_error_t chd_shim_open_file(chd_file_t* chd, const char* filename, int writeable, chd_file_t* parent);
chd_error_t chd_shim_open_custom(chd_file_t* chd, chd_rust_io_t handle, chd_rust_io_ops_t ops, int writeable, chd_file_t* parent);

chd_error_t chd_shim_create_file(chd_file_t* chd, const char* filename, uint64_t logicalbytes, uint32_t hunkbytes, uint32_t unitbytes, const uint32_t compression[4]);

void chd_shim_close(chd_file_t* chd);

uint32_t chd_shim_version(chd_file_t* chd);
uint32_t chd_shim_hunk_bytes(chd_file_t* chd);
uint32_t chd_shim_hunk_count(chd_file_t* chd);
uint32_t chd_shim_unit_bytes(chd_file_t* chd);
uint64_t chd_shim_unit_count(chd_file_t* chd);
uint64_t chd_shim_logical_bytes(chd_file_t* chd);

chd_error_t chd_shim_read_hunk(chd_file_t* chd, uint32_t hunknum, void* buffer);
chd_error_t chd_shim_write_hunk(chd_file_t* chd, uint32_t hunknum, const void* buffer);

chd_error_t chd_shim_read_bytes(chd_file_t* chd, uint64_t offset, void* buffer, uint32_t bytes);
chd_error_t chd_shim_write_bytes(chd_file_t* chd, uint64_t offset, const void* buffer, uint32_t bytes);

chd_error_t chd_shim_read_metadata(chd_file_t* chd, uint32_t tag, uint32_t index, void* buffer, uint32_t buffer_len, uint32_t* result_len);
chd_error_t chd_shim_write_metadata(chd_file_t* chd, uint32_t tag, uint32_t index, const void* buffer, uint32_t length, uint8_t flags);

#ifdef __cplusplus
}
#endif

#endif
