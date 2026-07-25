# FFI Bridge Design

MAME's `chd_file` is a heavy C++ class with templates and complex inheritance. To expose this to Rust safely, we use a "Shim" strategy.

## The Shim (`sys/chd_shim.cpp`)

The shim provides a flat `extern "C"` interface that handles:
1. **Opaque Handles**: Rust holds a pointer to an opaque `chd_file_t` struct, which the shim casts to `chd_file*`.
2. **Name Mangling**: C-style functions avoid C++ name mangling, making them easily callable from Rust.
3. **C++20 Compliance**: The shim is compiled as C++20, allowing it to interface with MAME's modern headers while exposing a stable ABI.
4. **Error Mapping**: MAME's `std::error_condition` is mapped to a stable `chd_error_t` (int32).
5. **Exception Containment**: no exception may cross back into Rust.

## Exception Containment (`sys/shim_guard.h`)

Rust frames cannot unwind a foreign exception — one that escapes an
`extern "C"` boundary aborts the process outright ("fatal runtime error:
Rust cannot catch foreign exceptions"). MAME throws in several places:
`cdrom_file`'s constructors signal bad input with a bare `throw nullptr`
(`cdrom.cpp:140,156,162,253,255,260`), and anything allocating can throw
`std::bad_alloc`.

So **every** `extern "C"` function in `sys/` routes its body through one of
two helpers:

```cpp
chd_error_t chd_shim_foo(...) {
    return chd_shim::guard([&] { /* body */ }, SHIM_ERR_EXCEPTION);
}

void chd_shim_bar(...) {
    chd_shim::guard_void([&] { /* body */ });
}
```

The fallback is what the entry point returns when the body threw: `nullptr`
for handle-returning functions, `0` for counts/booleans,
`SHIM_ERR_EXCEPTION` (MAME's `INVALID_FILE`, the same code `to_chd_error`
uses for anything outside `chd_category`) for `chd_error_t`. Because
`throw nullptr` carries no payload, `catch (...)` is the only option and the
fallback is all the diagnosis available — where the reason is knowable up
front, the Rust side pre-screens instead (see `cd::Cdrom::open`, which
checks `unit_bytes()` so it can report `ChdError::NotCdMedia` rather than a
generic failure).

A new entry point is exception-safe by construction if it follows the same
shape; `grep guard sys/*.cpp` shows the coverage.

## Rust-to-C++ Callbacks (`RustRandomReadWrite`)

To support `ChdIo` (Rust-backed I/O), the shim implements a C++ class `RustRandomReadWrite` that inherits from MAME's `util::random_read_write`.

This class holds:
- A `void*` handle to the Rust object.
- A table of function pointers (`chd_rust_io_ops_t`) provided by the Rust side.

When MAME performs I/O on the CHD, the C++ shim calls the Rust function pointers, which in turn call the methods on the Rust trait.
