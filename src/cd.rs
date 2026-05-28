//! CD-ROM CHDs (chdman `createcd` / `extractcd` parity).
//!
//! All CD-format logic — CUE parsing, track padding, ECC/EDC synthesis,
//! audio byte-swap, CHT2 metadata records — is delegated to MAME via
//! FFI shims (see `sys/cd_shim.cpp`). This module is a thin Rust facade.
//!
//! Create: `create_from_cue` (CUE/GDI/ISO/Nero inputs) and
//! `create_from_iso`. Extract: `extract_to_cue` (CUE/BIN),
//! `extract_to_iso` (single data track), and `extract_to_gdi` (Sega
//! GD-ROM index + per-track split files). `list_tracks` reports the TOC.

use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::streaming::run_compression;
use crate::{
    sys, Chd, ChdCompressor, ChdError, CompressionProgress, Result, CHD_CODEC_CD_FLAC,
    CHD_CODEC_CD_LZMA, CHD_CODEC_CD_ZLIB,
};

/// CD frame layout: 2352 bytes of sector data + 96 bytes of subcode.
pub const CD_FRAME_SIZE: u32 = 2448;
/// Default frames per hunk (matches `cdrom_file::FRAMES_PER_HUNK`).
pub const FRAMES_PER_HUNK: u32 = 8;
/// Default hunk size for CD CHDs (8 frames * 2448 bytes = 19584).
pub const DEFAULT_HUNK_SIZE: u32 = FRAMES_PER_HUNK * CD_FRAME_SIZE;

/// Track type, mirroring `cdrom_file::CD_TRACK_*`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackType {
    Mode1 = 0,
    Mode1Raw = 1,
    Mode2 = 2,
    Mode2Form1 = 3,
    Mode2Form2 = 4,
    Mode2FormMix = 5,
    Mode2Raw = 6,
    Audio = 7,
}

impl TrackType {
    pub fn from_raw(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Mode1),
            1 => Some(Self::Mode1Raw),
            2 => Some(Self::Mode2),
            3 => Some(Self::Mode2Form1),
            4 => Some(Self::Mode2Form2),
            5 => Some(Self::Mode2FormMix),
            6 => Some(Self::Mode2Raw),
            7 => Some(Self::Audio),
            _ => None,
        }
    }
}

/// Subchannel encoding, mirroring `cdrom_file::CD_SUB_*`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubcodeType {
    /// "Cooked" 96 bytes per sector.
    Normal = 0,
    /// Raw uninterleaved 96 bytes per sector.
    Raw = 1,
    /// No subcode data stored.
    None = 2,
}

impl SubcodeType {
    pub fn from_raw(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Normal),
            1 => Some(Self::Raw),
            2 => Some(Self::None),
            _ => None,
        }
    }
}

/// Per-track summary from a parsed CUE / TOC or from a CHD's metadata.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub track_num: u32,
    pub track_type: TrackType,
    pub subcode_type: SubcodeType,
    pub frames: u32,
    pub pregap: u32,
    pub postgap: u32,
    pub pregap_type: TrackType,
    pub pregap_subcode: SubcodeType,
}

impl TrackInfo {
    fn from_raw(track_num: u32, raw: sys::ChdShimTrack) -> Self {
        Self {
            track_num,
            track_type: TrackType::from_raw(raw.trktype).unwrap_or(TrackType::Mode1),
            subcode_type: SubcodeType::from_raw(raw.subtype).unwrap_or(SubcodeType::None),
            frames: raw.frames,
            pregap: raw.pregap,
            postgap: raw.postgap,
            pregap_type: TrackType::from_raw(raw.pgtype).unwrap_or(TrackType::Mode1),
            pregap_subcode: SubcodeType::from_raw(raw.pgsub).unwrap_or(SubcodeType::None),
        }
    }
}

/// Options for [`create_from_cue`] / [`create_from_iso`].
#[derive(Debug, Clone)]
pub struct CdCreateOptions {
    /// Hunk size in bytes. Default `19584` (8 frames * 2448).
    pub hunk_size: u32,
    /// Codec slots. Default `[cdlz, cdzl, cdfl, 0]` — chdman's `s_default_cd_compression`.
    pub codecs: [u32; 4],
}

