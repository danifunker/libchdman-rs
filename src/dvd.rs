//! DVD CHDs (chdman `createdvd` / `extractdvd` parity, MAME 0.287+).
//!
//! Simpler than CDs: flat 2048-byte sectors, no ECC synthesis, no
//! tracks. Just streamed compression with a single empty `DVD `
//! metadata record so MAME's `check_is_dvd` recognises the file.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::enhancements::metadata::tags::DVD_METADATA_TAG;
use crate::streaming::{run_compression, StreamingSource};
use crate::{
    sys, Chd, ChdCompressor, ChdError, CompressionProgress, Result, CHD_CODEC_FLAC,
    CHD_CODEC_HUFF, CHD_CODEC_LZMA, CHD_CODEC_ZLIB,
};

/// DVD logical sector size in bytes.
pub const DVD_SECTOR_SIZE: u32 = 2048;

/// chdman's default hunk size for DVDs (`2 * 2048 = 4096`).
pub const DEFAULT_HUNK_SIZE: u32 = 2 * DVD_SECTOR_SIZE;

/// Options for DVD CHD creation.
#[derive(Debug, Clone)]
pub struct DvdCreateOptions {
    /// Total bytes to consume from the source. Must be a multiple of
    /// 2048. The reader is zero-padded if it ends earlier.
    pub logical_size: u64,
    /// Hunk size in bytes. Defaults to chdman's `2 * 2048`. Must be a
    /// multiple of 2048.
    pub hunk_size: u32,
    /// Codec slots. Default mirrors chdman's `s_default_hd_compression`
    /// (chdman.cpp:666) which `do_create_dvd` reuses verbatim.
    pub codecs: [u32; 4],
}

impl Default for DvdCreateOptions {
    fn default() -> Self {
        Self {
            logical_size: 0,
            hunk_size: DEFAULT_HUNK_SIZE,
            codecs: [
                CHD_CODEC_LZMA,
                CHD_CODEC_ZLIB,
                CHD_CODEC_HUFF,
                CHD_CODEC_FLAC,
            ],
        }
    }
}

fn validate(opts: &DvdCreateOptions) -> Result<()> {
    if opts.hunk_size == 0 || opts.hunk_size % DVD_SECTOR_SIZE != 0 {
        return Err(ChdError::InvalidData);
    }
    if opts.logical_size == 0 || opts.logical_size % u64::from(DVD_SECTOR_SIZE) != 0 {
        return Err(ChdError::InvalidData);
    }
    Ok(())
}

/// Stream `reader` into a DVD CHD. Equivalent to `chdman createdvd`.
///
/// `opts.logical_size` must be set to the exact byte count to consume,
/// and must be a multiple of 2048. If the reader ends before then, the
/// tail is zero-padded — same as `hd::create_from_reader`.
pub fn create_from_reader<R: Read>(
    reader: R,
    out_path: &Path,
    opts: DvdCreateOptions,
    progress: &mut dyn FnMut(CompressionProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    validate(&opts)?;
    let source = StreamingSource::new(reader, opts.logical_size);
    let mut compressor = ChdCompressor::new(source);
    let path_str = out_path
        .to_str()
        .ok_or(ChdError::InvalidFile)?
        .to_string();
    compressor.create_file(
        &path_str,
        opts.logical_size,
        opts.hunk_size,
        DVD_SECTOR_SIZE,
        opts.codecs,
    )?;

    // Empty payload — matches chdman.cpp:2289. The presence of the tag
    // is what `check_is_dvd` looks for.
    // chdman writes an empty C string (`""`) — i.e. a valid pointer to
    // a single NUL byte rather than a null buffer. MAME's
    // `chd_file::write_metadata` rejects null even for zero-length
    // payloads.
    // chdman writes `chd->write_metadata(DVD_METADATA_TAG, 0, "")` which
    // hits MAME's std::string overload — that one stores `length + 1`
    // bytes to include the NUL terminator. So the on-disk payload is a
    // single zero byte, not zero bytes. Mirror that exactly so the
    // header matches chdman's output.
    let empty_terminator: [u8; 1] = [0];
    let err = unsafe {
        sys::chd_shim_write_metadata(
            compressor.as_chd_file_ptr(),
            DVD_METADATA_TAG,
            0,
            empty_terminator.as_ptr() as *const _,
            1,
            1,
        )
    };
    if err != ChdError::NoError {
        return Err(err);
    }

    run_compression(compressor, out_path, progress, cancel)
}

/// File-input convenience for [`create_from_reader`]. Pulls the file
/// size from the filesystem and uses it as `logical_size` if the caller
/// didn't supply one.
pub fn create_from_iso(
    iso_path: &Path,
    out_path: &Path,
    opts: DvdCreateOptions,
    progress: &mut dyn FnMut(CompressionProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let mut effective = opts;
    if effective.logical_size == 0 {
        let meta = std::fs::metadata(iso_path).map_err(|_| ChdError::InvalidFile)?;
        effective.logical_size = meta.len();
    }
    let f = File::open(iso_path).map_err(|_| ChdError::InvalidFile)?;
    create_from_reader(f, out_path, effective, progress, cancel)
}

/// Stream a DVD CHD's logical bytes back out. Rejects CHDs that lack
/// the `DVD ` metadata tag — for HD/raw CHDs use `hd::extract_to_writer`
/// and for CDs use `cd::extract_to_*`.
pub fn extract_to_writer<W: Write>(
    chd_path: &Path,
    mut writer: W,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let chd = Chd::open(
        chd_path.to_str().ok_or(ChdError::InvalidFile)?,
        false,
        None,
    )?;
    let info = chd.info()?;
    if !info.is_dvd {
        return Err(ChdError::UnsupportedFormat);
    }
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
pub fn extract_to_iso(
    chd_path: &Path,
    iso_path: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let f = File::create(iso_path).map_err(|_| ChdError::InvalidFile)?;
    extract_to_writer(chd_path, f, progress)
}
