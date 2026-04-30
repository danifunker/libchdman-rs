//! Hard-disk CHDs (chdman `createhd` / `extracthd` parity).
//!
//! Streams a raw disk image into a CHD with a `GDDD` geometry record,
//! and (optionally) an `IDNT` ident blob — matching chdman's
//! `do_create_hd` byte-for-byte for the same input. Geometry is either
//! supplied by the caller or derived via the same heuristic chdman uses
//! (`guess_chs` in `chdman.cpp`).

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::enhancements::metadata::tags::{HARD_DISK_IDENT_METADATA_TAG, HARD_DISK_METADATA_TAG};
use crate::streaming::{run_compression, StreamingSource};
use crate::{sys, Chd, ChdCompressor, ChdError, CompressionProgress, Result, CHD_CODEC_ZLIB};

/// Cylinder/head/sector geometry plus bytes-per-sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HdGeometry {
    pub cylinders: u32,
    pub heads: u32,
    pub sectors: u32,
    pub sector_bytes: u32,
}

impl HdGeometry {
    /// Total logical bytes implied by this geometry.
    pub fn logical_bytes(&self) -> u64 {
        u64::from(self.cylinders)
            * u64::from(self.heads)
            * u64::from(self.sectors)
            * u64::from(self.sector_bytes)
    }
}

/// Options for [`create_from_reader`].
#[derive(Debug, Clone)]
pub struct HdCreateOptions {
    /// Logical size of the destination CHD, in bytes. Must be a multiple
    /// of `unit_size`. The reader is zero-padded if it ends earlier; the
    /// CHD reports this exact size in `logical_bytes()`.
    pub logical_size: u64,
    /// Hunk size in bytes. chdman default is 4096. Must be a multiple of
    /// `unit_size`.
    pub hunk_size: u32,
    /// Unit (sector) size in bytes. chdman default is 512.
    pub unit_size: u32,
    /// Codec slots. Default `[zlib, 0, 0, 0]`. Use [`crate::parse_codec_spec`]
    /// to plumb chdman-style mnemonic strings.
    pub codecs: [u32; 4],
    /// If supplied, written verbatim into a `GDDD` record. Otherwise
    /// derived from `logical_size / unit_size` via the same heuristic
    /// chdman uses.
    pub geometry: Option<HdGeometry>,
    /// Optional identification blob written as an `IDNT` metadata record.
    pub ident: Option<Vec<u8>>,
}

impl Default for HdCreateOptions {
    fn default() -> Self {
        Self {
            logical_size: 0,
            hunk_size: 4096,
            unit_size: 512,
            codecs: [CHD_CODEC_ZLIB, 0, 0, 0],
            geometry: None,
            ident: None,
        }
    }
}

/// Heuristic from chdman's `guess_chs` (chdman.cpp:1115). Finds CHS
/// values whose product equals `total_sectors`, preferring large sector
/// counts (≤63) and head counts (≤16). If no clean factorization
/// exists, increments `total_sectors` and retries — i.e. allows a tiny
/// rounding-up to land on a factorable shape, just like chdman.
///
/// Always terminates for any positive input: at the limit, `(total, 1, 1)`
/// is always a valid factorization once we walk far enough.
pub fn compute_chs(logical_bytes: u64, sector_size: u32) -> Result<HdGeometry> {
    if logical_bytes == 0 || sector_size == 0 {
        return Err(ChdError::InvalidData);
    }
    if !logical_bytes.is_multiple_of(u64::from(sector_size)) {
        return Err(ChdError::InvalidData);
    }

    let initial = logical_bytes / u64::from(sector_size);
    let mut total: u64 = initial;
    loop {
        for cur_sectors in (2u32..=63).rev() {
            if total.is_multiple_of(u64::from(cur_sectors)) {
                let total_heads = total / u64::from(cur_sectors);
                for cur_heads in (2u32..=16).rev() {
                    if total_heads.is_multiple_of(u64::from(cur_heads)) {
                        let cylinders = (total_heads / u64::from(cur_heads)) as u32;
                        return Ok(HdGeometry {
                            cylinders,
                            heads: cur_heads,
                            sectors: cur_sectors,
                            sector_bytes: sector_size,
                        });
                    }
                }
            }
        }
        // No factorization at this total — bump and retry, mirroring
        // chdman's outer for-loop.
        total = total.checked_add(1).ok_or(ChdError::InvalidData)?;
    }
}

/// Format a geometry record exactly as chdman writes it:
/// `"CYLS:%d,HEADS:%d,SECS:%d,BPS:%d"`. The shim writes a NUL terminator
/// alongside, matching MAME's `write_metadata` convention.
pub fn format_gddd(g: HdGeometry) -> Vec<u8> {
    let mut s = format!(
        "CYLS:{},HEADS:{},SECS:{},BPS:{}",
        g.cylinders, g.heads, g.sectors, g.sector_bytes
    )
    .into_bytes();
    s.push(0);
    s
}