impl Default for CdCreateOptions {
    fn default() -> Self {
        Self {
            hunk_size: DEFAULT_HUNK_SIZE,
            codecs: [CHD_CODEC_CD_LZMA, CHD_CODEC_CD_ZLIB, CHD_CODEC_CD_FLAC, 0],
        }
    }
}

/// RAII handle around the C++ TOC.
struct Toc {
    inner: *mut sys::ChdShimToc,
}

impl Toc {
    fn parse(path: &Path) -> Result<Self> {
        let inner = unsafe { sys::chd_shim_toc_alloc() };
        if inner.is_null() {
            return Err(ChdError::InvalidFile);
        }
        let c_path = CString::new(path.to_str().ok_or(ChdError::InvalidFile)?)
            .map_err(|_| ChdError::InvalidFile)?;
        let err = unsafe { sys::chd_shim_toc_parse(inner, c_path.as_ptr()) };
        if err != ChdError::NoError {
            unsafe { sys::chd_shim_toc_free(inner) };
            return Err(err);
        }
        Ok(Self { inner })
    }

    fn pad_tracks(&mut self) {
        unsafe { sys::chd_shim_toc_pad_tracks(self.inner) };
    }

    fn logical_bytes(&self) -> u64 {
        unsafe { sys::chd_shim_toc_logical_bytes(self.inner) }
    }
}

impl Drop for Toc {
    fn drop(&mut self) {
        unsafe { sys::chd_shim_toc_free(self.inner) };
    }
}

