//! CHD codec FourCC table and `-c` compression-spec parser.
//!
//! Mirrors `chdcodec.h`'s constant set and `chdman.cpp`'s
//! `parse_compression` semantics. Values come from MAME via FFI; nothing
//! in this module reimplements codec logic — it just names and validates
//! codec identifiers so callers can plumb a UI string into a `[u32; 4]`
//! slot array.

use std::ffi::CStr;

use crate::sys;
use crate::{make_tag, ChdError, Result};

/// No codec (raw, uncompressed).
pub const CHD_CODEC_NONE: u32 = 0;
/// Generic deflate (`zlib`).
pub const CHD_CODEC_ZLIB: u32 = make_tag(b'z', b'l', b'i', b'b');
/// Zstandard (`zstd`).
pub const CHD_CODEC_ZSTD: u32 = make_tag(b'z', b's', b't', b'd');
/// LZMA (`lzma`).
pub const CHD_CODEC_LZMA: u32 = make_tag(b'l', b'z', b'm', b'a');
/// Static-Huffman (`huff`). MAME spells this `CHD_CODEC_HUFFMAN`.
pub const CHD_CODEC_HUFF: u32 = make_tag(b'h', b'u', b'f', b'f');
/// FLAC (`flac`).
pub const CHD_CODEC_FLAC: u32 = make_tag(b'f', b'l', b'a', b'c');

/// CD-aware deflate (`cdzl`).
pub const CHD_CODEC_CD_ZLIB: u32 = make_tag(b'c', b'd', b'z', b'l');
/// CD-aware Zstandard (`cdzs`).
pub const CHD_CODEC_CD_ZSTD: u32 = make_tag(b'c', b'd', b'z', b's');
/// CD-aware LZMA (`cdlz`).
pub const CHD_CODEC_CD_LZMA: u32 = make_tag(b'c', b'd', b'l', b'z');
/// CD-aware FLAC (`cdfl`).
pub const CHD_CODEC_CD_FLAC: u32 = make_tag(b'c', b'd', b'f', b'l');

/// AV (laserdisc) Huffman codec (`avhu`).
pub const CHD_CODEC_AVHUFF: u32 = make_tag(b'a', b'v', b'h', b'u');

/// True if MAME knows about `codec`. Wraps `chd_codec_list::codec_exists`.
pub fn codec_exists(codec: u32) -> bool {
    unsafe { sys::chd_shim_codec_exists(codec) != 0 }
}

/// Human-readable codec name from MAME's table, e.g. `"CD FLAC"`. Returns
/// `None` if MAME has no entry for `codec`. Wraps
/// `chd_codec_list::codec_name`.
pub fn codec_name(codec: u32) -> Option<&'static str> {
    let p = unsafe { sys::chd_shim_codec_name(codec) };
    if p.is_null() {
        return None;
    }
    // MAME returns pointers to static string literals — safe to assume
    // 'static lifetime and valid UTF-8.
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

/// Parse chdman's `-c` syntax into a 4-slot codec array.
///
/// Accepted forms (matching `chdman.cpp::parse_compression`):
/// - `"none"` → all four slots `CHD_CODEC_NONE`.
/// - 1..=4 comma-separated 4-character mnemonics (e.g. `"cdlz,cdzl,cdfl"`).
///   Trailing slots are padded with `CHD_CODEC_NONE`. Each mnemonic must
///   be exactly 4 ASCII bytes and must satisfy [`codec_exists`].
///
/// Whitespace is significant — chdman does not trim. Empty input or any
/// other shape returns [`ChdError::UnknownCompression`].
pub fn parse_codec_spec(s: &str) -> Result<[u32; 4]> {
    if s == "none" {
        return Ok([CHD_CODEC_NONE; 4]);
    }
    if s.is_empty() {
        return Err(ChdError::UnknownCompression);
    }

    let mut out = [CHD_CODEC_NONE; 4];
    let mut count = 0usize;
    for name in s.split(',') {
        if count == 4 {
            // chdman silently truncates at 4 — we reject so callers
            // notice typos like `"cdlz,cdzl,cdfl,zlib,zstd"`.
            return Err(ChdError::UnknownCompression);
        }
        let bytes = name.as_bytes();
        if bytes.len() != 4 {
            return Err(ChdError::UnknownCompression);
        }
        let tag = make_tag(bytes[0], bytes[1], bytes[2], bytes[3]);
        if !codec_exists(tag) {
            return Err(ChdError::UnknownCompression);
        }
        out[count] = tag;
        count += 1;
    }
    Ok(out)
}
