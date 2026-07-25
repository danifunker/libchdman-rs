# Changelog

Notable changes per release. Versions track the embedded MAME release plus
a patch counter (`0.288.<N>`) — see the Versioning section of `RELEASING.md`.
Releases before `0.288.10` are documented only in their GitHub Release
notes and git history.

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