/// Parse a CUE/TOC and create a CD CHD at `out_path`.
///
/// The CUE parser is MAME's (`cdrom_file::parse_toc`), which dispatches
/// on extension and content to handle CUE, GDI, ISO, and Nero TOC
/// formats. Track frames are padded to a 4-frame boundary the same way
/// chdman does. Audio tracks are big-endian byte-swapped on the way in
/// when the input format requires it (CUE BINARY vs MOTOROLA).
pub fn create_from_cue(
    cue_path: &Path,
    out_path: &Path,
    opts: CdCreateOptions,
    progress: &mut dyn FnMut(CompressionProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let mut toc = Toc::parse(cue_path)?;
    toc.pad_tracks();
    let logical_bytes = toc.logical_bytes();
    if logical_bytes == 0 {
        return Err(ChdError::InvalidData);
    }

    // Hand the toc to the MAME-side CdCompressor. ChdCompressor takes
    // ownership of the underlying ChdFileCompressor pointer, so we
    // hand-roll the alloc here rather than using ChdCompressor::new.
    let raw_compressor = unsafe { sys::chd_shim_cd_compressor_alloc(toc.inner) };
    if raw_compressor.is_null() {
        return Err(ChdError::InvalidFile);
    }
    let mut compressor = ChdCompressor::from_raw(raw_compressor, logical_bytes);

    let path_str = out_path.to_str().ok_or(ChdError::InvalidFile)?.to_string();
    compressor.create_file(
        &path_str,
        logical_bytes,
        opts.hunk_size,
        CD_FRAME_SIZE,
        opts.codecs,
    )?;

    let err = unsafe { sys::chd_shim_cd_write_metadata(compressor.as_chd_file_ptr(), toc.inner) };
    if err != ChdError::NoError {
        return Err(err);
    }

    // Toc must outlive the compressor — the C++ CdCompressor holds
    // references into it. Move toc into a guard that drops AFTER
    // run_compression returns.
    let result = run_compression(compressor, out_path, progress, cancel);
    drop(toc);
    result
}

/// Convenience: build an in-memory single-track MODE1/2048 CUE that
/// references `iso_path`, then call [`create_from_cue`] semantics.
///
/// We avoid writing a temp CUE file by writing one to a `tempfile::NamedTempFile`
/// adjacent to the ISO. MAME's parse_toc is path-based, so a real on-disk
/// CUE is required; we just don't pollute the user's directory.
pub fn create_from_iso(
    iso_path: &Path,
    out_path: &Path,
    opts: CdCreateOptions,
    progress: &mut dyn FnMut(CompressionProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let iso_name = iso_path
        .file_name()
        .ok_or(ChdError::InvalidFile)?
        .to_str()
        .ok_or(ChdError::InvalidFile)?;
    // Build the CUE next to the ISO so the relative FILE path resolves.
    let parent = iso_path.parent().ok_or(ChdError::InvalidFile)?;
    let temp_cue = tempfile::Builder::new()
        .prefix(".libchdman-rs-cue-")
        .suffix(".cue")
        .tempfile_in(parent)
        .map_err(|_| ChdError::InvalidFile)?;
    {
        let mut f = temp_cue.as_file();
        let cue = format!(
            "FILE \"{}\" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\n",
            iso_name
        );
        f.write_all(cue.as_bytes())
            .map_err(|_| ChdError::InvalidFile)?;
        f.flush().map_err(|_| ChdError::InvalidFile)?;
    }
    create_from_cue(temp_cue.path(), out_path, opts, progress, cancel)
}

/// Per-frame size MAME emits for raw read_data.
const RAW_SECTOR_SIZE: usize = 2352;
/// Cooked MODE1 user data, what's in a standard ISO9660 image.
const COOKED_MODE1_SIZE: usize = 2048;

/// Format a frame count as MSF (`MM:SS:FF`, 75 frames/sec).
fn msf_string(frames: u32) -> String {
    let m = frames / (75 * 60);
    let s = (frames / 75) % 60;
    let f = frames % 75;
    format!("{:02}:{:02}:{:02}", m, s, f)
}

/// Extract a CD CHD to a CUE/BIN pair. Writes a single combined `.bin`
/// at `bin_path` and a CUE file at `cue_path` referencing it. Audio
/// tracks are byte-swapped back to little-endian on the way out
/// (matching what chdman's `do_extract_cd` does for `MODE_CUEBIN`).
///
/// The CUE TRACK lines mirror chdman's `output_track_metadata`:
/// `FILE "<bin>" BINARY` once at the top, then `TRACK NN MODE1/2352`
/// (or `MODE2/####` or `AUDIO`) per track with appropriate `INDEX 01`
/// (and `INDEX 00` if pregap data is present) and `PREGAP`/`POSTGAP`.
///
/// Tracks with stored subcode are silently dropped from the output
/// (chdman warns; we just omit) — bin/cue cannot represent subcode.
pub fn extract_to_cue(
    chd_path: &Path,
    cue_path: &Path,
    bin_path: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let chd = Chd::open(chd_path.to_str().ok_or(ChdError::InvalidFile)?, false, None)?;
    let raw_chd = chd.raw_ptr();

    let cdrom = unsafe { sys::chd_shim_cdrom_open(raw_chd) };
    if cdrom.is_null() {
        return Err(ChdError::InvalidData);
    }
    // RAII guard so we always free on error.
    struct CdromGuard(*mut sys::ChdShimCdrom);
    impl Drop for CdromGuard {
        fn drop(&mut self) {
            unsafe { sys::chd_shim_cdrom_free(self.0) };
        }
    }
    let _guard = CdromGuard(cdrom);

    let n_tracks = unsafe { sys::chd_shim_cdrom_num_tracks(cdrom) };
    if n_tracks == 0 {
        return Err(ChdError::InvalidData);
    }

    // Open both outputs up front — easier cleanup if one fails.
    let bin_file = File::create(bin_path).map_err(|_| ChdError::InvalidFile)?;
    let mut bin_writer = BufWriter::with_capacity(64 * 1024, bin_file);
    let cue_file = File::create(cue_path).map_err(|_| ChdError::InvalidFile)?;
    let mut cue_writer = BufWriter::new(cue_file);

    let bin_filename = bin_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(ChdError::InvalidFile)?;
    writeln!(cue_writer, "FILE \"{}\" BINARY", bin_filename).map_err(|_| ChdError::InvalidFile)?;

    let mut sector = vec![0u8; RAW_SECTOR_SIZE];
    let mut written: u64 = 0;
    let mut frame_offset: u32 = 0;

    for tracknum in 0..n_tracks {
        let mut t = sys::ChdShimTrack::default();
        unsafe { sys::chd_shim_cdrom_get_track(cdrom, tracknum, &mut t) };
        let track_start = unsafe { sys::chd_shim_cdrom_get_track_start(cdrom, tracknum) };
        let trktype = TrackType::from_raw(t.trktype).unwrap_or(TrackType::Mode1);
        let subtype = SubcodeType::from_raw(t.subtype).unwrap_or(SubcodeType::None);

        // CUE TRACK / INDEX block.
        let mode = match trktype {
            TrackType::Mode1 | TrackType::Mode1Raw => format!("MODE1/{:04}", t.datasize),
            TrackType::Mode2
            | TrackType::Mode2Form1
            | TrackType::Mode2Form2
            | TrackType::Mode2FormMix
            | TrackType::Mode2Raw => format!("MODE2/{:04}", t.datasize),
            TrackType::Audio => "AUDIO".to_string(),
        };
        writeln!(cue_writer, "  TRACK {:02} {}", tracknum + 1, mode)
            .map_err(|_| ChdError::InvalidFile)?;

        if t.pregap > 0 && t.pgdatasize == 0 {
            writeln!(cue_writer, "    PREGAP {}", msf_string(t.pregap))
                .map_err(|_| ChdError::InvalidFile)?;
            writeln!(cue_writer, "    INDEX 01 {}", msf_string(frame_offset))
                .map_err(|_| ChdError::InvalidFile)?;
        } else if t.pregap > 0 && t.pgdatasize > 0 {
            writeln!(cue_writer, "    INDEX 00 {}", msf_string(frame_offset))
                .map_err(|_| ChdError::InvalidFile)?;
            writeln!(
                cue_writer,
                "    INDEX 01 {}",
                msf_string(frame_offset + t.pregap)
            )
            .map_err(|_| ChdError::InvalidFile)?;
        } else {
            writeln!(cue_writer, "    INDEX 01 {}", msf_string(frame_offset))
                .map_err(|_| ChdError::InvalidFile)?;
        }
        if t.postgap > 0 {
            writeln!(cue_writer, "    POSTGAP {}", msf_string(t.postgap))
                .map_err(|_| ChdError::InvalidFile)?;
        }

        // Frame loop: emit `frames - padframes + splitframes` (matches
        // chdman.cpp:2968), in this track's stored sector size.
        let actual_frames = t
            .frames
            .saturating_sub(t.padframes)
            .saturating_add(t.splitframes);
        let drop_subcode = subtype != SubcodeType::None;
        if drop_subcode {
            // Subcode is intentionally dropped here (bin/cue can't carry it).
        }
        for f in 0..actual_frames {
            let lba = track_start + f;
            let ok = unsafe {
                sys::chd_shim_cdrom_read_data(
                    cdrom,
                    lba,
                    sector.as_mut_ptr() as *mut _,
                    t.trktype,
                    1, // phys=true: read at the physical CHD frame, like chdman
                )
            };
            if ok == 0 {
                return Err(ChdError::InvalidData);
            }

            let bytes_to_write = t.datasize as usize;
            // Audio: byte-swap back to little-endian for CUE BINARY.
            if trktype == TrackType::Audio {
                for i in (0..bytes_to_write).step_by(2) {
                    sector.swap(i, i + 1);
                }
            }
            bin_writer
                .write_all(&sector[..bytes_to_write])
                .map_err(|_| ChdError::InvalidFile)?;
            written += bytes_to_write as u64;
            progress(written);
        }

        frame_offset += t.frames;
    }

    bin_writer.flush().map_err(|_| ChdError::InvalidFile)?;
    cue_writer.flush().map_err(|_| ChdError::InvalidFile)?;
    Ok(())
}

/// Extract a single-track MODE1/MODE1_RAW CHD to a 2048-byte/sector ISO.
///
/// Returns an error if the CHD has more than one track, or if the
/// single track is not MODE1 / MODE1_RAW. For multi-track or
/// audio-bearing CHDs use [`extract_to_cue`].
pub fn extract_to_iso(
    chd_path: &Path,
    iso_path: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let chd = Chd::open(chd_path.to_str().ok_or(ChdError::InvalidFile)?, false, None)?;
    let tracks = list_tracks(&chd)?;
    if tracks.len() != 1 {
        return Err(ChdError::UnsupportedFormat);
    }
    let track = &tracks[0];
    match track.track_type {
        TrackType::Mode1 | TrackType::Mode1Raw => {}
        _ => return Err(ChdError::UnsupportedFormat),
    }

    let raw_chd = chd.raw_ptr();
    let cdrom = unsafe { sys::chd_shim_cdrom_open(raw_chd) };
    if cdrom.is_null() {
        return Err(ChdError::InvalidData);
    }
    struct CdromGuard(*mut sys::ChdShimCdrom);
    impl Drop for CdromGuard {
        fn drop(&mut self) {
            unsafe { sys::chd_shim_cdrom_free(self.0) };
        }
    }
    let _guard = CdromGuard(cdrom);

    let track_start = unsafe { sys::chd_shim_cdrom_get_track_start(cdrom, 0) };
    let f = File::create(iso_path).map_err(|_| ChdError::InvalidFile)?;
    let mut writer = BufWriter::with_capacity(64 * 1024, f);

    // Datatype CD_TRACK_MODE1 (= 0) tells MAME to extract 2048 user
    // bytes regardless of whether the CHD stored the track as raw 2352
    // or cooked 2048 — saves us doing the sync/header/ECC strip in
    // Rust.
    let mut sector = vec![0u8; COOKED_MODE1_SIZE];
    let mut written: u64 = 0;
    for f_idx in 0..track.frames {
        let ok = unsafe {
            sys::chd_shim_cdrom_read_data(
                cdrom,
                track_start + f_idx,
                sector.as_mut_ptr() as *mut _,
                TrackType::Mode1 as u32,
                1,
            )
        };
        if ok == 0 {
            return Err(ChdError::InvalidData);
        }
        writer
            .write_all(&sector)
            .map_err(|_| ChdError::InvalidFile)?;
        written += COOKED_MODE1_SIZE as u64;
        progress(written);
    }
    writer.flush().map_err(|_| ChdError::InvalidFile)?;
    Ok(())
}

/// Extract a CD/GD-ROM CHD to a `.gdi` index plus per-track split files,
/// mirroring chdman's `extractcd` GDI output (`MODE_GDI` in
/// `do_extract_cd`).
///
/// Writes `gdi_path` — first line is the track count, then one
/// `num lba type datasize "file" offset` line per track (`type` 0 =
/// audio, 4 = data; `offset` is always 0 since each track gets its own
/// file). Alongside it, one file per track is written named
/// `<stem>NN.bin` (data) or `<stem>NN.raw` (audio), where `<stem>` is
/// `gdi_path` with its extension removed and `NN` is the zero-padded
/// track number — the same `%02t` scheme chdman uses.
///
/// Audio tracks are byte-swapped to little-endian for CHD version > 4
/// (matching chdman: GD-ROM v5+ CHDs store audio big-endian). Frames
/// belonging to a track but physically stored at the tail of the
/// previous track (`splitframes`) are pulled across the boundary, the
/// reverse of how the GD-ROM CHD was built. Subcode is dropped — GDI
/// cannot represent it.
pub fn extract_to_gdi(
    chd_path: &Path,
    gdi_path: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let chd = Chd::open(chd_path.to_str().ok_or(ChdError::InvalidFile)?, false, None)?;
    let raw_chd = chd.raw_ptr();
    let version = unsafe { sys::chd_shim_version(raw_chd) };

    let cdrom = unsafe { sys::chd_shim_cdrom_open(raw_chd) };
    if cdrom.is_null() {
        return Err(ChdError::InvalidData);
    }
    struct CdromGuard(*mut sys::ChdShimCdrom);
    impl Drop for CdromGuard {
        fn drop(&mut self) {
            unsafe { sys::chd_shim_cdrom_free(self.0) };
        }
    }
    let _guard = CdromGuard(cdrom);

    let n_tracks = unsafe { sys::chd_shim_cdrom_num_tracks(cdrom) };
    if n_tracks == 0 {
        return Err(ChdError::InvalidData);
    }

    // Output stem: gdi path with extension stripped. Track files are
    // siblings named "<stem>NN.<ext>".
    let stem = gdi_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or(ChdError::InvalidFile)?;
    let dir = gdi_path.parent().unwrap_or_else(|| Path::new("."));

    let mut tracks = Vec::with_capacity(n_tracks as usize);
    for i in 0..n_tracks {
        let mut t = sys::ChdShimTrack::default();
        unsafe { sys::chd_shim_cdrom_get_track(cdrom, i, &mut t) };
        tracks.push(t);
    }

    let gdi_file = File::create(gdi_path).map_err(|_| ChdError::InvalidFile)?;
    let mut gdi_writer = BufWriter::new(gdi_file);
    writeln!(gdi_writer, "{}", n_tracks).map_err(|_| ChdError::InvalidFile)?;

    let mut sector = vec![0u8; RAW_SECTOR_SIZE];
    let mut written: u64 = 0;
    // GDI LBA: accumulates across tracks (chdman never resets discoffs in
    // MODE_GDI). This is the per-track frame offset written to the index.
    let mut disc_offs: u32 = 0;

    for tracknum in 0..n_tracks as usize {
        let t = tracks[tracknum];
        let trktype = TrackType::from_raw(t.trktype).unwrap_or(TrackType::Mode1);
        let is_audio = trktype == TrackType::Audio;
        let ext = if is_audio { "raw" } else { "bin" };
        let track_filename = format!("{}{:02}.{}", stem, tracknum + 1, ext);
        let track_path = dir.join(&track_filename);

        // GDI index line: track# lba type datasize "file" offset.
        let gdi_type = if is_audio { 0 } else { 4 };
        let q = if track_filename.contains(' ') {
            "\""
        } else {
            ""
        };
        writeln!(
            gdi_writer,
            "{} {} {} {} {}{}{} 0",
            tracknum + 1,
            disc_offs,
            gdi_type,
            t.datasize,
            q,
            track_filename,
            q
        )
        .map_err(|_| ChdError::InvalidFile)?;

        let track_file = File::create(&track_path).map_err(|_| ChdError::InvalidFile)?;
        let mut track_writer = BufWriter::with_capacity(64 * 1024, track_file);

        let cur_phys = unsafe { sys::chd_shim_cdrom_get_track_start_phys(cdrom, tracknum as u32) };
        let prev_phys = if tracknum > 0 {
            unsafe { sys::chd_shim_cdrom_get_track_start_phys(cdrom, (tracknum - 1) as u32) }
        } else {
            0
        };

        let actual_frames = t
            .frames
            .saturating_sub(t.padframes)
            .saturating_add(t.splitframes);

        for frame in 0..actual_frames {
            // The first `splitframes` frames of this track live at the
            // tail of the previous track; pull them from there.
            let (src, lba) = if tracknum > 0 && frame < t.splitframes {
                let prev = tracks[tracknum - 1];
                let frameofs = prev.frames.wrapping_sub(t.splitframes).wrapping_add(frame);
                (prev, prev_phys.wrapping_add(frameofs))
            } else {
                let frameofs = frame.wrapping_sub(t.splitframes);
                (t, cur_phys.wrapping_add(frameofs))
            };
            let ok = unsafe {
                sys::chd_shim_cdrom_read_data(
                    cdrom,
                    lba,
                    sector.as_mut_ptr() as *mut _,
                    src.trktype,
                    1,
                )
            };
            if ok == 0 {
                return Err(ChdError::InvalidData);
            }
            let bytes = src.datasize as usize;
            // Audio in GDI + CHD v5+ is stored big-endian; swap to LE.
            if src.trktype == TrackType::Audio as u32 && version > 4 {
                for i in (0..bytes).step_by(2) {
                    sector.swap(i, i + 1);
                }
            }
            track_writer
                .write_all(&sector[..bytes])
                .map_err(|_| ChdError::InvalidFile)?;
            written += bytes as u64;
            progress(written);
            disc_offs = disc_offs.wrapping_add(1);
        }
        track_writer.flush().map_err(|_| ChdError::InvalidFile)?;
        disc_offs = disc_offs.wrapping_add(t.padframes);
    }
    gdi_writer.flush().map_err(|_| ChdError::InvalidFile)?;
    Ok(())
}

/// Read CHT2/CHTR/CHGD track records from a CD/GD CHD.
///
/// Walks the CHD's metadata via MAME's `cdrom_file` reader so we get
/// the parsed `track_info` directly, regardless of whether records were
/// stored as CHT2 (modern) or CHTR (legacy).
pub fn list_tracks(chd: &Chd) -> Result<Vec<TrackInfo>> {
    let raw_chd = chd.raw_ptr();
    let cdrom = unsafe { sys::chd_shim_cdrom_open(raw_chd) };
    if cdrom.is_null() {
        return Err(ChdError::InvalidData);
    }
    let n = unsafe { sys::chd_shim_cdrom_num_tracks(cdrom) };
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let mut t = sys::ChdShimTrack::default();
        unsafe { sys::chd_shim_cdrom_get_track(cdrom, i, &mut t) };
        out.push(TrackInfo::from_raw(i + 1, t));
    }
    unsafe { sys::chd_shim_cdrom_free(cdrom) };
    Ok(out)
}

// Internal: ChdCompressor needs a constructor that takes a pre-built
// raw pointer (since CD allocation is custom-driven).
impl ChdCompressor {
    pub(crate) fn from_raw(inner: *mut sys::ChdFileCompressor, logical_bytes: u64) -> Self {
        Self {
            inner,
            logical_bytes,
        }
    }
}

// Helper used above: expose the raw chd_file pointer for FFI consumers.
impl Chd {
    pub(crate) fn raw_ptr(&self) -> *mut sys::ChdFile {
        self.inner
    }
}

#[allow(dead_code)]
fn track_type_str(t: TrackType) -> &'static str {
    // Mirrors cdrom_file::cdrom_get_type_string. Used when we hand-roll
    // metadata strings; for now MAME writes them via write_metadata.
    match t {
        TrackType::Mode1 => "MODE1",
        TrackType::Mode1Raw => "MODE1_RAW",
        TrackType::Mode2 => "MODE2",
        TrackType::Mode2Form1 => "MODE2_FORM1",
        TrackType::Mode2Form2 => "MODE2_FORM2",
        TrackType::Mode2FormMix => "MODE2_FORM_MIX",
        TrackType::Mode2Raw => "MODE2_RAW",
        TrackType::Audio => "AUDIO",
    }
}

