pub mod cd;
pub mod codec;
pub mod dvd;
pub mod enhancements;
pub mod hd;
pub(crate) mod streaming;
pub mod sys;

pub use codec::{
    codec_exists, codec_name, parse_codec_spec, CHD_CODEC_AVHUFF, CHD_CODEC_CD_FLAC,
    CHD_CODEC_CD_LZMA, CHD_CODEC_CD_ZLIB, CHD_CODEC_CD_ZSTD, CHD_CODEC_FLAC, CHD_CODEC_HUFF,
    CHD_CODEC_LZMA, CHD_CODEC_NONE, CHD_CODEC_ZLIB, CHD_CODEC_ZSTD,
};
pub use enhancements::{
    cdrom, metadata, ChdReader, HunkIter, HunkReader, MetadataEntry, MetadataIter, Version,
};

use std::ffi::CString;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::raw::c_void;
use std::ptr;

pub use sys::ChdError;

pub type Result<T> = std::result::Result<T, ChdError>;

pub struct Chd {
    pub(crate) inner: *mut sys::ChdFile,
    owned: bool,
}

impl Default for Chd {
    fn default() -> Self {
        Self::new()
    }
}

impl Chd {
    pub fn new() -> Self {
        unsafe {
            Self {
                inner: sys::chd_shim_alloc(),
                owned: true,
            }
        }
    }

    pub fn open(filename: &str, writeable: bool, parent: Option<&Chd>) -> Result<Self> {
        let chd = Self::new();
        let c_filename = CString::new(filename).map_err(|_| ChdError::InvalidFile)?;
        let parent_ptr = parent.map_or(ptr::null_mut(), |p| p.inner);

        let err = unsafe {
            sys::chd_shim_open_file(
                chd.inner,
                c_filename.as_ptr(),
                if writeable { 1 } else { 0 },
                parent_ptr,
            )
        };

        if err == ChdError::NoError {
            Ok(chd)
        } else {
            Err(err)
        }
    }

    pub fn create(
        filename: &str,
        logicalbytes: u64,
        hunkbytes: u32,
        unitbytes: u32,
        compression: [u32; 4],
    ) -> Result<Self> {
        let chd = Self::new();
        let c_filename = CString::new(filename).map_err(|_| ChdError::InvalidFile)?;

        let err = unsafe {
            sys::chd_shim_create_file(
                chd.inner,
                c_filename.as_ptr(),
                logicalbytes,
                hunkbytes,
                unitbytes,
                compression.as_ptr(),
            )
        };

        if err == ChdError::NoError {
            Ok(chd)
        } else {
            Err(err)
        }
    }

    pub fn open_custom<T: ChdIo>(io: T, writeable: bool, parent: Option<&Chd>) -> Result<Self> {
        let chd = Self::new();
        let parent_ptr = parent.map_or(ptr::null_mut(), |p| p.inner);

        let io_box = Box::new(io);
        let handle = Box::into_raw(io_box) as sys::ChdRustIoHandle;

        let ops = sys::ChdRustIoOps {
            read: Some(chd_io_read::<T>),
            write: Some(chd_io_write::<T>),
            length: Some(chd_io_length::<T>),
            close: Some(chd_io_close::<T>),
        };

        let err = unsafe {
            sys::chd_shim_open_custom(
                chd.inner,
                handle,
                ops,
                if writeable { 1 } else { 0 },
                parent_ptr,
            )
        };

        if err == ChdError::NoError {
            Ok(chd)
        } else {
            Err(err)
        }
    }

    pub fn version(&self) -> u32 {
        unsafe { sys::chd_shim_version(self.inner) }
    }

    pub fn hunk_bytes(&self) -> u32 {
        unsafe { sys::chd_shim_hunk_bytes(self.inner) }
    }

    pub fn hunk_count(&self) -> u32 {
        unsafe { sys::chd_shim_hunk_count(self.inner) }
    }

    pub fn unit_bytes(&self) -> u32 {
        unsafe { sys::chd_shim_unit_bytes(self.inner) }
    }

    pub fn unit_count(&self) -> u64 {
        unsafe { sys::chd_shim_unit_count(self.inner) }
    }

