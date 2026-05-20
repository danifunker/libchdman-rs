# Maintainer build & release notes

## Cutting a new release

libchdman-rs versions track MAME's roughly bi-monthly release cadence.
The release flow is **dispatch-then-tag**: no tag is pushed manually.
The workflow only creates the tag after every build cell succeeds, so
a failed build leaves no tag and no release behind.

1. Update the MAME submodule (`deps/mame`) to the new MAME version.
2. Bump `version` in `Cargo.toml`. The version becomes the release tag,
   so use something parseable like `0.287.0-l3` (the `-l3` suffix is the
   libchdman-rs revision against that MAME version).
3. Update notes/changelog with anything that affects the wrapper.
4. Commit and push the branch (no tag yet):
   ```bash
   git commit -am "Bump to 0.287.0-l3 (MAME 0.287)"
   git push origin main
   ```
5. In the GitHub UI, go to **Actions → "Build prebuilt static archives"
   → Run workflow**, select the branch, and enter the tag name
   (e.g. `v0.287.0-l3`). The workflow:
   - **Preflight** validates: tag starts with `v`, `Cargo.toml` version
     matches the tag, and the tag/release doesn't already exist. Bails
     out fast if any of those fail.
   - **Build** runs all 10 target/glibc cells in parallel (~20 min).
   - **Release** runs only if every build cell succeeded; it creates
     the tag at the current commit and publishes the release with all
     assets attached.
6. After the workflow finishes, verify the release page contains every
   expected asset (see below).

If the workflow fails before the release job, nothing is published and
no tag is created. Fix the issue, push the fix to the branch, and run
the workflow again with the same tag input.

## What the workflow builds

For each tag, the workflow produces 10 archive files plus 10 `.sha256`
sidecars:

- 6 Linux archives: `(x86_64, aarch64) × (glibc2.31, glibc2.35, glibc2.39)`
- 2 macOS archives: `x86_64-apple-darwin`, `aarch64-apple-darwin`
- 2 Windows archives: `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`

Each archive is a single fat static library — the workflow merges the six
component `.a` / `.lib` files that `build.rs` produces (chd_shim plus the
five third-party static libs: lzma, zlib, utf8proc, zstd, flac) into one
artifact named `libchdman_rs.*` so consumers see and link a single file.

If any matrix cell fails, the release job still publishes the assets
from successful cells. **Rerun the failed cells via `workflow_dispatch`
with the same tag** rather than retagging.

## Rebuilding a single asset

If one archive turns out broken after release (rare — usually means a
runner ran out of disk or a transient build break):

1. Trigger `release-prebuilt.yml` via `workflow_dispatch` with the
   target tag as input.
2. The workflow builds everything again and re-uploads. The release
   action overwrites existing assets with the same name.

For a faster rebuild, comment out the matrix entries you don't need to
rebuild and dispatch again.

## Local validation before tagging

Before pushing a tag, run the validation script against a local build:

```bash
cargo build --release
# The build produces six separate component .a files in OUT_DIR; you
# can validate the main chd_shim archive directly, or merge first.
scripts/validate-archive.sh \
  target/release/build/libchdman-rs-*/out/libchd_shim.a
```

This catches obvious breakage (missing symbols, wrong target ABI)
before you commit to a release. The workflow also runs the script
against every merged artifact before upload.

## What changes when MAME ships a new version

- Update vendored MAME sources to the new version.
- The C++ build will likely break in small ways — fix the wrapper
  shims in `sys/chd_shim.cpp` / `sys/cd_shim.cpp`.
- Run the full test suite (`cargo test`) and the validation script.
- Bump version and tag as above.

## Glibc matrix changes

When a new glibc version becomes mainstream (e.g. Debian 13 ships with
a newer glibc), add a matrix entry to `release-prebuilt.yml` rather than
removing an existing one. Existing entries stay supported as long as
the Ubuntu runner image is supported by GitHub. Update the README table
to match. Also widen the accepted-value list in `build.rs::try_use_prebuilt`
(`LIBCHDMAN_GLIBC` validation) when adding a new floor.
