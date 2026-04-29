# Build Architecture

`libchdman-rs` uses a multi-stage build process in `build.rs` to compile MAME's C++ core and its 3rdparty dependencies into a static library.

## Dependency Management

To avoid requiring system-level libraries like `libz`, `liblzma`, or `libflac`, the crate compiles MAME's internal copies found in `deps/mame/3rdparty`.

The following libraries are built:
- **zlib**: For general compression.
- **lzma**: For LZMA/LZMA2 compression.
- **zstd**: For Zstandard compression (ASM disabled for maximum compatibility).
- **flac**: For audio compression (includes SSE/Neon SIMD optimizations based on target architecture).
- **utf8proc**: For unicode handling.

## MAME Core Integration

The `cc` crate is configured to compile the necessary parts of `src/lib/util`. Key files include:
- `chd.cpp`: The main CHD logic.
- `chdcodec.cpp`: Compression/Decompression codecs.
- `hashing.cpp`: SHA1/MD5 implementations.
- `corefile.cpp`: MAME's internal I/O abstraction.

## Minimal OSD (Operating System Dependent) layer

MAME's core expects certain OSD functions to be present (e.g., for work queues and timing). Instead of linking against a heavy OSD implementation like SDL, this crate provides `sys/minimal_osd.cpp`, which implements the absolute minimum set of functions using standard C++ and OS primitives.

This allows the crate to remain a "pure library" with no UI or hardware dependencies.
