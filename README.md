# libchdman-rs

A Rust wrapper for the official MAME CHD core (`chd_file`).

This crate provides native support for reading, writing, and managing CHD (Compressed Hunks of Data) containers by wrapping the original MAME C++ source code. It ensures 100% feature parity with `chdman` by using the same core logic.

## Features

- **Native CHD Support**: Seek, read, and write at both hunk and byte levels.
- **Metadata Management**: Add, read, and delete CHD metadata.
- **Custom I/O**: Implement the `ChdIo` trait to provide your own I/O backends (e.g., in-memory, network, or custom encrypted filesystems).
- **Asynchronous Compression**: High-level `ChdCompressor` API for creating compressed CHDs from custom data sources.
- **chdman parity**: `hd`, `dvd`, `cd`, and `copy` modules cover `createhd`/`extracthd`, `createdvd`/`extractdvd`, `createcd`/`extractcd`, and `copy` byte-for-byte.
- **Zero Re-implementation**: Directly wraps MAME's `libutil/chd.cpp`.

For the full API surface and a chdman-flag-by-flag mapping see
[`docs/format-modules.md`](docs/format-modules.md) and
[`docs/chdman-mapping.md`](docs/chdman-mapping.md).

## Usage

### Opening a CHD

```rust
use libchdman_rs::Chd;

let chd = Chd::open("game.chd", false, None).expect("Failed to open CHD");
let info = chd.info().expect("info");
println!("Version: {}, logical bytes: {}", info.version, info.logical_bytes);
println!("CD: {}, DVD: {}, HD: {}", info.is_cd, info.is_dvd, info.is_hd);
```

### Creating a hard-disk CHD

```rust
use libchdman_rs::hd::{create_from_path, HdCreateOptions};
use std::path::Path;

create_from_path(
    Path::new("disk.img"),
    Path::new("disk.chd"),
    HdCreateOptions::default(),
    &mut |p| eprintln!("{}/{}", p.bytes_done, p.bytes_total),
    &|| false,
)?;
```

### Creating a CD CHD from a CUE

```rust
use libchdman_rs::cd::{create_from_cue, CdCreateOptions};
use libchdman_rs::parse_codec_spec;
use std::path::Path;

create_from_cue(
    Path::new("game.cue"),
    Path::new("game.chd"),
    CdCreateOptions {
        codecs: parse_codec_spec("cdlz,cdfl,cdzl")?,
        ..CdCreateOptions::default()
    },
    &mut |_| {},
    &|| false,
)?;
```

### Re-compressing an existing CHD

```rust
use libchdman_rs::copy::{copy, CopyOptions};
use libchdman_rs::{CHD_CODEC_LZMA, CHD_CODEC_ZLIB};
use std::path::Path;

copy(
    Path::new("old.chd"),
    Path::new("new.chd"),
    CopyOptions {
        hunk_size: None,
        codecs: [CHD_CODEC_LZMA, CHD_CODEC_ZLIB, 0, 0],
    },
    &mut |_| {},
    &|| false,
)?;
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

## Versioning

The crate version tracks the MAME release it embeds, with a `-lN` suffix
for libchdman-rs releases against that MAME version. For example,
`0.287.0-l1` is the first libchdman-rs release built on MAME 0.287.0.

## Testing

`cargo test` runs the lib-only suite (round-trip parity, metadata,
custom I/O). It never invokes the `chdman` binary and has no system
dependencies beyond a C++20 toolchain.

For byte-for-byte parity verification against the upstream `chdman`
tool, enable the `chdman_compat_tests` feature locally:

```bash
cargo test --features chdman_compat_tests
```

This is **dev-local only**. The feature gates tests that shell out to
`chdman` on `$PATH`; CI does not install chdman, so these tests never
run there. Use them on demand during development cycles when you want
to confirm that the output of this crate matches what chdman produces
for the same input.

## Build Requirements

- Rust 1.75+
- C++20 compliant compiler (GCC 10+, Clang 10+, MSVC 2019+)

## License

This crate is licensed under the same terms as MAME (BSD-3-Clause).
