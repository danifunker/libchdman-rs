# Implementation Logic

This document details the internal logic of how `libchdman-rs` maps MAME's C++ concepts to Rust.

## Hunk-Based I/O

MAME's CHD format is hunk-based. A hunk is a fixed-size block of data (e.g., 4096 bytes).
- `read_hunk` and `write_hunk` operate directly on these blocks.
- `read_bytes` and `write_bytes` are convenience methods that handle reading/writing across hunk boundaries.

The logic in `libchdman-rs` simply wraps MAME's `read_hunk`/`write_hunk` calls, passing a pointer to a Rust-allocated buffer.

## Metadata Management

Metadata in a CHD is identified by a 4-character tag (e.g., 'CHAL') and an index.
- `write_metadata` allows adding or updating metadata.
- `read_metadata` retrieves it.
- `delete_metadata` removes it.

MAME's core implementation handles the dynamic resizing of the metadata area within the CHD file.

## Compression Logic

The `ChdCompressor` uses MAME's `chd_file_compressor` class.
1. `begin()` initializes the compression process.
2. `continue()` is called repeatedly. MAME's core manages its own thread pool for compression.
3. The `ChdDataHandler` trait in Rust is used by the C++ shim to request data from the source (e.g., a raw disk image) as the compressor needs it.

## Verification Logic

The `verify()` method performs a full SHA1 check of the logical data in the CHD.
- It iterates through the logical extent of the CHD.
- It reads data in chunks.
- It feeds these chunks into a `Sha1Creator` (wrapped `util::sha1_creator`).
- Finally, it compares the computed SHA1 against the one stored in the CHD header.
