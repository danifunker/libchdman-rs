# Releasing libchdman-rs

This is the maintainer runbook for cutting a release. It documents the
three GitHub Actions pipelines and the exact order to drive them.

There are two flavours of release:

- **Code release** — a bug fix or feature in the Rust/shim source, no new
  prebuilt platform. This is the common case (e.g. the CD extraction fix).
- **New prebuilt target** — adding a platform to the prebuilt-archive
  matrix. Needs an extra throwaway-CI validation pass first.

Both end at the same place: a GitHub Release carrying the prebuilt static
archives, and a `cargo publish` to crates.io.

---

## Versioning

- The crate version tracks the embedded MAME release plus a patch counter:
  `0.288.<N>`. Each release bumps the patch by **one** (`0.288.8 → 0.288.9`).
- The git tag is the version with a `v` prefix and **plain semver** —
  `v0.288.9`. The old `-lN` suffix is retired.
- The version lives in `Cargo.toml`; `Cargo.lock` must be updated to match
  (`cargo update -p libchdman-rs --precise <version>`). Both workflows'
  preflight jobs fail if `Cargo.toml` and the tag disagree.

---

## The pipelines

### 1. `ci.yml` — CI (validation)

- **Trigger:** every push and pull request to `main`.
- **Jobs:** `test` (build + `cargo test --all-features` + `cargo test` on
  ubuntu/macos/windows), `lint` (`cargo fmt --check`, `cargo clippy
  --all-features -- -D warnings`), `docs` (`cargo doc --all-features`).
- Runs with `LIBCHDMAN_FORCE_SOURCE=1` so it builds MAME from source
  instead of trying to download a prebuilt asset that doesn't exist yet for
  the in-development version.
- This is the gate that a **code release** relies on for validation — it
  runs automatically on the PR. Get it green before merging.

### 2. `release-prebuilt.yml` — Build prebuilt static archives (the release)

- **Trigger:** manual `workflow_dispatch` with a `tag` input.
- **Preflight** (fails fast, no build) checks: tag starts with `v`;
  `${tag#v}` equals the `Cargo.toml` version; the tag does **not** already
  exist on origin; no GitHub Release with that tag exists yet.
- **Build matrix** — one archive per row, each built → merged into a single
  fat archive → validated by `scripts/validate-archive.sh` (confirms the
  exported `chd_shim_*` symbols) → smoke-tested against
  `examples/check-prebuilt.rs` → uploaded:
  - `build-linux`: `x86_64` and `aarch64` × glibc **2.35** and **2.39** (4)
  - `build-linux-armv7`: `armv7-...-gnueabihf` glibc **2.31** (1)
  - `build-linux-riscv64`: `riscv64gc` × glibc **2.35** and **2.39** (2)
  - `build-macos`: `x86_64` and `aarch64` (2)
  - `build-windows`: `x64`, `x86`, `arm64` MSVC (3)
- **`release` job** downloads all artifacts and creates the GitHub Release,
  **tagging at the dispatched commit** (`target_commitish: github.sha`). You
  do **not** create the tag by hand — this workflow does.
- Net output: **12 archives + 12 `.sha256` sidecars = 24 assets**. If you
  change the matrix, update the expected count in `publish-crates-io.yml`
  (and the asset-count gate).

### 3. `publish-crates-io.yml` — Publish to crates.io

- **Trigger:** manual `workflow_dispatch` with `tag` and optional
  `dry_run` inputs.
- **Preflight** checks out the tag and verifies: `Cargo.toml` version
  matches the tag; the GitHub Release exists with **≥ 24 assets** (i.e.
  `release-prebuilt.yml` has already finished for this tag); the packaged
  tarball is slim (no `*.cpp/*.h` or `deps/`, under 2 MB).
- **Publish** runs `cargo publish --no-verify` (or `--dry-run --no-verify`
  when `dry_run=true`). `--no-verify` avoids the network-dependent prebuilt
  download during verification; the tarball was already validated in
  preflight.
- **Must run after** `release-prebuilt.yml` has completed — its preflight
  depends on the release + assets existing.

---

