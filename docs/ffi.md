# FFI Bridge Design

MAME's `chd_file` is a heavy C++ class with templates and complex inheritance. To expose this to Rust safely, we use a "Shim" strategy.

## The Shim (`sys/chd_shim.cpp`)

The shim provides a flat `extern "C"` interface that handles:
1. **Opaque Handles**: Rust holds a pointer to an opaque `chd_file_t` struct, which the shim casts to `chd_file*`.
2. **Name Mangling**: C-style functions avoid C++ name mangling, making them easily callable from Rust.
3. **C++20 Compliance**: The shim is compiled as C++20, allowing it to interface with MAME's modern headers while exposing a stable ABI.
4. **Error Mapping**: MAME's `std::error_condition` is mapped to a stable `chd_error_t` (int32).

## Rust-to-C++ Callbacks (`RustRandomReadWrite`)

To support `ChdIo` (Rust-backed I/O), the shim implements a C++ class `RustRandomReadWrite` that inherits from MAME's `util::random_read_write`.

This class holds:
- A `void*` handle to the Rust object.
- A table of function pointers (`chd_rust_io_ops_t`) provided by the Rust side.

When MAME performs I/O on the CHD, the C++ shim calls the Rust function pointers, which in turn call the methods on the Rust trait.
