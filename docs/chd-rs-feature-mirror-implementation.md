# chd-rs Feature Mirror — Implementation Tracker

This document tracks work to close the API gaps between this crate and
[chd-rs](https://github.com/SnowflakePowered/chd-rs). It is a living document;
update the checkboxes as items land.

## Scope

We do **not** offer drop-in source compatibility with chd-rs (their API is
generic over `F: Read + Seek`; ours is FFI-backed and non-generic). Instead we:

1. Add equivalent capabilities under our own type names.
2. Place all new Rust code in `src/enhancements.rs` so it is easy to spot
   what is novel to this crate vs. what is a thin wrapper over MAME.
3. Provide a short migration guide (`docs/migration-from-chd-rs.md`) covering
   the most common patterns.

Existing public API in `src/lib.rs` is unchanged. Nothing renames.

## Phases

### Phase A — FFI shims

C++ wrappers in `sys/chd_shim.cpp` + `sys/chd_shim.h`, `extern "C"` decls
in `src/sys.rs`. Limited to MAME's public `chd_file` surface.

- [ ] `chd_shim_compressed(chd)` — bool, wraps `chd_file::compressed()`.
- [ ] `chd_shim_compression(chd, index)` — `u32` codec for slot 0..3.
- [ ] `chd_shim_has_parent(chd)` — bool, wraps `chd_file::parent() != nullptr`.
- [ ] `chd_shim_check_is_hd / cd / gd / dvd / av(chd)` — typed checks
      from `chd_file::check_is_*`.
- [ ] `chd_shim_metadata_enum(chd, index, out_tag, out_flags, out_data, ...)`
      — wraps the public `read_metadata(WILDCARD, index, vec, &tag, &flags)`
      overload so we can enumerate every metadata entry by ordinal.

**Known omissions** (not in MAME's public API; would require reaching past
`chd_file`):

- `md5()` / `parent_md5()` — stored only in v3/v4 raw header bytes; MAME
  parses then discards. Re-reading the header to expose these is more
  invasive than the value of legacy MD5 access. Document in migration guide.
- `flags()` raw header flags — same reason.
- `meta_offset()` v5 metadata file offset — not surfaced by `chd_file`.

### Phase B — `src/enhancements.rs`

All new Rust here. Re-exported from `src/lib.rs` for ergonomic use.

- [ ] `Version` enum (V1/V3/V4/V5) + `Chd::version_typed()`.
- [ ] `Chd::is_compressed()`, `Chd::has_parent()`,
      `Chd::compression(i) -> Option<u32>`.
- [ ] `Chd::is_hd()`, `Chd::is_cd()`, `Chd::is_gd()`, `Chd::is_dvd()`,
      `Chd::is_av()`.
- [ ] `Chd::hunks() -> HunkIter` — yields `Result<Vec<u8>>` per hunk.
- [ ] `Chd::metadata_iter() -> MetadataIter` — yields
      `MetadataEntry { tag, index, flags, data }` using the wildcard shim.
- [ ] `ChdReader<'a>` — `impl Read + Seek` over `&Chd`, internally
      driven by `read_bytes`.
- [ ] `HunkReader` — `impl Read + Seek + BufRead` over a single hunk.
- [ ] `pub mod metadata::tags` — `KnownMetadata` constants
      (HARD_DISK, CDROM_OLD, CDROM_TRACK, CDROM_TRACK_2, GDROM_OLD,
      GDROM_TRACK, AV, AV_LD).
- [ ] `pub mod cdrom` — `CD_FRAME_SIZE`, `CD_MAX_SECTOR_DATA`,
      `CD_MAX_SUBCODE_DATA`, `CD_SYNC_NUM_BYTES`, `CD_SYNC_HEADER`,
      `CD_SYNC_OFFSET`, `CD_MODE_OFFSET`.

### Phase C — Integration tests

Fixtures generated at test-runtime via our own create/write API to keep the
repo small and avoid licensing concerns.

- [ ] `tests/fixtures/mod.rs` — helper that creates synthetic CHDs in `tempfile`.
- [ ] Synthetic raw CHD (~256 KB seeded pseudo-random; default codecs).
- [ ] Synthetic hard-disk CHD (~512 KB pattern data + HARD_DISK_METADATA_TAG).
- [ ] Synthetic CD CHD (1 data track + 1 short audio track of silence;
      CDROM_TRACK_METADATA2_TAG entries).
- [ ] `tests/integration_read.rs` — open all 3, iterate hunks, iterate
      metadata, verify SHA1.
- [ ] `tests/integration_write.rs` — round-trip create→close→reopen→compare.
- [ ] `tests/integration_reader.rs` — exercise `ChdReader` and `HunkReader`.

### Phase D — Migration guide

- [ ] `docs/migration-from-chd-rs.md` — short, focused. Cover:
  open/header access, hunk reads, metadata iteration, version detection,
  pointer to write-side APIs (which chd-rs lacks).

## Status

| Phase | Status |
|---|---|
| A — FFI shims | done |
| B — enhancements.rs | done |
| C — Integration tests | done |
| D — Migration guide | not started |

Update this table and the checkboxes above as work lands. Each phase ships
as its own commit on `main`.
