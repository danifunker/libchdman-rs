//! Pure-Rust additions on top of the MAME-backed `Chd` core.
//!
//! Everything in this file is *new* to libchdman-rs (not a thin wrapper of
//! a single MAME function). The goal is to provide ergonomic chd-rs-style
//! read-side helpers: a typed `Version`, hunk and metadata iteration,
//! `Read + Seek` adapters, and the well-known constants you would otherwise
//! have to look up in MAME's source.
//!
//! See `docs/chd-rs-feature-mirror-implementation.md` for the gap analysis
//! that motivated each item here, and `docs/migration-from-chd-rs.md` for
//! a porting guide.
//!
//! All items are re-exported from the crate root, so `use libchdman_rs::*`
//! is enough — you do not need to refer to this module directly.

use std::io::{self, BufRead, Read, Seek, SeekFrom};

use crate::sys;
use crate::{Chd, ChdError, Result};

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

/// CHD on-disk format version, in the same shape chd-rs exposes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Version {
    V1,
    V3,
    V4,
    V5,
    /// Any version libchdman has not enumerated. Carries the raw u32.
    Other(u32),
}

impl Version {
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Version::V1,
            3 => Version::V3,
            4 => Version::V4,
            5 => Version::V5,
            other => Version::Other(other),
        }
    }

    pub fn raw(self) -> u32 {
        match self {
            Version::V1 => 1,
            Version::V3 => 3,
            Version::V4 => 4,
            Version::V5 => 5,
            Version::Other(v) => v,
        }
    }
}

// ---------------------------------------------------------------------------
// Header introspection on Chd
// ---------------------------------------------------------------------------

impl Chd {
    /// Typed version (V1/V3/V4/V5).
    pub fn version_typed(&self) -> Version {
        Version::from_raw(self.version())
    }

    /// Iterator over every hunk in order. Each item is a freshly allocated
    /// `Vec<u8>` of size `hunk_bytes()`.
    pub fn hunks(&self) -> HunkIter<'_> {
        HunkIter {
            chd: self,
            next: 0,
            count: self.hunk_count(),
            hunk_bytes: self.hunk_bytes() as usize,
        }
    }

    /// Iterator over every metadata entry in the order MAME stores them.
    pub fn metadata_iter(&self) -> MetadataIter<'_> {
        MetadataIter { chd: self, next: 0 }
    }

    /// Return a [`ChdReader`] view that implements `Read + Seek` over the
    /// CHD's logical contents.
    pub fn reader(&self) -> ChdReader<'_> {
        ChdReader { chd: self, pos: 0 }
    }
}

// ---------------------------------------------------------------------------
// Hunk iteration
// ---------------------------------------------------------------------------

pub struct HunkIter<'a> {
    chd: &'a Chd,
    next: u32,
    count: u32,
    hunk_bytes: usize,
}

impl Iterator for HunkIter<'_> {
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let hunknum = self.next;
        self.next += 1;
        let mut buf = vec![0u8; self.hunk_bytes];
        Some(self.chd.read_hunk(hunknum, &mut buf).map(|_| buf))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.count - self.next) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for HunkIter<'_> {}

// ---------------------------------------------------------------------------
// Metadata iteration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MetadataEntry {
    /// Four-character tag (use `make_tag` to construct or `metadata::tags::*`
    /// for the well-known ones).
    pub tag: u32,
    pub flags: u8,
    pub data: Vec<u8>,
}

pub struct MetadataIter<'a> {
    chd: &'a Chd,
    next: u32,
}

impl Iterator for MetadataIter<'_> {
    type Item = Result<MetadataEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.next;
        let mut tag: u32 = 0;
        let mut flags: u8 = 0;
        let mut size: u32 = 0;

        // First call: query size (and stop on NotFound).
        let err = unsafe {
            sys::chd_shim_metadata_enum(
                self.chd.raw(),
                index,
                &mut tag,
                &mut flags,
                std::ptr::null_mut(),
                0,
                &mut size,
            )
        };
        match err {
            ChdError::NoError => {}
            ChdError::MetadataNotFound => return None,
            other => return Some(Err(other)),
        }

        let mut data = vec![0u8; size as usize];
        let err = unsafe {
            sys::chd_shim_metadata_enum(
                self.chd.raw(),
                index,
                &mut tag,
                &mut flags,
                data.as_mut_ptr() as *mut _,
                size,
                &mut size,
            )
        };
        if err != ChdError::NoError {
            return Some(Err(err));
        }

        self.next += 1;
        Some(Ok(MetadataEntry { tag, flags, data }))
    }
}

// ---------------------------------------------------------------------------
// Read + Seek adapter (whole CHD)
// ---------------------------------------------------------------------------

/// `Read + Seek` view of a CHD's logical contents. Internally backed by
/// `Chd::read_bytes`. Cheap to construct and clone.
pub struct ChdReader<'a> {
    chd: &'a Chd,
    pos: u64,
}

impl ChdReader<'_> {
    pub fn position(&self) -> u64 {
        self.pos
    }
}

impl Read for ChdReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let total = self.chd.logical_bytes();
        if self.pos >= total {
            return Ok(0);
        }
        let max = (total - self.pos).min(buf.len() as u64) as usize;
        if max == 0 {
            return Ok(0);
        }
        self.chd
            .read_bytes(self.pos, &mut buf[..max])
            .map_err(chd_err_to_io)?;
        self.pos += max as u64;
        Ok(max)
    }
}

