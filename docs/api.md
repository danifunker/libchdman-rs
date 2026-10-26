# Rust API Mapping

The goal of this crate is to provide an *idiomatic* Rust experience while staying true to MAME's performance.

## Resource Management

The `Chd` struct implements `Drop`. When a `Chd` goes out of scope, it automatically calls `chd_shim_close` and `chd_shim_free`, ensuring no memory leaks or dangling file handles.

## Error Handling

MAME's `chd_error` enum is mirrored in `src/sys.rs` and re-exported. All fallible operations return a `libchdman_rs::Result<T>`.

Two variants are Rust-only and never come back over FFI; their discriminants sit well above MAME's range (which stops at 24) so they can't collide:

- `ChdError::Cancelled` (100) — a long-running operation was stopped via its `cancel` callback.
- `ChdError::NotCdMedia` (101) — a `cd::*` call was handed a CHD that isn't CD/GD-ROM media (hard-disk, DVD, A/V). `cd::*` calls still report `InvalidData` when the geometry *is* CD-shaped but the track metadata is missing or unparseable.

## I/O Traits

Instead of forcing users to use `std::fs::File`, the crate defines a `ChdIo` trait:

```rust
pub trait ChdIo: Read + Write + Seek {
    fn length(&mut self) -> std::io::Result<u64>;
}
```

This trait is automatically implemented for any type that satisfies `Read + Write + Seek`. This allows seamless integration with `File`, `Cursor<Vec<u8>>`, or custom wrappers.

## Streaming CD readers

Two `cd` types read a CD CHD on demand instead of extracting it. Both own the `Chd` (recover it with `into_inner()`), implement `Read + Seek`, and are `Send` but not `Sync`: reads take `&mut self`, so share one across threads behind a `Mutex`.

| Type | Sector | Bytes match | Accepts |
| ---- | ------ | ----------- | ------- |
| `cd::CdCookedReader` | 2048 | `extract_to_iso` | MODE1 / MODE1_RAW / Mode 2 Form 1-style data tracks |
| `cd::CdAudioReader` | 2352 | `extract_to_cue`'s `.bin` | audio and raw (2352-byte) data tracks |

`CdAudioReader` API:

```rust
impl CdAudioReader {
    pub const SECTOR_SIZE: usize = 2352;
    pub fn open(chd: Chd) -> Result<Self>;                       // whole disc, BIN address space
    pub fn open_track(chd: Chd, track_index: u32) -> Result<Self>; // one track, 0-based
    pub fn sector_count(&self) -> u32;
    pub fn len(&self) -> u64;
    pub fn is_empty(&self) -> bool;
    pub fn read_sectors(&mut self, start: u32, count: u32, buf: &mut [u8]) -> Result<()>;
    pub fn into_inner(self) -> Chd;
}
```

Errors: `NotCdMedia` for a non-CD CHD; `InvalidData` for an out-of-range track or sector range, or a `buf` that isn't `count * 2352` bytes; `UnsupportedFormat` when the view would cover a cooked (non-2352) data track.

## Asynchronous Operations

The `ChdCompressor` provides a high-level API for creating compressed CHDs. It uses a `ChdDataHandler` trait (callback-based) to feed data to the MAME compressor, matching MAME's own internal multi-threaded compression architecture.
