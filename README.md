# libchdman-rs

A Rust wrapper for the official MAME CHD core (`chd_file`).

This crate provides native support for reading, writing, and managing CHD (Compressed Hunks of Data) containers by wrapping the original MAME C++ source code. It ensures 100% feature parity with `chdman` by using the same core logic.

## Features

- **Native CHD Support**: Seek, read, and write at both hunk and byte levels.
- **Metadata Management**: Add, read, and delete CHD metadata.
- **Custom I/O**: Implement the `ChdIo` trait to provide your own I/O backends (e.g., in-memory, network, or custom encrypted filesystems).
- **Asynchronous Compression**: High-level `ChdCompressor` API for creating compressed CHDs from custom data sources.
- **Zero Re-implementation**: Directly wraps MAME's `libutil/chd.cpp`.

## Usage

### Opening a CHD

```rust
use libchdman_rs::Chd;

let chd = Chd::open("game.chd", false, None).expect("Failed to open CHD");
println!("Version: {}", chd.version());
println!("Logical bytes: {}", chd.logical_bytes());
```

### Custom I/O

```rust
use libchdman_rs::{Chd, ChdIo};
use std::io::{Read, Write, Seek, SeekFrom};

struct MyCustomIo { ... }
impl Read for MyCustomIo { ... }
impl Write for MyCustomIo { ... }
impl Seek for MyCustomIo { ... }
// ChdIo is automatically implemented for Read + Write + Seek

let io = MyCustomIo::new();
let chd = Chd::open_custom(io, false, None).expect("Failed to open");
```

## Build Requirements

- Rust 1.75+
- C++20 compliant compiler (GCC 10+, Clang 10+, MSVC 2019+)

## License

This crate is licensed under the same terms as MAME (BSD-3-Clause).
