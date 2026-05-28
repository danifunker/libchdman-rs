# Updating MAME

This crate is locked to a specific version of MAME to ensure compatibility and consistency.

## Process to update MAME

1. Update the MAME submodule to the desired release tag:
   ```bash
   cd deps/mame
   git fetch --tags
   git checkout mame0288 # Replace with new version
   ```

2. Update the crate version in `Cargo.toml` to match the MAME version.

3. Verify `build.rs`:
   - Check if new core files are required for CHD support.
   - Check if 3rdparty dependencies (zlib, flac, etc.) have moved or changed their build structure.

4. Run the full test suite:
   ```bash
   cargo test
   ```

5. If MAME has introduced breaking changes to the `chd_file` class, update `sys/chd_shim.cpp` and `sys/chd_shim.h` accordingly.
