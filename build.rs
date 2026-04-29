fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    // LZMA
    let lzma_files = [
        "deps/mame/3rdparty/lzma/C/LzmaDec.c",
        "deps/mame/3rdparty/lzma/C/LzmaEnc.c",
        "deps/mame/3rdparty/lzma/C/Lzma2Dec.c",
        "deps/mame/3rdparty/lzma/C/Lzma2Enc.c",
        "deps/mame/3rdparty/lzma/C/7zCrc.c",
        "deps/mame/3rdparty/lzma/C/7zCrcOpt.c",
        "deps/mame/3rdparty/lzma/C/CpuArch.c",
        "deps/mame/3rdparty/lzma/C/LzFind.c",
        "deps/mame/3rdparty/lzma/C/Alloc.c",
    ];
    let mut lzma_build = cc::Build::new();
    for file in lzma_files.iter() {
        lzma_build.file(file);
    }
    lzma_build.include("deps/mame/3rdparty/lzma/C");
    lzma_build.define("Z7_ST", None);
    lzma_build.compile("lzma_internal");

    // Zlib
    let zlib_files = [
        "deps/mame/3rdparty/zlib/adler32.c",
        "deps/mame/3rdparty/zlib/compress.c",
        "deps/mame/3rdparty/zlib/crc32.c",
        "deps/mame/3rdparty/zlib/deflate.c",
        "deps/mame/3rdparty/zlib/inffast.c",
        "deps/mame/3rdparty/zlib/inflate.c",
        "deps/mame/3rdparty/zlib/inftrees.c",
        "deps/mame/3rdparty/zlib/trees.c",
        "deps/mame/3rdparty/zlib/uncompr.c",
        "deps/mame/3rdparty/zlib/zutil.c",
    ];
    let mut zlib_build = cc::Build::new();
    for file in zlib_files.iter() {
        zlib_build.file(file);
    }
    zlib_build.include("deps/mame/3rdparty/zlib");
    zlib_build.compile("zlib_internal");

    // utf8proc
    let mut utf8proc_build = cc::Build::new();
    utf8proc_build.file("deps/mame/3rdparty/utf8proc/utf8proc.c");
    utf8proc_build.include("deps/mame/3rdparty/utf8proc");
    if target_os == "windows" {
        // Suppress __declspec(dllimport) on declarations so the static-library
        // build doesn't hit MSVC C2491 when utf8proc.c defines the same symbols.
        utf8proc_build.define("UTF8PROC_STATIC", None);
    }
    utf8proc_build.compile("utf8proc_internal");

    // zstd
    let zstd_files = [
        "deps/mame/3rdparty/zstd/lib/common/debug.c",
        "deps/mame/3rdparty/zstd/lib/common/xxhash.c",
        "deps/mame/3rdparty/zstd/lib/common/pool.c",
        "deps/mame/3rdparty/zstd/lib/common/entropy_common.c",
        "deps/mame/3rdparty/zstd/lib/common/error_private.c",
        "deps/mame/3rdparty/zstd/lib/common/fse_decompress.c",
        "deps/mame/3rdparty/zstd/lib/common/threading.c",
        "deps/mame/3rdparty/zstd/lib/common/zstd_common.c",
        "deps/mame/3rdparty/zstd/lib/decompress/zstd_decompress_block.c",
        "deps/mame/3rdparty/zstd/lib/decompress/zstd_decompress.c",
        "deps/mame/3rdparty/zstd/lib/decompress/zstd_ddict.c",
        "deps/mame/3rdparty/zstd/lib/decompress/huf_decompress.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_lazy.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_compress_superblock.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstdmt_compress.c",
        "deps/mame/3rdparty/zstd/lib/compress/fse_compress.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_compress_literals.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_compress_sequences.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_fast.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_compress.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_opt.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_double_fast.c",
        "deps/mame/3rdparty/zstd/lib/compress/zstd_ldm.c",
        "deps/mame/3rdparty/zstd/lib/compress/hist.c",
        "deps/mame/3rdparty/zstd/lib/compress/huf_compress.c",
    ];
    let mut zstd_build = cc::Build::new();
    for file in zstd_files.iter() {
        zstd_build.file(file);
    }
    zstd_build.include("deps/mame/3rdparty/zstd/lib");
    zstd_build.include("deps/mame/3rdparty/zstd/lib/common");
    zstd_build.define("ZSTD_DISABLE_ASM", None);
    zstd_build.compile("zstd_internal");

    // flac
    let mut flac_files = vec![
        "deps/mame/3rdparty/flac/src/libFLAC/bitmath.c",
        "deps/mame/3rdparty/flac/src/libFLAC/bitreader.c",
        "deps/mame/3rdparty/flac/src/libFLAC/bitwriter.c",
        "deps/mame/3rdparty/flac/src/libFLAC/cpu.c",
        "deps/mame/3rdparty/flac/src/libFLAC/crc.c",
        "deps/mame/3rdparty/flac/src/libFLAC/fixed.c",
        "deps/mame/3rdparty/flac/src/libFLAC/float.c",
        "deps/mame/3rdparty/flac/src/libFLAC/format.c",
        "deps/mame/3rdparty/flac/src/libFLAC/lpc.c",
        "deps/mame/3rdparty/flac/src/libFLAC/md5.c",
        "deps/mame/3rdparty/flac/src/libFLAC/memory.c",
        "deps/mame/3rdparty/flac/src/libFLAC/metadata_iterators.c",
        "deps/mame/3rdparty/flac/src/libFLAC/metadata_object.c",
        "deps/mame/3rdparty/flac/src/libFLAC/stream_decoder.c",
        "deps/mame/3rdparty/flac/src/libFLAC/stream_encoder.c",
        "deps/mame/3rdparty/flac/src/libFLAC/stream_encoder_framing.c",
        "deps/mame/3rdparty/flac/src/libFLAC/window.c",
    ];

    let mut flac_build = cc::Build::new();
    if target_arch == "x86_64" {
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/fixed_intrin_sse2.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/fixed_intrin_ssse3.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/fixed_intrin_sse42.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/fixed_intrin_avx2.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/lpc_intrin_sse2.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/lpc_intrin_sse41.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/lpc_intrin_avx2.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/lpc_intrin_fma.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/stream_encoder_intrin_sse2.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/stream_encoder_intrin_ssse3.c");
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/stream_encoder_intrin_avx2.c");
        flac_build.flag("-msse2");
        flac_build.flag("-msse4.1");
        flac_build.flag("-msse4.2");
        flac_build.flag("-mavx2");
        flac_build.flag("-mfma");
    } else if target_arch == "aarch64" {
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/lpc_intrin_neon.c");
    }

    for file in flac_files.iter() {
        flac_build.file(file);
    }
    flac_build.include("deps/mame/3rdparty/flac/include");
    flac_build.include("deps/mame/3rdparty/flac/src/libFLAC/include");
    flac_build.define("HAVE_CONFIG_H", None);
    flac_build.define("FLAC__HAS_OGG", Some("0"));
    flac_build.define("HAVE_LROUND", Some("1"));
    flac_build.define("HAVE_INTTYPES_H", Some("1"));
    flac_build.define("HAVE_STDBOOL_H", Some("1"));
    flac_build.define("HAVE_STDINT_H", Some("1"));
    flac_build.define("HAVE_STDIO_H", Some("1"));
    flac_build.define("HAVE_STDLIB_H", Some("1"));
    flac_build.define("HAVE_STRING_H", Some("1"));
    flac_build.define("SIZE_T_MAX", Some("UINT64_MAX"));
    if target_os == "windows" {
        // Building FLAC statically: prevent headers from marking APIs as dllimport.
        flac_build.define("FLAC__NO_DLL", None);
        // FLAC's compat layer rewrites fopen to fopen_utf8 on Windows; supply it.
        flac_build.file("deps/mame/3rdparty/flac/src/share/win_utf8_io/win_utf8_io.c");
    }
    flac_build.compile("flac_internal");

    let mut build = cc::Build::new();

    build.cpp(true);
    build.std("c++20");

    // Include paths
    build.include("sys");
    build.include("deps/mame/src/lib/util");
    build.include("deps/mame/src/osd");
    build.include("deps/mame/3rdparty/zlib");
    build.include("deps/mame/3rdparty/flac/include");
    build.include("deps/mame/3rdparty/lzma/C");
    build.include("deps/mame/3rdparty");
    build.include("deps/mame/3rdparty/zstd/lib");
    build.include("deps/mame/3rdparty/utf8proc");

    // MAME Core Source Files
    let cpp_files = [
        "deps/mame/src/lib/util/chd.cpp",
        "deps/mame/src/lib/util/chdcodec.cpp",
        "deps/mame/src/lib/util/hashing.cpp",
        "deps/mame/src/lib/util/ioprocs.cpp",
        "deps/mame/src/lib/util/md5.cpp",
        "deps/mame/src/lib/util/flac.cpp",
        "deps/mame/src/lib/util/huffman.cpp",
        "deps/mame/src/lib/util/avhuff.cpp",
        "deps/mame/src/lib/util/cdrom.cpp",
        "deps/mame/src/lib/util/strformat.cpp",
        "deps/mame/src/lib/util/corestr.cpp",
        "deps/mame/src/lib/util/unicode.cpp",
        "deps/mame/src/lib/util/bitmap.cpp",
        "deps/mame/src/lib/util/palette.cpp",
        "deps/mame/src/lib/util/corefile.cpp",
        "deps/mame/src/lib/util/vecstream.cpp",
        "deps/mame/src/lib/util/path.cpp",
        "sys/minimal_osd.cpp",
        "sys/chd_shim.cpp",
    ];

    for file in cpp_files.iter() {
        build.file(file);
    }

    if target_os == "windows" {
        // Win32 string conversion used by unicode.cpp on Windows.
        build.file("deps/mame/src/osd/strconv.cpp");
    }

    // Macros — mirrors MAME's scripts/genie.lua and scripts/src/osd/windows_cfg.lua
    #[cfg(target_endian = "little")]
    build.define("LSB_FIRST", None);

    // CRLF: 1=CR, 2=LF, 3=CRLF. MAME uses 3 on Windows, 2 elsewhere.
    if target_os == "windows" {
        build.define("CRLF", Some("3"));
    } else {
        build.define("CRLF", Some("2"));
    }

    if target_os == "linux" {
        build.define("SDLMAME_UNIX", None);
        build.define("SDLMAME_LINUX", None);
    } else if target_os == "macos" {
        build.define("SDLMAME_UNIX", None);
        build.define("SDLMAME_MACOSX", None);
        build.define("SDLMAME_DARWIN", None);
    } else if target_os == "windows" {
        // From scripts/src/osd/windows_cfg.lua
        build.define("OSD_WINDOWS", None);
        build.define("UNICODE", None);
        build.define("_UNICODE", None);
        build.define("WIN32_LEAN_AND_MEAN", None);
        build.define("NOMINMAX", None);
        build.define("_WIN32_WINNT", Some("0x0602"));
        // MAME's genie.lua pairs _WIN32_WINNT=0x0602 with NTDDI_VERSION=0x06000000,
        // which MinGW tolerates but the MSVC SDK rejects with #error. Use the
        // matching Win8 NTDDI value so both compilers accept it.
        build.define("NTDDI_VERSION", Some("0x06020000"));
        // From scripts/genie.lua: 3rdparty static linkage and MSVC CRT deprecation silencing
        build.define("FLAC__NO_DLL", None);
        build.define("UTF8PROC_STATIC", None); // not in MAME; needed because we build utf8proc as a separate static lib via cc
        build.define("_CRT_NONSTDC_NO_DEPRECATE", None);
        build.define("_CRT_SECURE_NO_DEPRECATE", None);
        build.define("_CRT_STDIO_LEGACY_WIDE_SPECIFIERS", None);
    }

    build.compile("chd_shim");

    println!("cargo:rerun-if-changed=sys/chd_shim.h");
    println!("cargo:rerun-if-changed=sys/chd_shim.cpp");
    println!("cargo:rerun-if-changed=sys/minimal_osd.cpp");
    println!("cargo:rerun-if-changed=build.rs");
}
