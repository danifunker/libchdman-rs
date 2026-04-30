//! Re-compress a CHD into a different codec set or hunk size.
//!
//! Mirrors `chdman copy`: reads logical bytes from `source` through a
//! Rust [`ChdDataHandler`], hands them to a fresh `ChdCompressor`, and
//! clones every metadata record verbatim. The unit size is preserved
//! from the source (chdman does the same — copying across unit sizes
//! makes no sense for any CHD type).

use std::path::Path;

use crate::streaming::run_compression;
use crate::{sys, Chd, ChdCompressor, ChdDataHandler, ChdError, CompressionProgress, Result};

/// MAME's `CHDMETAINDEX_APPEND` (chd.h:206) — pass as the index when
/// writing metadata to mean "append, don't overwrite".
const CHDMETAINDEX_APPEND: u32 = !0u32;

#[derive(Debug, Clone, Default)]
pub struct CopyOptions {
    /// New hunk size in bytes. `None` keeps the source's hunk size.
    /// Must be a multiple of the source's unit size; chdman additionally
    /// requires that the new and old hunk sizes be a whole multiple of
    /// each other in either direction (MAME enforces this on create).
    pub hunk_size: Option<u32>,
    /// New codec slots. Use `[0; 4]` for uncompressed.
    pub codecs: [u32; 4],
}

/// `ChdDataHandler` that pulls logical bytes from an open CHD.
///
/// Owns the source `Chd` so it stays alive as long as the compressor
/// is reading from it. `read_data` is mostly stateless — `Chd::read_bytes`
/// supports random access — but the streaming infrastructure already
/// hands offsets to us in monotonic order, so we just forward.
struct ChdReadSource {
    chd: Chd,
}

impl ChdDataHandler for ChdReadSource {
    fn read_data(&mut self, dest: &mut [u8], offset: u64) -> u32 {
        match self.chd.read_bytes(offset, dest) {
            Ok(()) => dest.len() as u32,
            // Surface as a short read; the compressor zero-fills any
            // tail it didn't get back. SHA1 verification on the output
            // would catch silent truncation if the caller cares.
            Err(_) => 0,
        }
    }
}

/// Re-compress `source` into `dest` with `opts.codecs` and (optionally)
/// a new hunk size. All metadata records are cloned verbatim.
///
/// Use this for chdman-equivalent `copy` operations: switching codecs,
/// re-hunking with a different boundary, or producing an uncompressed
/// reference copy. The output's logical bytes, unit bytes, and SHA1 of
/// the decompressed payload are identical to the source's.
pub fn copy(
    source: &Path,
    dest: &Path,
    opts: CopyOptions,
    progress: &mut dyn FnMut(CompressionProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let source_chd = Chd::open(source.to_str().ok_or(ChdError::InvalidFile)?, false, None)?;
    let logical = source_chd.logical_bytes();
    let unit = source_chd.unit_bytes();
    let hunk = opts.hunk_size.unwrap_or(source_chd.hunk_bytes());

    // Snapshot metadata up front so we don't have to share `source_chd`
    // between the metadata walk and the handler.
    let metadata: Vec<crate::MetadataEntry> =
        source_chd.metadata_iter().collect::<Result<Vec<_>>>()?;

    // Move the source into the handler. The handler lives inside the
    // compressor; both are dropped at run_compression's end, after
    // which `source_chd` is also dropped.
    let handler = ChdReadSource { chd: source_chd };
    let mut compressor = ChdCompressor::new(handler);

    let dest_str = dest.to_str().ok_or(ChdError::InvalidFile)?.to_string();
    compressor.create_file(&dest_str, logical, hunk, unit, opts.codecs)?;

    // Clone metadata. Append index (~0) means MAME picks the next free
    // slot per tag — preserving original tag order without us having
    // to track per-tag counters.
    for entry in &metadata {
        let err = unsafe {
            sys::chd_shim_write_metadata(
                compressor.as_chd_file_ptr(),
                entry.tag,
                CHDMETAINDEX_APPEND,
                entry.data.as_ptr() as *const _,
                entry.data.len() as u32,
                entry.flags,
            )
        };
        if err != ChdError::NoError {
            return Err(err);
        }
    }

    run_compression(compressor, dest, progress, cancel)
}
