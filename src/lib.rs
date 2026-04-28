pub mod sys;

use std::ffi::{CString, CStr};
use std::io::{Read, Write, Seek, SeekFrom};
use std::ptr;
use std::os::raw::c_void;

pub use sys::ChdError;

pub type Result<T> = std::result::Result<T, ChdError>;

pub struct Chd {
    inner: *mut sys::ChdFile,
    owned: bool,
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
        let mut chd = Self::new();
        let c_filename = CString::new(filename).map_err(|_| ChdError::InvalidFile)?;
        let parent_ptr = parent.map_or(ptr::null_mut(), |p| p.inner);

        let err = unsafe {
            sys::chd_shim_open_file(chd.inner, c_filename.as_ptr(), if writeable { 1 } else { 0 }, parent_ptr)
        };

        if err == ChdError::NoError {
            Ok(chd)
        } else {
            Err(err)
        }
    }

    pub fn create(filename: &str, logicalbytes: u64, hunkbytes: u32, unitbytes: u32, compression: [u32; 4]) -> Result<Self> {
        let mut chd = Self::new();
        let c_filename = CString::new(filename).map_err(|_| ChdError::InvalidFile)?;

        let err = unsafe {
            sys::chd_shim_create_file(chd.inner, c_filename.as_ptr(), logicalbytes, hunkbytes, unitbytes, compression.as_ptr())
        };

        if err == ChdError::NoError {
            Ok(chd)
        } else {
            Err(err)
        }
    }

    pub fn open_custom<T: ChdIo>(io: T, writeable: bool, parent: Option<&Chd>) -> Result<Self> {
        let mut chd = Self::new();
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
            sys::chd_shim_open_custom(chd.inner, handle, ops, if writeable { 1 } else { 0 }, parent_ptr)
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

    pub fn read_hunk(&self, hunknum: u32, buffer: &mut [u8]) -> Result<()> {
        if buffer.len() < self.hunk_bytes() as usize {
            return Err(ChdError::InvalidData);
        }
        let err = unsafe { sys::chd_shim_read_hunk(self.inner, hunknum, buffer.as_mut_ptr() as *mut _) };
        if err == ChdError::NoError { Ok(()) } else { Err(err) }
    }

    pub fn write_hunk(&mut self, hunknum: u32, buffer: &[u8]) -> Result<()> {
        if buffer.len() < self.hunk_bytes() as usize {
            return Err(ChdError::InvalidData);
        }
        let err = unsafe { sys::chd_shim_write_hunk(self.inner, hunknum, buffer.as_ptr() as *const _) };
        if err == ChdError::NoError { Ok(()) } else { Err(err) }
    }

    pub fn read_bytes(&self, offset: u64, buffer: &mut [u8]) -> Result<()> {
        let err = unsafe { sys::chd_shim_read_bytes(self.inner, offset, buffer.as_mut_ptr() as *mut _, buffer.len() as u32) };
        if err == ChdError::NoError { Ok(()) } else { Err(err) }
    }

    pub fn write_bytes(&mut self, offset: u64, buffer: &[u8]) -> Result<()> {
        let err = unsafe { sys::chd_shim_write_bytes(self.inner, offset, buffer.as_ptr() as *const _, buffer.len() as u32) };
        if err == ChdError::NoError { Ok(()) } else { Err(err) }
    }

    pub fn read_metadata(&self, tag: u32, index: u32) -> Result<Vec<u8>> {
        let mut res_len = 0u32;
        let err = unsafe { sys::chd_shim_read_metadata(self.inner, tag, index, ptr::null_mut(), 0, &mut res_len) };
        if err != ChdError::NoError { return Err(err); }

        let mut buffer = vec![0u8; res_len as usize];
        let err = unsafe { sys::chd_shim_read_metadata(self.inner, tag, index, buffer.as_mut_ptr() as *mut _, res_len, &mut res_len) };
        if err == ChdError::NoError { Ok(buffer) } else { Err(err) }
    }

    pub fn write_metadata(&mut self, tag: u32, index: u32, data: &[u8], flags: u8) -> Result<()> {
        let err = unsafe { sys::chd_shim_write_metadata(self.inner, tag, index, data.as_ptr() as *const _, data.len() as u32, flags) };
        if err == ChdError::NoError { Ok(()) } else { Err(err) }
    }
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

extern "C" fn chd_io_read<T: ChdIo>(handle: sys::ChdRustIoHandle, offset: u64, buffer: *mut c_void, length: u32, actual: *mut u32) -> libc::c_int {
    let io = unsafe { &mut *(handle as *mut T) };
    if io.seek(SeekFrom::Start(offset)).is_err() { return -1; }
    let buf = unsafe { std::slice::from_raw_parts_mut(buffer as *mut u8, length as usize) };
    match io.read(buf) {
        Ok(n) => {
            unsafe { *actual = n as u32; }
            0
        }
        Err(_) => -1,
    }
}

extern "C" fn chd_io_write<T: ChdIo>(handle: sys::ChdRustIoHandle, offset: u64, buffer: *const c_void, length: u32, actual: *mut u32) -> libc::c_int {
    let io = unsafe { &mut *(handle as *mut T) };
    if io.seek(SeekFrom::Start(offset)).is_err() { return -1; }
    let buf = unsafe { std::slice::from_raw_parts(buffer as *const u8, length as usize) };
    match io.write(buf) {
        Ok(n) => {
            unsafe { *actual = n as u32; }
            0
        }
        Err(_) => -1,
    }
}

extern "C" fn chd_io_length<T: ChdIo>(handle: sys::ChdRustIoHandle, result: *mut u64) -> libc::c_int {
    let io = unsafe { &mut *(handle as *mut T) };
    match io.length() {
        Ok(len) => {
            unsafe { *result = len; }
            0
        }
        Err(_) => -1,
    }
}

extern "C" fn chd_io_close<T: ChdIo>(handle: sys::ChdRustIoHandle) {
    let _ = unsafe { Box::from_raw(handle as *mut T) };
}

pub fn make_tag(a: u8, b: u8, c: u8, d: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | (d as u32)
}
