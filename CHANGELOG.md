# Changelog

Notable changes per release. Versions track the embedded MAME release plus
a patch counter (`0.<mame>.<N>`) — see the Versioning section of `RELEASING.md`.
Releases before `0.288.10` are documented only in their GitHub Release
notes and git history.

## 0.289.3

### Added

- **`cd::CdAudioReader`: stream raw CD sectors out of a CD CHD, no
  extraction.** Reads 2352-byte sectors on demand, byte-identical to the
  `.bin` that `cd::extract_to_cue` writes: audio byte-swapped to
  little-endian, subcode dropped. Players can start a track and seek
  without unpacking the disc to a temp BIN first.
  - `open(chd)`: the whole disc in the BIN's address space (sector `N` is
    bytes `[N*2352 .. (N+1)*2352)` of the `.bin`).
  - `open_track(chd, index)`: one track; sector 0 is its first frame.
  - `sector_count`, `len`, `is_empty`, `read_sectors(start, count, buf)`,
    `into_inner`, plus `Read + Seek`. `Send`; reads take `&mut self`.
  - Raw (2352-byte) data tracks are readable too. A cooked track
    (`MODE1/2048`, ...) returns `UnsupportedFormat`. A non-CD CHD returns
    `NotCdMedia`. An out-of-range track or sector range returns
    `InvalidData`.

### Changed

- `extract_to_cue` and `extract_to_gdi` now share one internal frame
  mapping (physical track start, `padframes` and `splitframes`, audio
  byte-swap) with `CdAudioReader`. Their output is unchanged.

## 0.289.2

### Fixed

- **`ChdCompressor` leaked its data handler.** `ChdCompressor::new` boxed
  the handler and passed it to C++, and nothing ever freed it. Any files
  the handler owned stayed open after compression. On Windows that blocked
  renaming or deleting them later ("Access is denied"). The handler is now
  dropped with the compressor.

## 0.289.1

### Added

- **Metadata during CHD creation.** `cd::create_from_cue_with_metadata` /
  `create_from_iso_with_metadata` write caller-supplied metadata records
  while the CHD is created; the existing functions keep their signatures
  and delegate to them. Existing prebuilt archives still link: the shim
  already exported `chd_shim_write_metadata`.
- **`LIBCHDMAN_CPU=portable`** (source builds) compiles FLAC as generic C
  only: no `-m` flags, no intrinsics. Use it for x86_64 CPUs without
  AVX2: on GCC/Clang the default flags let the compiler auto-vectorize
  FLAC's unguarded generic code into AVX2. Unset (the default) is
  unchanged, so release archives are byte-identical.
- `scripts/osd-queue-harness.sh`: runs the work queue under
  ThreadSanitizer or AddressSanitizer (Linux, macOS), plus a
  `bench` mode.

### Changed

