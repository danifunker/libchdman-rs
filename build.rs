fn main() {
    println!("cargo:rerun-if-env-changed=LIBCHDMAN_FORCE_SOURCE");
    println!("cargo:rerun-if-env-changed=LIBCHDMAN_PREBUILT_FALLBACK");
    println!("cargo:rerun-if-env-changed=LIBCHDMAN_GLIBC");
    println!("cargo:rerun-if-env-changed=LIBCHDMAN_PREBUILT_LOCAL_ARCHIVE");

    let use_prebuilt =
        cfg!(feature = "prebuilt") && std::env::var("LIBCHDMAN_FORCE_SOURCE").is_err();

    if use_prebuilt {
        match try_use_prebuilt() {
            Ok(()) => return,
            Err(e) if std::env::var("LIBCHDMAN_PREBUILT_FALLBACK").is_ok() => {
                println!("cargo:warning=Prebuilt fetch failed ({e}); compiling from source.");
            }
            Err(e) => panic!(
                "Prebuilt archive fetch failed: {e}.\n\
                 Set LIBCHDMAN_PREBUILT_FALLBACK=1 to compile from source instead, \
                 or LIBCHDMAN_FORCE_SOURCE=1 to always compile from source."
            ),
        }
    }
    build_from_source();
}

fn build_from_source() {
    // Crates.io ships a slim tarball with only the Rust wrapper; the
    // ~900 MB MAME C++ source isn't included. If we got here from that
    // tarball, fail loudly with an actionable message instead of erroring
    // hundreds of lines deep in `cc-rs` with "file not found".
    let deps_marker = std::path::Path::new("deps/mame/src/lib/util/chd.cpp");
    if !deps_marker.exists() {
        let v = env!("CARGO_PKG_VERSION");
        let msg = format!(
            "
libchdman-rs source build requires MAME's vendored C++ source, which is NOT
shipped in the crates.io package.

Use one of:

  1. Enable the `prebuilt` feature (recommended — downloads a pre-built
     static archive matching your target triple from GitHub Releases):

         libchdman-rs = {{ version = \"{v}\", features = [\"prebuilt\"] }}

  2. Depend on this crate via its git repo for source builds (full MAME
     source tree, ~1 GB; cold builds are slow):

         libchdman-rs = {{ git = \"https://github.com/danifunker/libchdman-rs\", tag = \"v{v}\" }}

See the README's \"Choosing between crates.io and git\" section.
"
        );
        panic!("{msg}");
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    // Silences MAME / 3rdparty C/C++ diagnostics across GCC/Clang/MSVC.
    // We don't own this code; its warnings are noise we can't act on.
    // (Note: a handful of macOS-only ranlib "no symbols" warnings can still
    // appear for platform-gated sources that compile to empty objects on
    // macOS. These come from the linker stage; cc-rs does not expose a
    // ranlib-flag hook to silence them.)
    let quiet = |b: &mut cc::Build| {
        b.warnings(false).extra_warnings(false);
    };

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
    quiet(&mut lzma_build);
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
    quiet(&mut zlib_build);
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
    quiet(&mut utf8proc_build);
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
    quiet(&mut zstd_build);
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
    if target_arch == "x86_64" || target_arch == "x86" {
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
    } else if target_arch == "aarch64" && target_env != "msvc" {
        // FLAC's NEON intrinsics are GCC/Clang-only: lpc_intrin_neon.c pulls
        // in <arm_neon.h> with GCC-style code that MSVC's ARM64 compiler
        // rejects. On aarch64-pc-windows-msvc we drop the file and disable
        // the NEON path entirely via FLAC__NO_ASM below (portable C fallback).
        flac_files.push("deps/mame/3rdparty/flac/src/libFLAC/lpc_intrin_neon.c");
    }

    for file in flac_files.iter() {
        flac_build.file(file);
    }
    flac_build.include("deps/mame/3rdparty/flac/include");
    flac_build.include("deps/mame/3rdparty/flac/src/libFLAC/include");
    flac_build.define("HAVE_CONFIG_H", None);
    // Suppress FLAC's internal "clipping rice_parameter" debug prints to stderr.
    // These are gated on #ifndef NDEBUG and represent internal clamping, not errors.
    flac_build.define("NDEBUG", None);
    flac_build.define("FLAC__HAS_OGG", Some("0"));
    flac_build.define("HAVE_LROUND", Some("1"));
    flac_build.define("HAVE_INTTYPES_H", Some("1"));
    flac_build.define("HAVE_STDBOOL_H", Some("1"));
    flac_build.define("HAVE_STDINT_H", Some("1"));
    flac_build.define("HAVE_STDIO_H", Some("1"));
    flac_build.define("HAVE_STDLIB_H", Some("1"));
    flac_build.define("HAVE_STRING_H", Some("1"));
    flac_build.define("SIZE_T_MAX", Some("UINT64_MAX"));
    if target_arch == "aarch64" && target_env == "msvc" {
        // MSVC ARM64 (_M_ARM64) makes FLAC's bundled config.h enable the NEON
        // path (FLAC__HAS_NEONINTRIN), but those intrinsics don't compile
        // under cl.exe and we don't ship the .c file (see above). config.h
        // lives in the pinned upstream MAME submodule, so we can't edit it;
        // instead define FLAC__NO_ASM, which gates out both the NEON symbol
        // declarations (lpc.h) and their dispatch in stream_encoder.c. FLAC
        // then uses its portable C path — correct, just unoptimized for LPC.
        flac_build.define("FLAC__NO_ASM", None);
    }
    if target_os == "windows" {
        // Building FLAC statically: prevent headers from marking APIs as dllimport.
        flac_build.define("FLAC__NO_DLL", None);
        // FLAC's compat layer rewrites fopen to fopen_utf8 on Windows; supply it.
        flac_build.file("deps/mame/3rdparty/flac/src/share/win_utf8_io/win_utf8_io.c");
    }
    quiet(&mut flac_build);
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
        "sys/cd_shim.cpp",
    ];

    for file in cpp_files.iter() {
        build.file(file);
    }

    // strconv.cpp provides osd::text::to_wstring/from_wstring (Windows-only) and
    // osd_uchar_from_osdchar (both branches), so compile on every platform.
    build.file("deps/mame/src/osd/strconv.cpp");

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

    quiet(&mut build);
    build.compile("chd_shim");

    println!("cargo:rerun-if-changed=sys/chd_shim.h");
    println!("cargo:rerun-if-changed=sys/chd_shim.cpp");
    println!("cargo:rerun-if-changed=sys/minimal_osd.cpp");
    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(not(feature = "prebuilt"))]
fn try_use_prebuilt() -> Result<(), String> {
    unreachable!("try_use_prebuilt called without the `prebuilt` feature")
}

#[cfg(feature = "prebuilt")]
fn try_use_prebuilt() -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::PathBuf;

    let target = std::env::var("TARGET").map_err(|_| "TARGET env var missing".to_string())?;
    let out_dir =
        PathBuf::from(std::env::var("OUT_DIR").map_err(|_| "OUT_DIR env var missing".to_string())?);
    let version = env!("CARGO_PKG_VERSION");

    let is_windows_msvc = target.contains("pc-windows-msvc");
    let is_linux_gnu = target.contains("unknown-linux-gnu");
    let is_apple = target.contains("apple-darwin");

    // CI / dogfooding hook: when set, skip download/verify and link the
    // archive at this path as if it were the prebuilt asset. The release
    // workflow uses this to exercise the prebuilt link path against the
    // archive it just merged, before the asset is uploaded.
    if let Ok(local) = std::env::var("LIBCHDMAN_PREBUILT_LOCAL_ARCHIVE") {
        let src = PathBuf::from(&local);
        let src_bytes = fs::read(&src)
            .map_err(|e| format!("read LIBCHDMAN_PREBUILT_LOCAL_ARCHIVE {local}: {e}"))?;
        let dst_name = if is_windows_msvc {
            "chdman_rs.lib"
        } else {
            "libchdman_rs.a"
        };
        let dst = out_dir.join(dst_name);
        fs::write(&dst, &src_bytes).map_err(|e| format!("copy local archive: {e}"))?;
        println!("cargo:rustc-link-search=native={}", out_dir.display());
        println!("cargo:rustc-link-lib=static=chdman_rs");
        if is_apple {
            println!("cargo:rustc-link-lib=c++");
        } else if is_linux_gnu {
            println!("cargo:rustc-link-lib=stdc++");
        }
        println!("cargo:warning=libchdman-rs: linked LOCAL prebuilt archive {local}");
        return Ok(());
    }

    if !(is_windows_msvc || is_linux_gnu || is_apple) {
        return Err(format!(
            "no prebuilt archive published for target `{target}` (supported: \
             *-unknown-linux-gnu, *-apple-darwin, *-pc-windows-msvc)"
        ));
    }

    let ext = if is_windows_msvc { "lib" } else { "a" };

    let glibc_suffix = if is_linux_gnu {
        let raw = std::env::var("LIBCHDMAN_GLIBC").unwrap_or_else(|_| "auto".into());
        let chosen = match raw.as_str() {
            "auto" | "" => {
                // armv7 ships only a glibc2.31 prebuilt (MiSTer / Cortex-A9).
                // x86_64 and aarch64 default to the 2.35 floor.
                if target.contains("armv7") {
                    "2.31"
                } else {
                    "2.35"
                }
            }
            "2.31" | "2.35" | "2.39" => raw.as_str(),
            other => {
                return Err(format!(
                    "LIBCHDMAN_GLIBC={other} not recognized \
                     (expected 2.31, 2.35, 2.39, or auto). \
                     The 2.31 floor is available for armv7 only; \
                     x86_64/aarch64 minimum is 2.35."
                ));
            }
        };
        format!("-glibc{}", chosen)
    } else {
        String::new()
    };

    let asset = format!("libchdman_rs-{target}{glibc_suffix}.{ext}");
    let base_url =
        format!("https://github.com/danifunker/libchdman-rs/releases/download/v{version}/{asset}");
    let sha_url = format!("{base_url}.sha256");

    let cache_dir = cache_dir().join(version);
    fs::create_dir_all(&cache_dir).map_err(|e| format!("create cache dir: {e}"))?;
    let cached_archive = cache_dir.join(&asset);
    let cached_sha = cache_dir.join(format!("{asset}.sha256"));

    let expected_sha = match fs::read_to_string(&cached_sha) {
        Ok(s) => parse_sha256_line(&s, &asset),
        Err(_) => None,
    };

    let archive_bytes =
        if let (Some(expected), Ok(bytes)) = (expected_sha.as_deref(), fs::read(&cached_archive)) {
            if hex::encode(Sha256::digest(&bytes)) == expected {
                println!("cargo:warning=libchdman-rs: using cached prebuilt {asset}");
                bytes
            } else {
                download_with_sha(&base_url, &sha_url, &asset, &cached_archive, &cached_sha)?
            }
        } else {
            download_with_sha(&base_url, &sha_url, &asset, &cached_archive, &cached_sha)?
        };

    let dst_name = if is_windows_msvc {
        "chdman_rs.lib".to_string()
    } else {
        "libchdman_rs.a".to_string()
    };
    let dst = out_dir.join(&dst_name);
    fs::write(&dst, &archive_bytes).map_err(|e| format!("write archive to OUT_DIR: {e}"))?;

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=chdman_rs");
    if is_apple {
        println!("cargo:rustc-link-lib=c++");
    } else if is_linux_gnu {
        println!("cargo:rustc-link-lib=stdc++");
    }
    println!("cargo:warning=libchdman-rs: linked prebuilt archive {asset}");
    Ok(())
}

#[cfg(feature = "prebuilt")]
fn parse_sha256_line(content: &str, asset: &str) -> Option<String> {
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let hex_part = parts.next()?;
        let name_part = parts.next().unwrap_or("");
        let name_matches = name_part.is_empty()
            || name_part == asset
            || name_part.trim_start_matches('*') == asset;
        if name_matches && hex_part.len() == 64 && hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hex_part.to_ascii_lowercase());
        }
    }
    None
}

