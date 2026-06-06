# Maintainer build & release notes

## Cutting a new release

libchdman-rs versions track MAME's roughly bi-monthly release cadence.
The release flow is **dispatch-then-tag**: no tag is pushed manually.
The workflow only creates the tag after every build cell succeeds, so
a failed build leaves no tag and no release behind.

1. Update the MAME submodule (`deps/mame`) to the new MAME version.
2. Bump `version` in `Cargo.toml` to match the MAME version (e.g.
   `0.288.0` embeds MAME 0.288). The version becomes the release tag.
   For a wrapper-only fix against the same MAME version, bump the patch
   component (`0.288.1`, `0.288.2`, ...).
3. Update notes/changelog with anything that affects the wrapper.
4. Commit and push the branch (no tag yet):
   ```bash
   git commit -am "Bump to 0.288.0 (MAME 0.288)"
   git push origin main
   ```
5. In the GitHub UI, go to **Actions → "Build prebuilt static archives"
   → Run workflow**, select the branch, and enter the tag name
   (e.g. `v0.288.0`). The workflow:
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

For each tag, the workflow produces 9 archive files plus 9 `.sha256`
sidecars:

- 4 Linux x86_64/aarch64 archives: `(x86_64, aarch64) × (glibc2.35, glibc2.39)`
- 1 Linux armv7 archive: `armv7-unknown-linux-gnueabihf-glibc2.31`
  (cross-compiled in an `ubuntu:20.04` container; targets MiSTer /
  Cyclone V Cortex-A9 systems running glibc 2.31)
- 2 macOS archives: `x86_64-apple-darwin` (on `macos-15-intel`),
  `aarch64-apple-darwin` (on `macos-latest`)
- 2 Windows archives: `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`

The glibc2.31 floor for x86_64/aarch64 was retired when GitHub deprecated
the `ubuntu-20.04` runner image. For armv7, glibc2.31 is still produced by
cross-compiling inside an `ubuntu:20.04` Docker container (no native runner
needed), which carries the correct ARM sysroot.

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

## Publishing to crates.io

libchdman-rs ships to crates.io in a **slim form** — only the Rust wrapper
code, no vendored MAME C++. Consumers who use the `prebuilt` feature get
a ~55 KB tarball instead of cloning the ~1 GB git repo.

### One-time setup

1. Generate a crates.io API token at <https://crates.io/me>.
   - Scope: `publish-update` is enough after the first version is live.
   - The very first publish needs `publish-new` (only once for this crate name).
2. Add the token as a repo Actions secret named `CARGO_REGISTRY_TOKEN`:
   ```bash
   gh secret set CARGO_REGISTRY_TOKEN
   ```

### Release flow (do this for every version)

The crates.io publish is **chained after the prebuilt release**, not
parallel to it. Order:

1. Bump `version` in `Cargo.toml`, commit, push to `main`.
2. Dispatch `Build prebuilt static archives` for the new tag (e.g.
   `v0.288.0`). Wait for it to finish — this is what actually
   creates the git tag and the GitHub Release with all 16 assets.
3. Dispatch `Publish to crates.io` with the same tag. The preflight
   job confirms the release exists with 16 assets, validates the
   slim tarball contents (no C++, < 2 MB), then `cargo publish`s.
4. Verify on <https://crates.io/crates/libchdman-rs>.

```bash
# Dispatch the prebuilt build:
gh workflow run release-prebuilt.yml --ref main -f tag=v0.288.0

# After it succeeds, dispatch the crates.io publish:
gh workflow run publish-crates-io.yml -f tag=v0.288.0

# Or use dry_run=true to validate without uploading:
gh workflow run publish-crates-io.yml -f tag=v0.288.0 -f dry_run=true
```

### Local dry-run before tagging

```bash
cargo package --no-verify --list   # show files in the tarball
cargo package --no-verify          # produce target/package/*.crate
tar tzf target/package/libchdman-rs-*.crate | sort   # inspect
cargo publish --dry-run --no-verify
```

The slim tarball should be:
- under 2 MB compressed (currently ~55 KB),
- ~17 files (Rust source + `Cargo.{toml,lock}` + `README.md` + `LICENSE`
  + `examples/check-prebuilt.rs`),
- no `.cpp`/`.h`/`deps/` paths anywhere.

The publish workflow's preflight job enforces these — it fails the
build before invoking `cargo publish` if any banned content sneaks in.

### Why `--no-verify` on publish

`cargo publish` verification runs `cargo build` in a fresh extraction
of the tarball. That triggers `build.rs`, which has two paths:

- Without `--features prebuilt`: `build.rs` hits the source-build
  guard (no `deps/` in the tarball) and panics with the documented
  "use prebuilt or git" message — intentional, but it fails publish.
- With `--features prebuilt`: works, but downloads from GitHub
  Releases during publish — slow and network-dependent.

Skipping verification with `--no-verify` is safe because the preflight
job already exercised both paths (it ran `cargo package --list` and
validated content + size).