- **Source builds now work from the crates.io package, offline
  included.** The subset of MAME that `build.rs` compiles (the CHD core
  from `src/lib/util` plus the bundled zlib, LZMA SDK, zstd, FLAC and
  utf8proc, ~9 MB, 1.5 MiB compressed) is vendored unmodified in
  `mame-chd-src/` and shipped in the package, along with the `sys/`
  shims, which were previously missing too. Before this, a build without
  the `prebuilt` feature aborted unless you depended on the git repo,
  and there was no way to build in a network-less sandbox such as distro
  packaging (techomancer/iris#213). Now `cargo vendor` followed by
  `LIBCHDMAN_FORCE_SOURCE=1 cargo build --offline` works.
- `mame-chd-src/MAME_VERSION` records the upstream MAME version, tag
  and commit. `build.rs` refuses to build if it doesn't match the crate
  version.
- New maintainer script `scripts/vendor-mame.sh` regenerates
  `mame-chd-src/` from a MAME checkout (default: the `deps/mame`
  submodule). Its file list comes from `build.rs`.
- **Compression uses up to 16 worker threads, not 4.** The array a worker's 
  `threadid` indexes is `m_codecs[WORK_MAX_THREADS]`, and `osdcore.h` defines
  `WORK_MAX_THREADS` as 16 (unchanged since at least 0.150). The shim now caps 
  at that constant, like MAME's own pool. `OSDWORKQUEUEMAXTHREADS` lowers the
  count, as in MAME.

### Fixed

- **Use-after-free in the work queue.** `chd_file_compressor` releases a
  work item once the callback marks the hunk complete, which can be before
  the worker has finished with it. `osd_work_item_release` now waits for
  the worker, as MAME's does. `completed` is also now atomic.
- **The thread waiting on compression no longer spins.** The wait functions
  ignored their timeout and looped on `yield()`; they now block on a
  condition variable and honour the timeout.
- Less lock contention: work items are allocated outside the queue mutex,
  and queuing one item wakes one worker instead of all of them.

## 0.289.0

### Changed

- **MAME bumped to 0.289** (`deps/mame` → tag `mame0289`). No wrapper
  changes were required.

  The CHD core is functionally unchanged in this release. `chd.cpp`,
  `chdcodec.cpp`, `cdrom.cpp`, `avhuff.cpp`, and `huffman.cpp` differ
  from 0.288 only in their `#include` blocks — adding `<cstring>` and
  dropping `coretmpl.h`. Every CHD header we compile against
  (`chd.h`, `chdcodec.h`, `cdrom.h`, `hashing.h`, `avhuff.h`,
  `flac.h`, `huffman.h`) is byte-identical to 0.288, the on-disk
  format is still `HEADER_VERSION = 5`, and no codec was added or
  removed. Both shims build unmodified and all 68 tests pass.

- **Vendored zlib 1.3.1 → 1.3.2**, pulled in by the MAME bump. A patch
  release: no source files were added to or removed from the set
  `build.rs` compiles, and there is no API change.

### Notes

- MAME 0.289 adds four Quantum Fireball hard-disk templates to the
  `chdman` CLI's `listtemplates` table, and widens that command's size
  calculation to 64-bit so the new >4 GB entries print correctly. This
  crate does not build `src/tools/chdman.cpp` and does not mirror the
  template table — `hd::create_from_path` / `hd::create_from_reader`
  take geometry directly — so there is nothing to sync. Exposing named
  templates remains a possible future addition, not a compatibility gap.

## 0.288.11

### Added

- **iOS prebuilt targets**: `aarch64-apple-ios` (device) and
  `aarch64-apple-ios-sim` (simulator on Apple Silicon). The `prebuilt`
  feature now resolves an archive for both, bringing the release set to 14
  archives / 28 assets.

  No C++ changes were needed — MAME's CHD core, both shims, and FLAC
  (including its NEON intrinsics) compile clean for iOS as-is. The library
  is filesystem-only: no `fork`/`exec`/`dlopen`, and its compression worker
  pool was already capped at 4 threads, which suits iOS's memory ceiling.
  File access is subject to the app sandbox.

  Source builds for iOS already worked before this release; what was
  missing was the published archive and the target detection to find it.
  `build.rs` previously matched only `apple-darwin` and rejected iOS
  triples with "no prebuilt archive published for target".

  CI caveat, by nature rather than by choice: an iOS *device* binary cannot
  be executed on a CI runner, so that archive is validated by build,
  required-symbol check, and a Mach-O platform check. Execution is proven
  by the simulator archive, built from the same sources with the same
  compiler. See `RELEASING.md`.

- **iOS coverage in `ci.yml`**, so an iOS break surfaces on the PR that
  causes it rather than when a release is dispatched. The device target is
  built and linked; the simulator target runs the **entire test suite** as
  iOS binaries via `xcrun simctl spawn` — CHD create/extract round-trips
  included, all 68 tests, matching the host count.

## 0.288.10

### Fixed

- **A non-CD CHD no longer aborts the process.** Calling any `cd::*`
  function on a CHD without CD track metadata — an ordinary hard-disk or
  DVD image — used to kill the host process outright:

  ```
  libc++abi: terminating due to uncaught exception of type std::nullptr_t
  fatal runtime error: Rust cannot catch foreign exceptions, aborting
  ```

  MAME's `cdrom_file` constructor reports bad input by throwing a bare
  `nullptr` (`cdrom.cpp:253-260`); a hard-disk CHD has `unit_bytes() == 512`
  against the required 2448 and trips the first check. Rust frames cannot
  unwind a foreign exception, so the abort happened before any caller got a
  chance to handle it. These calls now return an ordinary `Err`.

  **Behavioural change worth pinning to**: consumers that pre-screened with
  `Chd::info().is_cd` purely to avoid the abort can now just call and match.

- Every `extern "C"` entry point in `sys/chd_shim.cpp` and `sys/cd_shim.cpp`
  is now exception-safe, not just the one that was reported. Bodies route
  through `chd_shim::guard` / `guard_void` (`sys/shim_guard.h`), which
  catches and returns a documented fallback (`nullptr`, `0`, or MAME's
  `INVALID_FILE`). See `docs/ffi.md`.

- `build.rs` now emits `rerun-if-changed` for `sys/cd_shim.cpp` and
  `sys/shim_guard.h`. Editing `cd_shim.cpp` previously did not trigger a
  rebuild, so shim changes could silently not apply.

### Added

- `ChdError::NotCdMedia` — returned by `cd::list_tracks`,
  `cd::extract_to_cue`, `cd::extract_to_iso`, `cd::extract_to_gdi`, and
  `CdCookedReader::open`/`open_track` when the CHD isn't CD/GD-ROM media.
  Distinct from `ChdError::InvalidData`, which those calls still return when
  the geometry *is* CD-shaped but the track metadata is missing or
  unparseable. Like `ChdError::Cancelled` it is Rust-only and never produced
  over FFI.

  This is an added enum variant: exhaustive `match` over `ChdError` needs a
  new arm. No function signatures changed.