## Runbook: code release (common case)

Roles: the branch prep, version bump, and merge are done without asking.
**Cutting each public release is gated on an explicit go-ahead** — pause
before dispatching `release-prebuilt.yml` and again before
`publish-crates-io.yml`. Those are the irreversible, outward-facing steps.

1. **Land the change on a branch and open a PR.** Let `ci.yml` go green on
   the PR (test + lint + docs across the three host OSes). Fix anything red.

2. **Bump the version** on the branch:
   ```sh
   # edit Cargo.toml: version = "0.288.9"
   cargo update -p libchdman-rs --precise 0.288.9   # syncs Cargo.lock
   ```
   Commit it (`chore: bump to 0.288.9`). Re-confirm CI is green.

3. **Merge to `main`** (fast-forward preferred so `main` is exactly the
   commit you intend to release and tag).

4. **[confirm]** Dispatch the release build against `main`:
   ```sh
   gh workflow run release-prebuilt.yml --ref main -f tag=v0.288.9
   gh run watch "$(gh run list --workflow=release-prebuilt.yml --limit 1 --json databaseId --jq '.[0].databaseId')" --exit-status
   ```
   On success this creates the `v0.288.9` tag + GitHub Release with all 24
   assets. Preflight rejects a mismatched/duplicate tag.

5. **[confirm]** Publish to crates.io (optionally dry-run first):
   ```sh
   gh workflow run publish-crates-io.yml -f tag=v0.288.9 -f dry_run=true   # optional validation
   gh workflow run publish-crates-io.yml -f tag=v0.288.9
   ```

---

## Runbook: adding a new prebuilt target

`ci.yml` only builds the three host OSes from source, so a new
cross/exotic target can pass CI yet fail its own prebuilt build (this is how
the FLAC NEON failure on MSVC-ARM64 was caught). Validate it in isolation
first.

1. **Producing side** — add the target row to the matrix in
   `release-prebuilt.yml` (the appropriate `build-*` job), and add that job
   to the `release` job's `needs:` list. Each row: build → merge libs
   (`ar -M` on Linux, `libtool` on macOS, `lib.exe` on Windows) →
   `scripts/validate-archive.sh` → smoke test → upload.

2. **Consuming side** — usually no `build.rs` change: it keys Linux off
   `unknown-linux-gnu` (routed through the `LIBCHDMAN_GLIBC` floor logic,
   auto → 2.35) and Windows off `pc-windows-msvc`. Asset name is
   `libchdman_rs-<target>[-glibcX.Y].{a|lib}`.

3. **Throwaway validation** — on the feature branch, add a temporary
   `test-<target>.yml` triggered `on: push: branches: [<branch>]` that runs
   only the new target's build + validate + smoke test (no release). Push,
   watch it green (`gh run watch <id> --exit-status`), then **delete the
   throwaway workflow**. (`ci.yml` only watches `main`, so a feature-branch
   push won't trigger it.)

4. **Update the asset-count gate** in `publish-crates-io.yml` for the new
   archive + sidecar (each new target adds 2 to the expected count).

5. From here, follow the **code release** runbook (bump `.1`, merge,
   dispatch `release-prebuilt.yml`, then `publish-crates-io.yml`). One
   validated target per `.1` bump.

---

## Ordering & gotchas

- **Order is fixed:** merge → `release-prebuilt.yml` (creates tag + release
  + assets) → `publish-crates-io.yml` (needs the release to exist).
- **Don't tag manually.** `release-prebuilt.yml` tags at the built commit;
  a pre-existing tag makes its preflight fail.
- **Version/​tag must agree.** Bump `Cargo.toml` (and `Cargo.lock`) before
  dispatching; both workflows' preflights compare `${tag#v}` to the crate
  version.
- **Asset count.** The expected `24` in `publish-crates-io.yml` is
  `matrix archives × 2`. Keep it in sync when the matrix changes.
- CI, and any local `cargo build`/`cargo test`, exercise the from-source
  path (`LIBCHDMAN_FORCE_SOURCE=1`); the `prebuilt` feature only resolves
  once the corresponding tag's release assets are live.
