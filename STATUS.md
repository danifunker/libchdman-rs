# Current Implementation Status

Date: 2026-04-30
Version: 0.287.0-l1

## Completed
- [x] Full integration of MAME 0.287.0 CHD core.
- [x] Pure library build (Minimal OSD, no SDL/UI dependencies).
- [x] Support for all major codecs (zlib, lzma, zstd, flac).
- [x] Idiomatic Rust wrapper `Chd` with `Drop` support.
- [x] Custom Rust-backed I/O support via `ChdIo`.
- [x] Hunk and Byte-level reading/writing.
- [x] Metadata management (Read/Write/Delete).
- [x] Parent/Child (Diff) CHD support.
- [x] Asynchronous `ChdCompressor` wrapper.

## Verified (Tests Passing)
- [x] **Basic I/O**: Opening, Creating (uncompressed), Reading/Writing hunks and bytes.
- [x] **Metadata**: Creating, reading, and verifying metadata tags.
- [x] **Custom I/O**: Running a CHD entirely from a Rust-managed `Vec<u8>` via `open_custom`.
- [x] **Error Handling**: Correct mapping of MAME error codes to Rust results.

## chdman Feature Parity (in progress)
- [x] **M1**: Shared infrastructure
  - `ChdError::Cancelled` variant (Rust-only, never returned from FFI).
  - `CompressionProgress { bytes_done, bytes_total, ratio }` (replaces the old `{ err, progress, ratio }` shape).
  - `CompressStep::{Continue, Done}` returned by `ChdCompressor::compress_continue`.
  - `Chd::info() -> ChdInfo` aggregates header/codec/sha1/track-count/format-flag introspection in one walk.
  - Per-method introspection helpers (`is_hd`, `is_cd`, `is_dvd`, `compression(idx)`, `compression_codecs()`, etc.) removed in favour of `ChdInfo`.
  - `chdman_compat_tests` feature flag declared (dev-local only).
- [x] **M2**: Codec FourCC table + `parse_codec_spec` / `codec_name` / `codec_exists`.
  - Full FourCC set in `src/codec.rs` (zlib, zstd, lzma, huff, flac, cdzl, cdzs, cdlz, cdfl, avhuff).
  - `codec_exists` / `codec_name` thin wrappers over MAME's `chd_codec_list`.
  - `parse_codec_spec` mirrors chdman's `-c` syntax (`"none"` or 1..=4 comma-separated mnemonics).
- [x] **M3**: Streaming `Read` → `ChdCompressor` adapter.
  - `StreamingSource<R: Read>` adapts a Rust reader to MAME's pull-model `read_data`. Zero-pads past EOF, asserts monotonic offsets (verified against `chd_file_compressor::async_read` invariants), tolerates short reads from the underlying source.
  - `run_compression` drives `compress_begin`/`compress_continue`, samples the user `cancel` callback between iterations, and on cancel/error drops the compressor and unlinks the partial output file.
  - Crate-private (`pub(crate)`); the public surface comes through `hd::create_from_reader` etc. in M4+.
- [ ] **M4**: `hd` module (`createraw` / `extractraw`, GDDD metadata).
- [ ] **M5**: `dvd` module (`createdvd` / `extractdvd`, DVD metadata tag).
- [ ] **M6**: `cd` module (`createcd` / `extractcd`, CUE parser via FFI shim, ECC/EDC via FFI shim, CHT2 metadata).
- [ ] **M7**: `copy` module (re-compress with metadata pass-through).
- [ ] **M9**: Documentation pass (`docs/format-modules.md`, `docs/chdman-mapping.md`, README examples).

## Remaining Carry-Over
- [ ] **SIMD Optimization Verification**: Verify that the FLAC SIMD optimizations are correctly used on all target platforms.
- [ ] **Documentation Examples**: Add more complex examples (like Diff CHDs) to the README.

## Instructions for Continuation
To run the existing tests:
```bash
cargo test
```

The core logic resides in:
- `sys/chd_shim.cpp`: The FFI bridge.
- `src/lib.rs`: The high-level Rust API.
- `build.rs`: The complex build logic for MAME dependencies.
