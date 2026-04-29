# Migrating from `chd-rs`

This is a short, opinionated guide for code already using
[`chd-rs`](https://github.com/SnowflakePowered/chd-rs) that wants to switch
to `libchdman-rs`. It covers the patterns most code uses; the rest is left as
an exercise (the API names rhyme).

## TL;DR — what's the same, what's different

| | `chd-rs` | `libchdman-rs` |
|---|---|---|
| Backing | pure-Rust reimplementation | thin Rust wrapper around MAME's C++ CHD core |
| Reading CHDs | yes | yes |
| **Writing / creating CHDs** | no | **yes** (`Chd::create`, `write_hunk`, `write_metadata`, `delete_metadata`, `verify`, streaming `ChdCompressor`) |
| Open from `Read + Seek` | yes (generic `F: Read + Seek`) | via `Chd::open_custom(io)` taking the `ChdIo` trait |
| Header type | typed `Header` enum exposing every field per version | `version_typed()` + per-field accessors on `Chd` |
| MD5 / raw header `flags` | exposed on `Header` (legacy v3/v4 only) | **not exposed** — see "Known omissions" below |

You will edit code, but the diff is small in practice. There is **no**
drop-in `use chd as libchdman_rs;` shim — chd-rs is generic over the file
type throughout, and we are not. Plan on a search-and-replace pass.

## Common porting patterns

### Opening a CHD

```rust
// chd-rs
let f = std::fs::File::open(path)?;
let mut chd = chd::Chd::open(f, None)?;
```

```rust
// libchdman-rs — from a path
let chd = libchdman_rs::Chd::open(path.to_str().unwrap(), false, None)?;
```

If you need to read from a non-`File` source (in-memory buffer, network,
encrypted store, …), implement `libchdman_rs::ChdIo` (it's just
`Read + Write + Seek`) and use `Chd::open_custom`. See
`tests/custom_io.rs` for an example.

### Reading the header

```rust
// chd-rs
let h = chd.header();
let hunk_bytes = h.hunk_size();
let total      = h.logical_bytes();
let sha1       = h.sha1();              // Option<[u8; 20]>
let v          = h.version();           // Version enum
let compressed = h.is_compressed();
```

```rust
// libchdman-rs — accessors live directly on Chd
let hunk_bytes = chd.hunk_bytes();
let total      = chd.logical_bytes();
let sha1       = chd.sha1();            // [u8; 20] (zeros if absent)
let v          = chd.version_typed();   // Version enum
let compressed = chd.is_compressed();
```

The shape is the same — the names and the host type differ. SHA-1 / parent
SHA-1 / raw SHA-1 are returned as `[u8; 20]` directly (filled with zeros
when the file does not carry one), rather than `Option<[u8; 20]>`.

### Iterating hunks

```rust
// chd-rs
for h in chd.hunks() { /* ... */ }
```

```rust
// libchdman-rs
for hunk in chd.hunks() {
    let bytes = hunk?;       // Result<Vec<u8>>
    /* ... */
}
```

For random access to a specific hunk, prefer
`HunkReader::new(&chd, hunknum)` — it gives you `Read + Seek + BufRead` over
a single decompressed hunk, without keeping the rest of the CHD resident.

### Reading by byte offset

`chd-rs` doesn't ship a CHD-as-stream adapter out of the box; you build one.
We do:

```rust
let mut r = chd.reader();    // ChdReader: Read + Seek
r.seek(SeekFrom::Start(off))?;
r.read_exact(&mut buf)?;
```

There's also a non-`Read` version: `chd.read_bytes(off, &mut buf)`.

### Iterating metadata

```rust
// chd-rs
for entry in chd.metadata() {
    let m = entry.read()?;
    println!("{:?} = {:?}", m.tag(), m.data());
}
```

```rust
// libchdman-rs
for entry in chd.metadata_iter() {
    let entry = entry?;       // MetadataEntry { tag, flags, data }
    println!("{:?} = {:?}", entry.tag, entry.data);
}
```

The well-known tag constants (`HARD_DISK_METADATA_TAG`,
`CDROM_TRACK_METADATA2_TAG`, …) live in `libchdman_rs::metadata::tags`,
matching chd-rs's `KnownMetadata` constants in spirit.

`metadata::is_cdrom(tag)` and `metadata::is_gdrom(tag)` are the equivalents
of chd-rs's classifier helpers.

### Detecting CHD type

```rust
chd.is_hd() | chd.is_cd() | chd.is_gd() | chd.is_dvd() | chd.is_av()
```

Each is backed by MAME's `chd_file::check_is_*`, which queries for the
appropriate metadata tag. `chd-rs` users typically rolled this themselves
by inspecting which tags showed up in the metadata iterator; we expose it
directly.

### CD-ROM frame layout constants

Same names as chd-rs, in `libchdman_rs::cdrom`:

```rust
use libchdman_rs::cdrom::{CD_FRAME_SIZE, CD_MAX_SECTOR_DATA, CD_SYNC_HEADER};
```

### Writing / creating CHDs

`chd-rs` is read-only. The write-side API has no analogue; here you have:

- `Chd::create(path, logical_bytes, hunk_bytes, unit_bytes, [c0, c1, c2, c3])`
- `chd.write_hunk(n, &data)`, `chd.write_bytes(off, &data)`
- `chd.write_metadata(tag, idx, data, flags)`, `chd.delete_metadata(...)`
- `chd.verify()` — content-side SHA-1 verification
- `ChdCompressor` for streaming "chdman create"-style compression with
  progress reporting

See `tests/integration.rs` and `tests/basic_tests.rs` for end-to-end usage.

## Known omissions vs. chd-rs

These chd-rs APIs have no equivalent here, by design:

- **`md5()` / `parent_md5()`** — MAME's `chd_file` parses the v3/v4 raw
  header and discards the MD5 fields. We could re-read the header bytes
  ourselves, but legacy v3/v4 CHDs are rare enough that we left this out.
- **Raw header `flags()`** — same reason.
- **`meta_offset()`** (v5 metadata file offset) — internal detail of
  MAME's storage, not surfaced by `chd_file`.

Open an issue if any of these block your migration; they're conscious
omissions, not hard blockers.

## Things to keep in mind

- `Chd` here is **not** generic over the file type. For non-path sources,
  use `Chd::open_custom` with a `ChdIo` impl.
- `Result<T>` is `std::result::Result<T, libchdman_rs::ChdError>`, not
  chd-rs's `Error`. The variants are similar in spirit but the names are
  CHD's native error codes.
- We pull in MAME's CHD core through C++ and FFI. That means a slower
  *first* compile (and a `git submodule update --init` step), but no Rust
  reimplementation drift to worry about as MAME evolves.