#[cfg(feature = "prebuilt")]
fn download_with_sha(
    archive_url: &str,
    sha_url: &str,
    asset: &str,
    cached_archive: &std::path::Path,
    cached_sha: &std::path::Path,
) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha256};
    use std::fs;

    let sha_text = http_get_text(sha_url)?;
    let expected = parse_sha256_line(&sha_text, asset)
        .ok_or_else(|| format!("could not parse sha256 line from {sha_url}"))?;

    let bytes = http_get_bytes(archive_url)?;
    let got = hex::encode(Sha256::digest(&bytes));
    if got != expected {
        return Err(format!(
            "sha256 mismatch for {asset}: expected {expected}, got {got}"
        ));
    }

    fs::write(cached_archive, &bytes).map_err(|e| format!("cache archive: {e}"))?;
    fs::write(cached_sha, sha_text.as_bytes()).map_err(|e| format!("cache sha: {e}"))?;
    Ok(bytes)
}

#[cfg(feature = "prebuilt")]
fn http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use std::time::Duration;
    let mut last_err = String::new();
    for attempt in 0..3u32 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(1u64 << (attempt - 1)));
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .into();
        match agent.get(url).call() {
            Ok(resp) => {
                let mut buf = Vec::new();
                if let Err(e) = resp.into_body().into_reader().read_to_end(&mut buf) {
                    last_err = format!("read body: {e}");
                    continue;
                }
                return Ok(buf);
            }
            Err(e) => last_err = format!("request: {e}"),
        }
    }
    Err(format!("GET {url} failed after 3 attempts: {last_err}"))
}

#[cfg(feature = "prebuilt")]
fn http_get_text(url: &str) -> Result<String, String> {
    let bytes = http_get_bytes(url)?;
    String::from_utf8(bytes).map_err(|e| format!("non-utf8 response from {url}: {e}"))
}

#[cfg(feature = "prebuilt")]
fn cache_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("libchdman-rs");
        }
    }
    if cfg!(target_os = "windows") {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("libchdman-rs");
        }
    }
    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Caches/libchdman-rs");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache/libchdman-rs");
    }
    std::env::temp_dir().join("libchdman-rs")
}
