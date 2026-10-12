# Updating MAME

This crate is locked to a specific version of MAME to ensure compatibility and consistency.

## Process to update MAME

1. Update the MAME submodule to the desired release tag:
   ```bash
   cd deps/mame
   git fetch --tags
   git checkout mame0289 # Replace with new version
   ```

2. Update the crate version in `Cargo.toml` to match the MAME version.

3. Regenerate the vendored subset in `mame-chd-src/` (the files `build.rs`
   actually compiles, plus headers and licenses; this is what ships on
   crates.io):
   ```bash
   scripts/vendor-mame.sh            # defaults to deps/mame
   ```
   The script refuses to run if the MAME version doesn't match the crate
   version, and records the MAME tag and commit in
   `mame-chd-src/MAME_VERSION`. Review the diff to see what upstream
   changed.

4. Verify `build.rs`:
   - Check if new core files are required for CHD support. Add them to
     `build.rs` as `mame-chd-src/...` paths and re-run the vendor script;
     it picks up source files from `build.rs` automatically. New header
     directories need adding to `header_dirs` in the script.
   - Check if 3rdparty dependencies (zlib, flac, etc.) have moved or changed their build structure.

5. Run the full test suite:
   ```bash
   cargo test
   ```

6. If MAME has introduced breaking changes to the `chd_file` class, update `sys/chd_shim.cpp` and `sys/chd_shim.h` accordingly.
