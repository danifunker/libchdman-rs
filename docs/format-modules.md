# Format modules

libchdman-rs exposes one Rust module per CHD container shape that
chdman knows how to create or extract. Each module is a thin facade
over the same MAME C++ code chdman uses, so output is byte-for-byte
compatible with the upstream tool.

| Module           | chdman equivalents              | Status                  |
| ---------------- | ------------------------------- | ----------------------- |
| [`hd`](#hd)      | `createhd`, `extracthd`         | Implemented             |
| [`dvd`](#dvd)    | `createdvd`, `extractdvd`       | Implemented (MAME 0.287+)|
| [`cd`](#cd)      | `createcd`, `extractcd`         | Implemented             |
| [`copy`](#copy)  | `copy`                          | Implemented             |
| `gd` (planned)   | `creategd`, `extractgd`         | Deferred — see TODO.md  |
| `ld` / `av`      | `createld`, `createav`, etc.    | Deferred — see TODO.md  |

All four implemented modules share a single calling convention:

- A `*CreateOptions` struct with codec slots, hunk size, and any
  format-specific tunables. Defaults match chdman's defaults exactly.
- A `create_from_*` function that takes the source (a `Read`,
  a `&Path`, or a CUE path), the output `&Path`, options, a
  `&mut dyn FnMut(CompressionProgress)` progress callback, and a
  `&dyn Fn() -> bool` cancel predicate.
- An `extract_to_*` function that takes the source CHD `&Path`, the
  destination, and a `&mut dyn FnMut(u64)` byte-progress callback.

The progress callback is invoked at hunk boundaries during creation
and at hunk boundaries (or per-frame, for CD extraction) during
extraction. The cancel predicate is sampled between hunks; on cancel
the partial output file is unlinked and `ChdError::Cancelled` is
returned. See [`crate::streaming`] for the underlying machinery.

## Codec selection

Codecs are passed as `[u32; 4]` slot arrays. Use the named constants in
[`crate::codec`] or parse a chdman-style mnemonic string with
[`parse_codec_spec`]:

```rust
use libchdman_rs::{parse_codec_spec, CHD_CODEC_LZMA, CHD_CODEC_ZLIB};

let codecs = parse_codec_spec("lzma,zlib")?;
assert_eq!(codecs, [CHD_CODEC_LZMA, CHD_CODEC_ZLIB, 0, 0]);
```

`"none"` is a special case that produces `[0; 4]` (uncompressed).
Trailing slots default to `0`. Each mnemonic must be exactly four
ASCII bytes and must satisfy [`codec_exists`]. See chdman's
`parse_compression` for the upstream behaviour this mirrors.

---

## `hd`

Hard-disk CHDs. Logical-byte stream plus a `GDDD` geometry record and
an optional `IDNT` ident blob. Geometry is either supplied or derived
via the same heuristic chdman uses (`compute_chs`, ported from
`guess_chs` in `chdman.cpp`).

```rust
use libchdman_rs::hd::{create_from_path, extract_to_path, HdCreateOptions};
use std::path::Path;

let opts = HdCreateOptions {
    logical_size: 0, // 0 means "use the input file size"
    ..HdCreateOptions::default()
};
create_from_path(
    Path::new("disk.img"),
    Path::new("disk.chd"),
    opts,
    &mut |p| eprintln!("{}/{} bytes", p.bytes_done, p.bytes_total),
    &|| false,
)?;

extract_to_path(
    Path::new("disk.chd"),
    Path::new("out.img"),
    &mut |bytes| eprintln!("{bytes}"),
)?;
```

Defaults: hunk_size 4096, unit_size 512, codecs `[zlib, 0, 0, 0]`. To
write a `createraw`-style payload (no GDDD, custom hunk/unit), supply
your own `HdCreateOptions` with `geometry: Some(...)` set to a
caller-defined value and any hunk/unit shape — the only requirement is
that `hunk_size % unit_size == 0` and `logical_size % unit_size == 0`.

## `dvd`

DVD CHDs (MAME 0.287+). Flat 2048-byte sectors, no track structure,
single empty `DVD ` metadata record so MAME's `check_is_dvd` recognises
the file.

```rust
use libchdman_rs::dvd::{create_from_iso, extract_to_iso, DvdCreateOptions};
use std::path::Path;

create_from_iso(
    Path::new("game.iso"),
    Path::new("game.chd"),
    DvdCreateOptions::default(),
    &mut |_p| {},
    &|| false,
)?;

extract_to_iso(Path::new("game.chd"), Path::new("out.iso"), &mut |_| {})?;
```

Defaults: hunk_size 4096 (`2 * 2048`), codecs `[lzma, zlib, huff, flac]`
— chdman's `s_default_hd_compression`, reused verbatim by `do_create_dvd`.
`logical_size` must be a multiple of 2048; `extract_to_iso` rejects any
CHD that lacks the `DVD ` tag with `ChdError::UnsupportedFormat`.

## `cd`

CD-ROM CHDs. All CD-format logic — CUE/GDI/ISO/Nero parsing, track
padding, ECC/EDC synthesis, audio byte-swap, CHT2 metadata records —
runs inside MAME's `cdrom_file` and the ported `chd_cd_compressor`,
giving byte-for-byte parity with `chdman createcd` for the same input.

```rust
use libchdman_rs::cd::{create_from_cue, extract_to_cue, list_tracks, CdCreateOptions};
use libchdman_rs::Chd;
use std::path::Path;

create_from_cue(
    Path::new("game.cue"),
    Path::new("game.chd"),
    CdCreateOptions::default(),
    &mut |_p| {},
    &|| false,
)?;

let chd = Chd::open("game.chd", false, None)?;
for t in list_tracks(&chd)? {
    println!("track {}: {:?} {} frames", t.track_num, t.track_type, t.frames);
}

extract_to_cue(
    Path::new("game.chd"),
    Path::new("out.cue"),
    Path::new("out.bin"),
    &mut |_| {},
)?;
```

Defaults: hunk_size 19584 (8 frames × 2448 bytes), codecs `[cdlz, cdzl, cdfl, 0]`.

- [`create_from_cue`] handles CUE, GDI, ISO, and Nero TOC inputs (MAME's
  `parse_toc` dispatches on extension and content).
- [`create_from_iso`] is a convenience that synthesizes a one-track
  MODE1/2048 CUE next to the ISO, then calls `create_from_cue`.
- [`extract_to_cue`] writes a single combined `.bin` plus a CUE that
  references it, matching chdman's `MODE_CUEBIN` output (audio tracks
  byte-swapped back to little-endian, subcode dropped on output —
  bin/cue cannot represent it).
- [`extract_to_iso`] requires a single MODE1/MODE1_RAW track and emits
  cooked 2048-byte sectors. For multi-track or audio-bearing CHDs,
  use `extract_to_cue`.
- [`extract_to_gdi`] writes a Sega GD-ROM `.gdi` index plus per-track
  split files (`<stem>NN.bin` for data, `<stem>NN.raw` for audio),
  matching chdman's `MODE_GDI` output. Audio is byte-swapped back to
  little-endian for v5+ CHDs and `splitframes` pregap data is pulled
  across track boundaries; subcode is dropped (GDI cannot carry it). A
  `.gdi` round-trips: feeding it to `create_from_cue` produces a GD-ROM
  CHD (reports `is_gd`), and `extract_to_gdi` reproduces the track files.

## `copy`

Re-compress a CHD into a different codec set or hunk size. Mirrors
`chdman copy`: cloning every metadata record verbatim, preserving the
source's `unit_bytes` and `raw_sha1`.

```rust
use libchdman_rs::copy::{copy, CopyOptions};
use libchdman_rs::{CHD_CODEC_LZMA, CHD_CODEC_ZLIB};
use std::path::Path;

copy(
    Path::new("old.chd"),
    Path::new("new.chd"),
    CopyOptions {
        hunk_size: None, // preserve source's hunk size
        codecs: [CHD_CODEC_LZMA, CHD_CODEC_ZLIB, 0, 0],
    },
    &mut |_p| {},
    &|| false,
)?;
```

Works for HD, DVD, and CD CHDs uniformly — the metadata clone and
logical-byte read path do not care which format the source is.