/// `Read + Seek` over the cooked 2048-byte MODE1 user data of a CD CHD track.
///
/// Lets ISO9660 / UDF parsers consume a CD CHD directly without an intermediate
/// extraction to a `.iso` on disk. MAME's `cdrom_file` strips sync, header, and
/// ECC bytes regardless of whether the CHD stored MODE1 raw (2352) or cooked
/// (2048), so the stream length is always `track.frames * 2048`.
///
/// Open with [`CdCookedReader::open`] for single-track CHDs, or
/// [`CdCookedReader::open_track`] to pick a specific track by index in a
/// multi-track CHD. The selected track must be `Mode1` or `Mode1Raw`; audio
/// and Mode 2 variants can't be read as cooked 2048-byte sectors here.
pub struct CdCookedReader {
    chd: Chd,
    cdrom: *mut sys::ChdShimCdrom,
    track_start: u32,
    total_frames: u32,
    pos: u64,
    cache_frame: Option<u32>,
    cache: [u8; COOKED_MODE1_SIZE],
}

unsafe impl Send for CdCookedReader {}

impl CdCookedReader {
    /// Open a single-track CD CHD as a cooked 2048-byte sector stream.
    ///
    /// Errors with [`ChdError::UnsupportedFormat`] if the CHD has more than
    /// one track, or if the single track is not `Mode1` / `Mode1Raw`. For
    /// multi-track CHDs use [`CdCookedReader::open_track`].
    pub fn open(chd: Chd) -> Result<Self> {
        let tracks = list_tracks(&chd)?;
        if tracks.len() != 1 {
            return Err(ChdError::UnsupportedFormat);
        }
        Self::open_track(chd, 0)
    }