    pub fn logical_bytes(&self) -> u64 {
        unsafe { sys::chd_shim_logical_bytes(self.inner) }
    }

    pub fn sha1(&self) -> [u8; 20] {
        let mut res = [0u8; 20];
        unsafe {
            sys::chd_shim_get_sha1(self.inner, res.as_mut_ptr());
        }
        res
    }

    pub fn raw_sha1(&self) -> [u8; 20] {
        let mut res = [0u8; 20];
        unsafe {
            sys::chd_shim_get_raw_sha1(self.inner, res.as_mut_ptr());
        }
        res
    }

    pub fn parent_sha1(&self) -> [u8; 20] {
        let mut res = [0u8; 20];
        unsafe {
            sys::chd_shim_get_parent_sha1(self.inner, res.as_mut_ptr());
        }
        res
    }

    pub fn hunk_info(&self, hunknum: u32) -> Result<HunkInfo> {
        let mut compressor = 0u32;
        let mut compbytes = 0u32;
        let err = unsafe {
            sys::chd_shim_hunk_info(self.inner, hunknum, &mut compressor, &mut compbytes)
        };
        if err == ChdError::NoError {
            Ok(HunkInfo {
                compressor,
                compbytes,
            })
        } else {
            Err(err)
        }
    }

    pub fn read_hunk(&self, hunknum: u32, buffer: &mut [u8]) -> Result<()> {
        if buffer.len() < self.hunk_bytes() as usize {
            return Err(ChdError::InvalidData);
        }
        let err =
            unsafe { sys::chd_shim_read_hunk(self.inner, hunknum, buffer.as_mut_ptr() as *mut _) };
        if err == ChdError::NoError {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn write_hunk(&mut self, hunknum: u32, buffer: &[u8]) -> Result<()> {
        if buffer.len() < self.hunk_bytes() as usize {
            return Err(ChdError::InvalidData);
        }
        let err =
            unsafe { sys::chd_shim_write_hunk(self.inner, hunknum, buffer.as_ptr() as *const _) };
        if err == ChdError::NoError {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn read_bytes(&self, offset: u64, buffer: &mut [u8]) -> Result<()> {
        let err = unsafe {
            sys::chd_shim_read_bytes(
                self.inner,
                offset,
                buffer.as_mut_ptr() as *mut _,
                buffer.len() as u32,
            )
        };
        if err == ChdError::NoError {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn write_bytes(&mut self, offset: u64, buffer: &[u8]) -> Result<()> {
        let err = unsafe {
            sys::chd_shim_write_bytes(
                self.inner,
                offset,
                buffer.as_ptr() as *const _,
                buffer.len() as u32,
            )
        };
        if err == ChdError::NoError {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn read_metadata(&self, tag: u32, index: u32) -> Result<Vec<u8>> {
        let mut res_len = 0u32;
        let err = unsafe {
            sys::chd_shim_read_metadata(self.inner, tag, index, ptr::null_mut(), 0, &mut res_len)
        };
        if err != ChdError::NoError {
            return Err(err);
        }

        let mut buffer = vec![0u8; res_len as usize];
        let err = unsafe {
            sys::chd_shim_read_metadata(
                self.inner,
                tag,
                index,
                buffer.as_mut_ptr() as *mut _,
                res_len,
                &mut res_len,
            )
        };
        if err == ChdError::NoError {
            Ok(buffer)
        } else {
            Err(err)
        }
    }

    pub fn write_metadata(&mut self, tag: u32, index: u32, data: &[u8], flags: u8) -> Result<()> {
        let err = unsafe {
            sys::chd_shim_write_metadata(
                self.inner,
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

    pub fn delete_metadata(&mut self, tag: u32, index: u32) -> Result<()> {
        let err = unsafe { sys::chd_shim_delete_metadata(self.inner, tag, index) };
        if err == ChdError::NoError {
            Ok(())
        } else {
            Err(err)
        }
    }

    /// Aggregate header + introspection snapshot. One FFI-walking call
    /// returns everything chdman's `info` subcommand prints, suitable for
    /// rendering in a UI without further round-trips.
    ///
    /// `track_count` counts CHT2/CHTR/CHGD metadata entries; for non-CD/GD
    /// CHDs it is zero. `metadata_tags` lists every metadata tag in the
    /// order MAME stores them, paired with its index within that tag.
    pub fn info(&self) -> Result<ChdInfo> {
        let codecs = [0i32, 1, 2, 3]
            .map(|i| unsafe { sys::chd_shim_compression(self.inner, i) });

        // Walk metadata once. Counts CD/GD track tags and collects every
        // (tag, index) pair for downstream consumers.
        let mut metadata_tags: Vec<(u32, u32)> = Vec::new();
        let mut per_tag_index: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut track_count: u32 = 0;
        let mut idx: u32 = 0;
        loop {
            let mut tag: u32 = 0;
            let mut flags: u8 = 0;
            let mut size: u32 = 0;
            let err = unsafe {
                sys::chd_shim_metadata_enum(
                    self.inner,
                    idx,
                    &mut tag,
                    &mut flags,
                    ptr::null_mut(),
                    0,
                    &mut size,
                )
            };
            match err {
                ChdError::NoError => {
                    let n = per_tag_index.entry(tag).or_insert(0);
                    metadata_tags.push((tag, *n));
                    *n += 1;
                    if matches!(
                        tag,
                        // CHT2 / CHTR / CHGD — values match metadata::tags::*.
                        0x43485432 | 0x43485452 | 0x43484744
                    ) {
                        track_count += 1;
                    }
                    idx += 1;
                }
                ChdError::MetadataNotFound => break,
                other => return Err(other),
            }
        }

        let is_dvd = unsafe { sys::chd_shim_check_is_dvd(self.inner) != 0 };
        let is_hd = unsafe { sys::chd_shim_check_is_hd(self.inner) != 0 };
        let is_cd = unsafe { sys::chd_shim_check_is_cd(self.inner) != 0 };
        let is_gd = unsafe { sys::chd_shim_check_is_gd(self.inner) != 0 };
        let is_av = unsafe { sys::chd_shim_check_is_av(self.inner) != 0 };
        let compressed = unsafe { sys::chd_shim_compressed(self.inner) != 0 };
        let has_parent = unsafe { sys::chd_shim_has_parent(self.inner) != 0 };

        Ok(ChdInfo {
            version: self.version(),
            hunk_bytes: self.hunk_bytes(),
            unit_bytes: self.unit_bytes(),
            hunk_count: self.hunk_count(),
            logical_bytes: self.logical_bytes(),
            codecs,
            sha1: self.sha1(),
            raw_sha1: self.raw_sha1(),
            parent_sha1: self.parent_sha1(),
            metadata_tags,
            track_count,
            compressed,
            has_parent,
            is_hd,
            is_cd,
            is_gd,
            is_dvd,
            is_av,
        })
    }

    pub fn verify(&self) -> Result<()> {
        let mut rawsha = Sha1Creator::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        let total = self.logical_bytes();
        for offset in (0..total).step_by(buffer.len()) {
            let to_read = std::cmp::min(buffer.len() as u64, total - offset) as u32;
            self.read_bytes(offset, &mut buffer[..to_read as usize])?;
            rawsha.append(&buffer[..to_read as usize]);
        }
        let computed = rawsha.finish();
        let expected = if self.version() <= 3 {
            self.sha1()
        } else {
            self.raw_sha1()
        };
        if computed != expected {
            return Err(ChdError::DecompressionError);
        }
        Ok(())
    }
}

pub struct HunkInfo {
    pub compressor: u32,
    pub compbytes: u32,
}

/// Aggregate snapshot returned by [`Chd::info`]. Mirrors the data chdman's
/// `info` subcommand prints, in a single FFI walk.
#[derive(Debug, Clone)]
pub struct ChdInfo {
    pub version: u32,
    pub hunk_bytes: u32,
    pub unit_bytes: u32,
    pub hunk_count: u32,
    pub logical_bytes: u64,
    /// Per-slot codec FourCCs (slot 0..=3). Zero means "no codec".
    pub codecs: [u32; 4],
    pub sha1: [u8; 20],
    pub raw_sha1: [u8; 20],
    pub parent_sha1: [u8; 20],
    /// Every metadata entry in MAME's stored order, paired with its
    /// per-tag index (so `(CHT2, 0)`, `(CHT2, 1)`, … are distinguishable).
    pub metadata_tags: Vec<(u32, u32)>,
    /// Count of CD/GD track metadata entries (CHT2 + CHTR + CHGD).
    pub track_count: u32,
    pub compressed: bool,
    pub has_parent: bool,
    pub is_hd: bool,
    pub is_cd: bool,
    pub is_gd: bool,
    pub is_dvd: bool,
    pub is_av: bool,
}

impl Drop for Chd {
    fn drop(&mut self) {
        if self.owned {
            unsafe {
                sys::chd_shim_free(self.inner);
            }
        }
    }
}

pub trait ChdIo: Read + Write + Seek {
    fn length(&mut self) -> std::io::Result<u64>;
}

impl<T: Read + Write + Seek> ChdIo for T {
    fn length(&mut self) -> std::io::Result<u64> {
        let cur = self.stream_position()?;
        let len = self.seek(SeekFrom::End(0))?;
        self.seek(SeekFrom::Start(cur))?;
        Ok(len)
    }
}

extern "C" fn chd_io_read<T: ChdIo>(
    handle: sys::ChdRustIoHandle,
    offset: u64,
    buffer: *mut c_void,
    length: u32,
    actual: *mut u32,
) -> libc::c_int {
    let io = unsafe { &mut *(handle as *mut T) };
    if io.seek(SeekFrom::Start(offset)).is_err() {
        return -1;
    }
    let buf = unsafe { std::slice::from_raw_parts_mut(buffer as *mut u8, length as usize) };
    match io.read(buf) {
        Ok(n) => {
            unsafe {
                *actual = n as u32;
            }
            0
        }
        Err(_) => -1,
    }
}

extern "C" fn chd_io_write<T: ChdIo>(
    handle: sys::ChdRustIoHandle,
    offset: u64,
    buffer: *const c_void,
    length: u32,
    actual: *mut u32,
) -> libc::c_int {
    let io = unsafe { &mut *(handle as *mut T) };
    if io.seek(SeekFrom::Start(offset)).is_err() {
        return -1;
    }
    let buf = unsafe { std::slice::from_raw_parts(buffer as *const u8, length as usize) };
    match io.write(buf) {
        Ok(n) => {
            unsafe {
                *actual = n as u32;
            }
            0
        }
        Err(_) => -1,
    }
}

extern "C" fn chd_io_length<T: ChdIo>(
    handle: sys::ChdRustIoHandle,
    result: *mut u64,
) -> libc::c_int {
    let io = unsafe { &mut *(handle as *mut T) };
    match io.length() {
        Ok(len) => {
            unsafe {
                *result = len;
            }
            0
        }
        Err(_) => -1,
    }
}

extern "C" fn chd_io_close<T: ChdIo>(handle: sys::ChdRustIoHandle) {
    let _ = unsafe { Box::from_raw(handle as *mut T) };
}

pub trait ChdDataHandler {
    fn read_data(&mut self, dest: &mut [u8], offset: u64) -> u32;
}

pub struct ChdCompressor {
    pub(crate) inner: *mut sys::ChdFileCompressor,
    pub(crate) logical_bytes: u64,
}

impl ChdCompressor {
    pub fn new<T: ChdDataHandler>(handler: T) -> Self {
        let handler_box = Box::new(handler);
        let handle = Box::into_raw(handler_box) as sys::ChdRustCompressorHandle;
        let ops = sys::ChdRustCompressorOps {
            read_data: Some(chd_compressor_read_data::<T>),
        };
        unsafe {
            Self {
                inner: sys::chd_shim_compressor_alloc(handle, ops),
                logical_bytes: 0,
            }
        }
    }

    pub fn create_file(
        &mut self,
        filename: &str,
        logicalbytes: u64,
        hunkbytes: u32,
        unitbytes: u32,
        compression: [u32; 4],
    ) -> Result<()> {
        let c_filename = CString::new(filename).map_err(|_| ChdError::InvalidFile)?;
        let err = unsafe {
            sys::chd_shim_compressor_create_file(
                self.inner,
                c_filename.as_ptr(),
                logicalbytes,
                hunkbytes,
                unitbytes,
                compression.as_ptr(),
            )
        };
        if err == ChdError::NoError {
            self.logical_bytes = logicalbytes;
            Ok(())
        } else {
            Err(err)
        }
    }

    /// Internal: the underlying compressor pointer reinterpreted as a
    /// plain `chd_file_t*`. `chd_file_compressor` inherits from
    /// `chd_file`, so calling chd_file shims (e.g. `write_metadata`)
    /// against this pointer is valid; the C++ vtable dispatches
    /// correctly.
    #[doc(hidden)]
    pub fn as_chd_file_ptr(&mut self) -> *mut sys::ChdFile {
        self.inner as *mut sys::ChdFile
    }

    pub fn compress_begin(&mut self) {
        unsafe {
            sys::chd_shim_compressor_begin(self.inner);
        }
    }

    /// Drives one chunk of compression and reports state.
    ///
    /// Returns `CompressStep::Done` when MAME signals completion, or
    /// `CompressStep::Continue` while work remains. Any other error is
    /// surfaced via `Err`.
    pub fn compress_continue(&mut self) -> Result<CompressStep> {
        let mut progress_frac = 0.0f64;
        let mut ratio = 0.0f64;
        let err = unsafe {
            sys::chd_shim_compressor_continue(self.inner, &mut progress_frac, &mut ratio)
        };
        let total = self.logical_bytes;
        let done = (progress_frac.clamp(0.0, 1.0) * total as f64) as u64;
        let prog = CompressionProgress {
            bytes_done: done.min(total),
            bytes_total: total,
            ratio,
        };
        match err {
            ChdError::NoError => Ok(CompressStep::Done(prog)),
            ChdError::WalkingParent | ChdError::Compressing => Ok(CompressStep::Continue(prog)),
            other => Err(other),
        }
    }
}

/// Byte-accurate progress reported during compression.
///
/// `bytes_total` is the logical size requested at `create_file` time;
/// `bytes_done` is derived from MAME's normalized progress fraction.
/// `ratio` is the running compressed/logical ratio (0.0..=1.0+).
#[derive(Debug, Clone, Copy)]
pub struct CompressionProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub ratio: f64,
}

/// One step of a compression loop. `Continue` means the caller should call
/// `compress_continue` again; `Done` means the output is fully written.
#[derive(Debug, Clone, Copy)]
pub enum CompressStep {
    Continue(CompressionProgress),
    Done(CompressionProgress),
}

extern "C" fn chd_compressor_read_data<T: ChdDataHandler>(
    handle: sys::ChdRustCompressorHandle,
    dest: *mut c_void,
    offset: u64,
    length: u32,
) -> u32 {
    let handler = unsafe { &mut *(handle as *mut T) };
    let buf = unsafe { std::slice::from_raw_parts_mut(dest as *mut u8, length as usize) };
    handler.read_data(buf, offset)
}

impl Drop for ChdCompressor {
    fn drop(&mut self) {
        unsafe {
            sys::chd_shim_compressor_free(self.inner);
        }
    }
}

struct Sha1Creator {
    inner: *mut sys::ChdSha1,
}
impl Sha1Creator {
    fn new() -> Self {
        unsafe {
            Self {
                inner: sys::chd_shim_sha1_alloc(),
            }
        }
    }
    fn append(&mut self, data: &[u8]) {
        unsafe {
            sys::chd_shim_sha1_append(self.inner, data.as_ptr() as *const _, data.len() as u32);
        }
    }
    fn finish(&self) -> [u8; 20] {
        let mut res = [0u8; 20];
        unsafe {
            sys::chd_shim_sha1_finish(self.inner, res.as_mut_ptr());
        }
        res
    }
}
impl Drop for Sha1Creator {
    fn drop(&mut self) {
        unsafe {
            sys::chd_shim_sha1_free(self.inner);
        }
    }
}

pub const fn make_tag(a: u8, b: u8, c: u8, d: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | (d as u32)
}