impl Seek for ChdReader<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let total = self.chd.logical_bytes();
        let new_pos: i128 = match pos {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(off) => total as i128 + off as i128,
            SeekFrom::Current(off) => self.pos as i128 + off as i128,
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

// ---------------------------------------------------------------------------
// HunkReader — Read + Seek + BufRead over a single hunk
// ---------------------------------------------------------------------------

/// In-memory reader for a single hunk. Reads the hunk eagerly on
/// construction; subsequent `Read`/`Seek`/`BufRead` calls are pure memory.
pub struct HunkReader {
    data: Vec<u8>,
    pos: usize,
}

impl HunkReader {
    pub fn new(chd: &Chd, hunknum: u32) -> Result<Self> {
        let mut data = vec![0u8; chd.hunk_bytes() as usize];
        chd.read_hunk(hunknum, &mut data)?;
        Ok(Self { data, pos: 0 })
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Read for HunkReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = &self.data[self.pos..];
        let n = remaining.len().min(buf.len());
        buf[..n].copy_from_slice(&remaining[..n]);
        self.pos += n;
        Ok(n)
    }
}

impl Seek for HunkReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let len = self.data.len() as i128;
        let new_pos: i128 = match pos {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(off) => len + off as i128,
            SeekFrom::Current(off) => self.pos as i128 + off as i128,
        };
        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.pos = new_pos as usize;
        Ok(self.pos as u64)
    }
}

impl BufRead for HunkReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Ok(&self.data[self.pos.min(self.data.len())..])
    }
    fn consume(&mut self, amt: usize) {
        self.pos = (self.pos + amt).min(self.data.len());
    }
}

// ---------------------------------------------------------------------------
// Tag / cdrom constants
// ---------------------------------------------------------------------------

/// Well-known metadata tags. Values match MAME's `chd.h`.
pub mod metadata {
    /// Returns the four ASCII characters for a tag, useful for debug prints.
    pub fn tag_to_chars(tag: u32) -> [char; 4] {
        [
            ((tag >> 24) & 0xff) as u8 as char,
            ((tag >> 16) & 0xff) as u8 as char,
            ((tag >> 8) & 0xff) as u8 as char,
            (tag & 0xff) as u8 as char,
        ]
    }

    pub mod tags {
        use crate::make_tag;

        pub const HARD_DISK_METADATA_TAG: u32 = make_tag(b'G', b'D', b'D', b'D');
        pub const HARD_DISK_IDENT_METADATA_TAG: u32 = make_tag(b'I', b'D', b'N', b'T');
        pub const HARD_DISK_KEY_METADATA_TAG: u32 = make_tag(b'K', b'E', b'Y', b' ');
        pub const PCMCIA_CIS_METADATA_TAG: u32 = make_tag(b'C', b'I', b'S', b' ');

        pub const CDROM_OLD_METADATA_TAG: u32 = make_tag(b'C', b'H', b'C', b'D');
        pub const CDROM_TRACK_METADATA_TAG: u32 = make_tag(b'C', b'H', b'T', b'R');
        pub const CDROM_TRACK_METADATA2_TAG: u32 = make_tag(b'C', b'H', b'T', b'2');

        pub const GDROM_OLD_METADATA_TAG: u32 = make_tag(b'C', b'H', b'G', b'T');
        pub const GDROM_TRACK_METADATA_TAG: u32 = make_tag(b'C', b'H', b'G', b'D');

        pub const DVD_METADATA_TAG: u32 = make_tag(b'D', b'V', b'D', b' ');

        pub const AV_METADATA_TAG: u32 = make_tag(b'A', b'V', b'A', b'V');
        pub const AV_LD_METADATA_TAG: u32 = make_tag(b'A', b'V', b'L', b'D');
    }

    /// Returns true if `tag` is a CD-ROM-related tag (any version).
    pub fn is_cdrom(tag: u32) -> bool {
        tag == tags::CDROM_OLD_METADATA_TAG
            || tag == tags::CDROM_TRACK_METADATA_TAG
            || tag == tags::CDROM_TRACK_METADATA2_TAG
    }

    /// Returns true if `tag` is a GD-ROM-related tag.
    pub fn is_gdrom(tag: u32) -> bool {
        tag == tags::GDROM_OLD_METADATA_TAG || tag == tags::GDROM_TRACK_METADATA_TAG
    }

    // Re-export for convenience so callers can do `metadata::make(...)`.
    pub use crate::make_tag as make;
}

/// CD-ROM frame layout constants, mirroring the values in chd-rs's `cdrom`
/// module and MAME's `cdrom.h`.
pub mod cdrom {
    pub const CD_MAX_SECTOR_DATA: u32 = 2352;
    pub const CD_MAX_SUBCODE_DATA: u32 = 96;
    pub const CD_FRAME_SIZE: u32 = CD_MAX_SECTOR_DATA + CD_MAX_SUBCODE_DATA;

    pub const CD_SYNC_NUM_BYTES: usize = 12;
    pub const CD_SYNC_HEADER: [u8; CD_SYNC_NUM_BYTES] = [
        0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00,
    ];

    pub const CD_SYNC_OFFSET: usize = 0x000;
    pub const CD_MODE_OFFSET: usize = 0x00f;
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

impl Chd {
    /// Internal: raw FFI handle for use from this module's impls.
    fn raw(&self) -> *mut sys::ChdFile {
        self.inner
    }
}

fn chd_err_to_io(err: ChdError) -> io::Error {
    io::Error::other(format!("CHD error: {:?}", err))
}
