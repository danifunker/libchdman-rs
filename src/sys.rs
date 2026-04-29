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
pub enum ChdFileCompressor {}

pub type ChdRustIoHandle = *mut c_void;

#[repr(C)]
pub struct ChdRustIoOps {
    pub read: Option<extern "C" fn(handle: ChdRustIoHandle, offset: u64, buffer: *mut c_void, length: u32, actual: *mut u32) -> c_int>,
    pub write: Option<extern "C" fn(handle: ChdRustIoHandle, offset: u64, buffer: *const c_void, length: u32, actual: *mut u32) -> c_int>,
    pub length: Option<extern "C" fn(handle: ChdRustIoHandle, result: *mut u64) -> c_int>,
    pub close: Option<extern "C" fn(handle: ChdRustIoHandle)>,
}

pub type ChdRustCompressorHandle = *mut c_void;

#[repr(C)]
pub struct ChdRustCompressorOps {
    pub read_data: Option<extern "C" fn(handle: ChdRustCompressorHandle, dest: *mut c_void, offset: u64, length: u32) -> u32>,
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
    pub fn chd_shim_get_sha1(chd: *mut ChdFile, sha1: *mut u8);
    pub fn chd_shim_get_raw_sha1(chd: *mut ChdFile, sha1: *mut u8);
    pub fn chd_shim_get_parent_sha1(chd: *mut ChdFile, sha1: *mut u8);
    pub fn chd_shim_hunk_info(chd: *mut ChdFile, hunknum: u32, compressor: *mut u32, compbytes: *mut u32) -> ChdError;

    pub fn chd_shim_read_hunk(chd: *mut ChdFile, hunknum: u32, buffer: *mut c_void) -> ChdError;
    pub fn chd_shim_write_hunk(chd: *mut ChdFile, hunknum: u32, buffer: *const c_void) -> ChdError;

    pub fn chd_shim_read_bytes(chd: *mut ChdFile, offset: u64, buffer: *mut c_void, bytes: u32) -> ChdError;
    pub fn chd_shim_write_bytes(chd: *mut ChdFile, offset: u64, buffer: *const c_void, bytes: u32) -> ChdError;

    pub fn chd_shim_read_metadata(chd: *mut ChdFile, tag: u32, index: u32, buffer: *mut c_void, buffer_len: u32, result_len: *mut u32) -> ChdError;
    pub fn chd_shim_write_metadata(chd: *mut ChdFile, tag: u32, index: u32, buffer: *const c_void, length: u32, flags: u8) -> ChdError;
    pub fn chd_shim_delete_metadata(chd: *mut ChdFile, tag: u32, index: u32) -> ChdError;

    pub fn chd_shim_compressor_alloc(handle: ChdRustCompressorHandle, ops: ChdRustCompressorOps) -> *mut ChdFileCompressor;
    pub fn chd_shim_compressor_free(compressor: *mut ChdFileCompressor);
    pub fn chd_shim_compressor_create_file(compressor: *mut ChdFileCompressor, filename: *const c_char, logicalbytes: u64, hunkbytes: u32, unitbytes: u32, compression: *const u32) -> ChdError;
    pub fn chd_shim_compressor_begin(compressor: *mut ChdFileCompressor);
    pub fn chd_shim_compressor_continue(compressor: *mut ChdFileCompressor, progress: *mut f64, ratio: *mut f64) -> ChdError;

    pub fn chd_shim_codec_exists(codec_type: u32) -> c_int;
    pub fn chd_shim_codec_name(codec_type: u32) -> *const c_char;

}

pub enum ChdSha1 {}

extern "C" {
    pub fn chd_shim_sha1_alloc() -> *mut ChdSha1;
    pub fn chd_shim_sha1_free(s: *mut ChdSha1);
    pub fn chd_shim_sha1_append(s: *mut ChdSha1, data: *const c_void, length: u32);
    pub fn chd_shim_sha1_finish(s: *mut ChdSha1, sha1: *mut u8);
}
