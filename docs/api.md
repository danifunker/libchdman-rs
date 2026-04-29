# Rust API Mapping

The goal of this crate is to provide an *idiomatic* Rust experience while staying true to MAME's performance.

## Resource Management

The `Chd` struct implements `Drop`. When a `Chd` goes out of scope, it automatically calls `chd_shim_close` and `chd_shim_free`, ensuring no memory leaks or dangling file handles.

## Error Handling

MAME's `chd_error` enum is mirrored in `src/sys.rs` and re-exported. All fallible operations return a `libchdman_rs::Result<T>`.

## I/O Traits

Instead of forcing users to use `std::fs::File`, the crate defines a `ChdIo` trait:

```rust
pub trait ChdIo: Read + Write + Seek {
    fn length(&mut self) -> std::io::Result<u64>;
}
```

This trait is automatically implemented for any type that satisfies `Read + Write + Seek`. This allows seamless integration with `File`, `Cursor<Vec<u8>>`, or custom wrappers.

## Asynchronous Operations

The `ChdCompressor` provides a high-level API for creating compressed CHDs. It uses a `ChdDataHandler` trait (callback-based) to feed data to the MAME compressor, matching MAME's own internal multi-threaded compression architecture.