/// Parse a `GDDD` payload back into geometry. Tolerant to a trailing
/// NUL (which `format_gddd` always writes, and chdman writes too).
pub fn read_geometry(chd: &Chd) -> Result<HdGeometry> {
    let raw = chd.read_metadata(HARD_DISK_METADATA_TAG, 0)?;
    let s = std::str::from_utf8(&raw)
        .map_err(|_| ChdError::InvalidMetadata)?
        .trim_end_matches('\0');
    let parts: Vec<_> = s.split(',').collect();
    if parts.len() != 4 {
        return Err(ChdError::InvalidMetadata);
    }
    fn parse_kv(s: &str, prefix: &str) -> Result<u32> {
        s.strip_prefix(prefix)
            .ok_or(ChdError::InvalidMetadata)?
            .parse::<u32>()
            .map_err(|_| ChdError::InvalidMetadata)
    }
    Ok(HdGeometry {
        cylinders: parse_kv(parts[0], "CYLS:")?,
        heads: parse_kv(parts[1], "HEADS:")?,
        sectors: parse_kv(parts[2], "SECS:")?,
        sector_bytes: parse_kv(parts[3], "BPS:")?,
    })
}

fn validate_options(o: &HdCreateOptions) -> Result<()> {
    if o.unit_size == 0 || o.hunk_size == 0 {
        return Err(ChdError::InvalidData);
    }
    if !o.hunk_size.is_multiple_of(o.unit_size) {
        return Err(ChdError::InvalidData);
    }
    if !o.logical_size.is_multiple_of(u64::from(o.unit_size)) {
        return Err(ChdError::InvalidData);
    }
    Ok(())
}

/// Stream `reader` into a hard-disk CHD at `out_path`.
///
/// The reader is consumed sequentially, never seeked, and never
/// buffered in full — only one hunk at a time. If it ends before
/// `opts.logical_size` bytes are produced, the tail is zero-padded.
///
/// On cancellation (i.e. `cancel()` returns true between hunks): drops
/// the partial output file and returns [`ChdError::Cancelled`].
pub fn create_from_reader<R: Read>(
    reader: R,
    out_path: &Path,
    opts: HdCreateOptions,
    progress: &mut dyn FnMut(CompressionProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    validate_options(&opts)?;

    let geom = match opts.geometry {
        Some(g) => g,
        None => compute_chs(opts.logical_size, opts.unit_size)?,
    };

    let source = StreamingSource::new(reader, opts.logical_size);
    let mut compressor = ChdCompressor::new(source);
    let path_str = out_path.to_str().ok_or(ChdError::InvalidFile)?.to_string();
    compressor.create_file(
        &path_str,
        opts.logical_size,
        opts.hunk_size,
        opts.unit_size,
        opts.codecs,
    )?;

    // Metadata must be written before compress_begin spins up workers,
    // so it lives in the on-disk header. chdman writes GDDD then IDNT
    // in that order; preserve.
    write_compressor_metadata(
        &mut compressor,
        HARD_DISK_METADATA_TAG,
        0,
        &format_gddd(geom),
        1,
    )?;
    if let Some(ident) = &opts.ident {
        write_compressor_metadata(&mut compressor, HARD_DISK_IDENT_METADATA_TAG, 0, ident, 1)?;
    }

    run_compression(compressor, out_path, progress, cancel)
}

/// File-input convenience. Equivalent to opening `in_path` and calling
/// [`create_from_reader`].
pub fn create_from_path(
    in_path: &Path,
    out_path: &Path,
    opts: HdCreateOptions,
    progress: &mut dyn FnMut(CompressionProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let mut effective = opts;
    if effective.logical_size == 0 {
        // Caller didn't override: take the file's size.
        let meta = std::fs::metadata(in_path).map_err(|_| ChdError::InvalidFile)?;
        effective.logical_size = meta.len();
    }
    let f = File::open(in_path).map_err(|_| ChdError::InvalidFile)?;
    create_from_reader(f, out_path, effective, progress, cancel)
}

/// Stream the logical contents of `chd_path` to `writer`. Reports bytes
/// written via the progress callback.
pub fn extract_to_writer<W: Write>(
    chd_path: &Path,
    mut writer: W,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let chd = Chd::open(chd_path.to_str().ok_or(ChdError::InvalidFile)?, false, None)?;
    let total = chd.logical_bytes();
    let hunk = chd.hunk_bytes() as usize;
    let mut buf = vec![0u8; hunk];
    let mut written: u64 = 0;
    let mut offset: u64 = 0;
    while offset < total {
        let chunk = std::cmp::min(hunk as u64, total - offset) as usize;
        chd.read_bytes(offset, &mut buf[..chunk])?;
        writer
            .write_all(&buf[..chunk])
            .map_err(|_| ChdError::CompressionError)?;
        offset += chunk as u64;
        written += chunk as u64;
        progress(written);
    }
    Ok(())
}

/// File-output convenience for [`extract_to_writer`].
pub fn extract_to_path(
    chd_path: &Path,
    out_path: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let f = File::create(out_path).map_err(|_| ChdError::InvalidFile)?;
    extract_to_writer(chd_path, f, progress)
}

// `ChdCompressor` has no public `write_metadata`; reach through the
// shim using its underlying chd_file pointer. The compressor is a
// subclass, so the cast is safe.
fn write_compressor_metadata(
    compressor: &mut ChdCompressor,
    tag: u32,
    index: u32,
    data: &[u8],
    flags: u8,
) -> Result<()> {
    // Internal: ChdCompressor wraps the C++ chd_file_compressor which
    // inherits from chd_file. The shim uses the same chd_file_t pointer
    // type, so we can call chd_shim_write_metadata against it.
    let raw = compressor.as_chd_file_ptr();
    let err = unsafe {
        sys::chd_shim_write_metadata(
            raw,
            tag,
            index,
            data.as_ptr() as *const _,
            data.len() as u32,
            flags,
        )
    };
    if err == ChdError::NoError {
        Ok(())
    } else {
        Err(err)
    }
}
