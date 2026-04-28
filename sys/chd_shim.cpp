#include "chd_shim.h"
#include "chd.h"
#include "ioprocs.h"
#include <memory>
#include <string_view>
#include <system_error>

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
            case SEEK_SET: new_offset = offset; break;
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

static chd_error_t to_chd_error(std::error_condition err) {
    if (!err) return 0;
    if (err.category() == chd_category()) return (chd_error_t)err.value();
    // Map other errors to a generic INVALID_FILE or similar if they are not CHD errors
    return (chd_error_t)chd_file::error::INVALID_FILE;
}

extern "C" {

chd_file_t* chd_shim_alloc() {
    return reinterpret_cast<chd_file_t*>(new chd_file());
}

void chd_shim_free(chd_file_t* chd) {
    delete reinterpret_cast<chd_file*>(chd);
}

chd_error_t chd_shim_open_file(chd_file_t* chd, const char* filename, int writeable, chd_file_t* parent) {
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->open(filename, writeable != 0, reinterpret_cast<chd_file*>(parent)));
}

chd_error_t chd_shim_open_custom(chd_file_t* chd, chd_rust_io_t handle, chd_rust_io_ops_t ops, int writeable, chd_file_t* parent) {
    auto io = std::make_unique<RustRandomReadWrite>(handle, ops);
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->open(std::move(io), writeable != 0, reinterpret_cast<chd_file*>(parent)));
}

chd_error_t chd_shim_create_file(chd_file_t* chd, const char* filename, uint64_t logicalbytes, uint32_t hunkbytes, uint32_t unitbytes, const uint32_t compression[4]) {
    chd_codec_type comp[4];
    for (int i = 0; i < 4; i++) comp[i] = compression[i];
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->create(filename, logicalbytes, hunkbytes, unitbytes, comp));
}

void chd_shim_close(chd_file_t* chd) {
    reinterpret_cast<chd_file*>(chd)->close();
}

uint32_t chd_shim_version(chd_file_t* chd) {
    return reinterpret_cast<chd_file*>(chd)->version();
}

uint32_t chd_shim_hunk_bytes(chd_file_t* chd) {
    return reinterpret_cast<chd_file*>(chd)->hunk_bytes();
}

uint32_t chd_shim_hunk_count(chd_file_t* chd) {
    return reinterpret_cast<chd_file*>(chd)->hunk_count();
}

uint32_t chd_shim_unit_bytes(chd_file_t* chd) {
    return reinterpret_cast<chd_file*>(chd)->unit_bytes();
}

uint64_t chd_shim_unit_count(chd_file_t* chd) {
    return reinterpret_cast<chd_file*>(chd)->unit_count();
}

uint64_t chd_shim_logical_bytes(chd_file_t* chd) {
    return reinterpret_cast<chd_file*>(chd)->logical_bytes();
}

chd_error_t chd_shim_read_hunk(chd_file_t* chd, uint32_t hunknum, void* buffer) {
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->read_hunk(hunknum, buffer));
}

chd_error_t chd_shim_write_hunk(chd_file_t* chd, uint32_t hunknum, const void* buffer) {
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->write_hunk(hunknum, buffer));
}

chd_error_t chd_shim_read_bytes(chd_file_t* chd, uint64_t offset, void* buffer, uint32_t bytes) {
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->read_bytes(offset, buffer, bytes));
}

chd_error_t chd_shim_write_bytes(chd_file_t* chd, uint64_t offset, const void* buffer, uint32_t bytes) {
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->write_bytes(offset, buffer, bytes));
}

chd_error_t chd_shim_read_metadata(chd_file_t* chd, uint32_t tag, uint32_t index, void* buffer, uint32_t buffer_len, uint32_t* result_len) {
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->read_metadata(tag, index, buffer, buffer_len, *result_len));
}

chd_error_t chd_shim_write_metadata(chd_file_t* chd, uint32_t tag, uint32_t index, const void* buffer, uint32_t length, uint8_t flags) {
    return to_chd_error(reinterpret_cast<chd_file*>(chd)->write_metadata(tag, index, buffer, length, flags));
}

}
