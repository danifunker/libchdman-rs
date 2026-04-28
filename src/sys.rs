use libc::{c_char, c_int, c_void};

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ChdError {
    NoError = 0,
    NoInterface,
    NotOpen,
    AlreadyOpen,
    InvalidFile,
    InvalidData,
    RequiresParent,
    FileNotWriteable,
    CodecError,
    InvalidParent,
    HunkOutOfRange,
    DecompressionError,
    CompressionError,
    CantVerify,
    MetadataNotFound,
    InvalidMetadataSize,
    UnsupportedVersion,
    VerifyIncomplete,
    InvalidMetadata,
    InvalidState,
    OperationPending,
    UnsupportedFormat,
    UnknownCompression,
    WalkingParent,
    Compressing,
}

pub enum ChdFile {}

pub type ChdRustIoHandle = *mut c_void;

#[repr(C)]
pub struct ChdRustIoOps {
    pub read: Option<extern "C" fn(handle: ChdRustIoHandle, offset: u64, buffer: *mut c_void, length: u32, actual: *mut u32) -> c_int>,
    pub write: Option<extern "C" fn(handle: ChdRustIoHandle, offset: u64, buffer: *const c_void, length: u32, actual: *mut u32) -> c_int>,
    pub length: Option<extern "C" fn(handle: ChdRustIoHandle, result: *mut u64) -> c_int>,
    pub close: Option<extern "C" fn(handle: ChdRustIoHandle)>,
}

extern "C" {
    pub fn chd_shim_alloc() -> *mut ChdFile;
    pub fn chd_shim_free(chd: *mut ChdFile);

    pub fn chd_shim_open_file(chd: *mut ChdFile, filename: *const c_char, writeable: c_int, parent: *mut ChdFile) -> ChdError;
    pub fn chd_shim_open_custom(chd: *mut ChdFile, handle: ChdRustIoHandle, ops: ChdRustIoOps, writeable: c_int, parent: *mut ChdFile) -> ChdError;

    pub fn chd_shim_create_file(chd: *mut ChdFile, filename: *const c_char, logicalbytes: u64, hunkbytes: u32, unitbytes: u32, compression: *const u32) -> ChdError;

    pub fn chd_shim_close(chd: *mut ChdFile);

    pub fn chd_shim_version(chd: *mut ChdFile) -> u32;
    pub fn chd_shim_hunk_bytes(chd: *mut ChdFile) -> u32;
    pub fn chd_shim_hunk_count(chd: *mut ChdFile) -> u32;
    pub fn chd_shim_unit_bytes(chd: *mut ChdFile) -> u32;
    pub fn chd_shim_unit_count(chd: *mut ChdFile) -> u64;
    pub fn chd_shim_logical_bytes(chd: *mut ChdFile) -> u64;

    pub fn chd_shim_read_hunk(chd: *mut ChdFile, hunknum: u32, buffer: *mut c_void) -> ChdError;
    pub fn chd_shim_write_hunk(chd: *mut ChdFile, hunknum: u32, buffer: *const c_void) -> ChdError;

    pub fn chd_shim_read_bytes(chd: *mut ChdFile, offset: u64, buffer: *mut c_void, bytes: u32) -> ChdError;
    pub fn chd_shim_write_bytes(chd: *mut ChdFile, offset: u64, buffer: *const c_void, bytes: u32) -> ChdError;

    pub fn chd_shim_read_metadata(chd: *mut ChdFile, tag: u32, index: u32, buffer: *mut c_void, buffer_len: u32, result_len: *mut u32) -> ChdError;
    pub fn chd_shim_write_metadata(chd: *mut ChdFile, tag: u32, index: u32, buffer: *const c_void, length: u32, flags: u8) -> ChdError;
}
