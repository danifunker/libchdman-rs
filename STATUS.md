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
- [x] **M4**: `hd` module — `createhd`/`extracthd` parity with optional `createraw`-style payloads.
  - `HdCreateOptions` (logical_size, hunk_size=4096, unit_size=512, codecs=[ZLIB,0,0,0], optional geometry, optional ident).
  - `compute_chs` ports chdman's `guess_chs` heuristic exactly.
  - `format_gddd` / `read_geometry` round-trip MAME's `"CYLS:%d,HEADS:%d,SECS:%d,BPS:%d"` format.
  - `create_from_reader` / `create_from_path` stream input through `StreamingSource`, write GDDD + optional IDNT before workers spin up.
  - `extract_to_writer` / `extract_to_path` stream the logical bytes back out hunk-by-hunk.
  - **Important**: `run_compression` was changed to drive the compressor to completion even on cancel (then unlink), avoiding the C++ vtable race during `~RustChdCompressor` when worker threads are still mid-flight. Documented in `streaming.rs`.
  - 8 integration tests cover compute_chs, GDDD format, codec matrix (zlib/zstd/lzma), zero-padding short reader, IDNT writes, cancel-deletes-file, and a real-ISO round-trip from the checked-in fixture.
- [x] **M5**: `dvd` module — `createdvd` / `extractdvd` parity (MAME 0.287+).
  - `DvdCreateOptions` (default hunk `2 * 2048` and codecs `[LZMA, ZLIB, HUFF, FLAC]` — chdman's `s_default_hd_compression` reused verbatim per chdman.cpp:2263).
  - `create_from_reader` / `create_from_iso` stream into a CHD with the empty `DVD ` metadata record. **Subtle bug found**: chdman's `chd->write_metadata(DVD_METADATA_TAG, 0, "")` invokes the std::string overload (chd.h:351), which stores `length + 1` bytes — i.e. one NUL byte, not zero bytes. The shim takes raw `(ptr, len)` so we must explicitly pass a 1-byte zero buffer. Documented inline.
  - `extract_to_writer` / `extract_to_iso` reject non-DVD CHDs with `UnsupportedFormat`.
  - 5 tests: real DVD ISO fixture round-trip (2.3 MiB), 2048-alignment validation, multi-codec sweep `[LZMA]`/`[ZSTD]`/`[ZLIB]`/`[NONE;4]`, extract refuses HD CHD, cancellation deletes partial output.
- [x] **M6**: `cd` module — chdman `createcd` / `extractcd` parity.
  - **M6a CUE parser via FFI**: `chd_shim_toc_*` wraps `cdrom_file::parse_toc` (handles CUE, GDI, ISO, Nero TOC). No CUE logic reimplemented in Rust.
  - **M6b–c metadata + ECC/EDC**: `chd_shim_cd_write_metadata` calls `cdrom_file::write_metadata` (writes CHT2). ECC/EDC, audio byte-swap, track padding all happen inside MAME's `cdrom_file` and the ported `chd_cd_compressor`.
  - **M6d–e sector pipeline**: `sys/cd_shim.cpp` ports chdman's `chd_cd_compressor` near-verbatim (chdman.cpp:419) — track lookup, byte-swap, split-bin handling. Same code path = byte-for-byte parity.
  - **M6f public API**: `cd::create_from_cue`, `cd::create_from_iso` (synthesizes a temp CUE next to the ISO), `cd::list_tracks`, `cd::extract_to_cue` (combined `.bin` + emitted `.cue`, audio swapped back to little-endian), `cd::extract_to_iso` (single MODE1/MODE1_RAW track only; uses MAME's `read_data` with `CD_TRACK_MODE1` to strip 2352→2048 inside the shim).
  - 7 tests: 2-track BIN/CUE round-trip through create + `list_tracks`, ISO `create_from_iso`, codec matrix sweep over `[CDLZ,CDZL]`/`[CDFL,CDZL]`/`[CDZS,CDZL]`/`[NONE;4]`, cancellation deletes partial output, `extract_to_iso` byte-exact round-trip from the ISO fixture, `extract_to_iso` rejects multi-track CHDs, `extract_to_cue` byte-exact round-trip with the 2-track BIN/CUE fixture.
- [x] **M7**: `copy` module — chdman `copy` parity.
  - `CopyOptions { hunk_size: Option<u32>, codecs: [u32; 4] }`. None for hunk_size preserves the source's value.
  - `copy(source, dest, opts, progress, cancel)` opens the source, snapshots all metadata, allocates a fresh compressor whose `ChdDataHandler` reads from the source via `read_bytes`, clones every metadata record with MAME's append index (`CHDMETAINDEX_APPEND = ~0u32`), and runs compression to completion.
  - 4 tests: HD codec change preserves raw_sha1 + GDDD/IDNT, HD hunk_size change with byte-exact extract, DVD uncompressed → LZMA preserves the `DVD ` tag and round-trips, CD copy preserves track metadata across `[CDLZ,CDZL]` → `[CDFL,CDZL]` re-compression.
- [x] **M9**: Documentation pass — `docs/format-modules.md` (per-module API + examples), `docs/chdman-mapping.md` (subcommand + flag mapping), README examples for hd/cd/copy plus `Chd::info` snippet, links from README to both docs.

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