    /// Open a specific track of a CHD as a cooked 2048-byte sector stream.
    ///
    /// `track_index` is 0-based. Position 0 of the resulting reader
    /// corresponds to the start of that track's user data, not the start of
    /// the CHD. Multi-track CHDs (PSX / Saturn / mixed-mode PC CDs) store
    /// tracks back-to-back with chdman's 4-frame padding between them; the
    /// underlying offset math is handled by MAME's `cdrom_file`, so callers
    /// just pass the track index.
    ///
    /// Accepted track types (all yield 2048 user bytes per sector via
    /// MAME's per-mode extraction):
    ///
    /// - [`TrackType::Mode1`]
    /// - [`TrackType::Mode1Raw`]
    /// - [`TrackType::Mode2Form1`] (CD-XA Form 1: strips sync + header + subheader)
    /// - [`TrackType::Mode2Raw`] (treated like Form 1; common PSX/Saturn case)
    /// - [`TrackType::Mode2FormMix`] (CD-XA mixed-form: strips subheader only)
    ///
    /// Errors:
    /// - [`ChdError::InvalidData`] if `track_index` is out of range.
    /// - [`ChdError::UnsupportedFormat`] if the selected track is
    ///   [`TrackType::Audio`] (no 2048-byte cooked representation of a
    ///   Red Book audio track), or [`TrackType::Mode2`] / [`TrackType::Mode2Form2`]
    ///   (MAME's `cdrom_file` doesn't define a 2048-byte conversion from these
    ///   — Mode 2 bare carries 2336 user bytes per sector and Form 2 carries
    ///   2324; neither lines up with a 2048-byte stream).
    pub fn open_track(chd: Chd, track_index: u32) -> Result<Self> {
        let tracks = list_tracks(&chd)?;
        if (track_index as usize) >= tracks.len() {
            return Err(ChdError::InvalidData);
        }
        let track = &tracks[track_index as usize];
        match track.track_type {
            TrackType::Mode1
            | TrackType::Mode1Raw
            | TrackType::Mode2Form1
            | TrackType::Mode2Raw
            | TrackType::Mode2FormMix => {}
            TrackType::Audio | TrackType::Mode2 | TrackType::Mode2Form2 => {
                return Err(ChdError::UnsupportedFormat);
            }
        }
        let cdrom = unsafe { sys::chd_shim_cdrom_open(chd.raw_ptr()) };
        if cdrom.is_null() {
            return Err(ChdError::InvalidData);
        }
        let track_start = unsafe { sys::chd_shim_cdrom_get_track_start(cdrom, track_index) };
        Ok(Self {
            chd,
            cdrom,
            track_start,
            total_frames: track.frames,
            pos: 0,
            cache_frame: None,
            cache: [0u8; COOKED_MODE1_SIZE],
        })
    }

