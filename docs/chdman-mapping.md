# chdman → libchdman-rs mapping

Quick reference for porting code that previously shelled out to the
`chdman` CLI. Each row maps a chdman subcommand and its commonly-used
flags to the equivalent Rust API. For full detail on each module see
[format-modules.md](format-modules.md).

## Subcommand map

| chdman                 | libchdman-rs                                  |
| ---------------------- | --------------------------------------------- |
| `chdman createhd`      | [`hd::create_from_path`] / [`hd::create_from_reader`] |
| `chdman extracthd`     | [`hd::extract_to_path`] / [`hd::extract_to_writer`] |
| `chdman createraw`     | [`hd::create_from_reader`] with custom `HdCreateOptions` (no geometry) |
| `chdman extractraw`    | [`hd::extract_to_path`] (no geometry round-trip needed) |
| `chdman createdvd`     | [`dvd::create_from_iso`] / [`dvd::create_from_reader`] |
| `chdman extractdvd`    | [`dvd::extract_to_iso`] / [`dvd::extract_to_writer`] |
| `chdman createcd`      | [`cd::create_from_cue`] / [`cd::create_from_iso`] |
| `chdman extractcd`     | [`cd::extract_to_cue`] (multi-track) / [`cd::extract_to_iso`] (MODE1 only) |
| `chdman copy`          | [`copy::copy`]                                |
| `chdman info`          | [`Chd::info`] returns a `ChdInfo` snapshot    |
| `chdman verify`        | [`Chd::verify`]                               |
| `chdman addmeta`       | [`Chd::write_metadata`]                       |
| `chdman delmeta`       | [`Chd::delete_metadata`]                      |
| `chdman dumpmeta`      | [`Chd::read_metadata`] (+ [`MetadataIter`])   |
| `chdman creategd` / `extractgd` | Deferred — see TODO.md                |
| `chdman createld` / `createav` / `extractld` / `extractav` | Deferred — see TODO.md |

## Flag map

| chdman flag                | Rust equivalent                                  |
| -------------------------- | ------------------------------------------------ |
| `-c <spec>` / `--compression` | `opts.codecs = parse_codec_spec(spec)?`       |
| `-c none`                  | `opts.codecs = [0; 4]`                           |
| `-hs <bytes>` / `--hunksize`  | `opts.hunk_size = bytes`                      |
| `-us <bytes>` / `--unitsize` | `HdCreateOptions::unit_size`                   |
| `-chs C,H,S` / `--chs`     | `HdCreateOptions::geometry = Some(HdGeometry { … })` |
| `-ident <file>` / `--ident` | `HdCreateOptions::ident = Some(bytes)`          |
| `-i <file>` / `--input`    | `create_from_path(in_path, …)`                   |
| `-o <file>` / `--output`   | `create_from_path(…, out_path, …)`               |
| `-ib <bytes>` / `--inputbytes` | `opts.logical_size = bytes`                  |
| `-f` / `--force`           | Delete the output file yourself before calling create — the API never overwrites |
| Cancellation (`Ctrl+C`)    | `cancel: &dyn Fn() -> bool` returning `true`     |
| Progress output            | `progress: &mut dyn FnMut(CompressionProgress)` (creation) or `&mut dyn FnMut(u64)` (extraction) |

## Defaults

The Rust defaults match chdman's defaults so that
`Default::default()` produces equivalent output to running chdman with
no flags:

| Format | Hunk size | Unit size | Codecs                       |
| ------ | --------- | --------- | ---------------------------- |
| HD     | 4096      | 512       | `[zlib, 0, 0, 0]`            |
| DVD    | 4096      | 2048      | `[lzma, zlib, huff, flac]`   |
| CD     | 19584     | 2448      | `[cdlz, cdzl, 0, 0]`         |
| copy   | source's  | source's  | `[0; 4]` (uncompressed)      |

## What's not exposed

- The chdman CLI itself. This crate is a library; the binary is not
  reimplemented.
- v3/v4 CHD *creation*. Reading legacy CHDs via `Chd::open` works fine,
  but chdman has effectively retired creating those formats and so
  has libchdman-rs.
- chdman's interactive prompts (e.g. confirming output overwrite,
  asking for parent CHD passwords). The Rust API never prompts —
  callers decide.

## Example: porting a `chdman createcd` invocation

```sh
chdman createcd -i game.cue -o game.chd -c cdlz,cdfl,cdzl
```

becomes

```rust
use libchdman_rs::cd::{create_from_cue, CdCreateOptions};
use libchdman_rs::parse_codec_spec;
use std::path::Path;

create_from_cue(
    Path::new("game.cue"),
    Path::new("game.chd"),
    CdCreateOptions {
        codecs: parse_codec_spec("cdlz,cdfl,cdzl")?,
        ..CdCreateOptions::default()
    },
    &mut |_p| {},
    &|| false,
)?;
```