    pub fn len(&self) -> u64 {
        self.total_frames as u64 * COOKED_MODE1_SIZE as u64
    }

    pub fn is_empty(&self) -> bool {
        self.total_frames == 0
    }

    /// Recover the underlying [`Chd`].
    pub fn into_inner(mut self) -> Chd {
        unsafe { sys::chd_shim_cdrom_free(self.cdrom) };
        self.cdrom = std::ptr::null_mut();
        let chd = std::mem::take(&mut self.chd);
        std::mem::forget(self);
        chd
    }

    fn load_frame(&mut self, frame: u32) -> io::Result<()> {
        if self.cache_frame == Some(frame) {
            return Ok(());
        }
        let ok = unsafe {
            sys::chd_shim_cdrom_read_data(
                self.cdrom,
                self.track_start + frame,
                self.cache.as_mut_ptr() as *mut _,
                TrackType::Mode1 as u32,
                1,
            )
        };
        if ok == 0 {
            self.cache_frame = None;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "chd_shim_cdrom_read_data failed",
            ));
        }
        self.cache_frame = Some(frame);
        Ok(())
    }
}

impl Drop for CdCookedReader {
    fn drop(&mut self) {
        if !self.cdrom.is_null() {
            unsafe { sys::chd_shim_cdrom_free(self.cdrom) };
        }
    }
}

impl Read for CdCookedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let total = self.len();
        if self.pos >= total || buf.is_empty() {
            return Ok(0);
        }
        let remaining = total - self.pos;
        let want = (buf.len() as u64).min(remaining) as usize;
        let frame = (self.pos / COOKED_MODE1_SIZE as u64) as u32;
        let off = (self.pos % COOKED_MODE1_SIZE as u64) as usize;
        let n = want.min(COOKED_MODE1_SIZE - off);
        self.load_frame(frame)?;
        buf[..n].copy_from_slice(&self.cache[off..off + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for CdCookedReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let total = self.len() as i128;
        let new_pos: i128 = match pos {
            SeekFrom::Start(v) => v as i128,
            SeekFrom::End(v) => total + v as i128,
            SeekFrom::Current(v) => self.pos as i128 + v as i128,
        };
        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

#[allow(dead_code)]
fn cstr_to_string(p: *const std::os::raw::c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(p) }
            .to_str()
            .ok()
            .map(|s| s.to_owned())
    }
}
