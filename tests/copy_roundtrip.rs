use std::io::Cursor;

use libchdman_rs::copy::{self, CopyOptions};
use libchdman_rs::enhancements::metadata::tags::{
    HARD_DISK_IDENT_METADATA_TAG, HARD_DISK_METADATA_TAG,
};
use libchdman_rs::hd::{self, HdCreateOptions};
use libchdman_rs::{
    cd, dvd, Chd, CHD_CODEC_LZMA, CHD_CODEC_NONE, CHD_CODEC_ZLIB, CHD_CODEC_ZSTD,
};

mod common;

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn synth(len: usize, seed: u64) -> Vec<u8> {
    let mut s = seed;
    (0..len)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (s >> 33) as u8
        })
        .collect()
}

#[test]
fn copy_hd_changes_codec_preserves_payload_and_metadata() {
    // Create a ZLIB-compressed HD CHD with an IDENT blob.
    let dir = tmpdir();
    let src = dir.path().join("src.chd");
    let payload = synth(128 * 1024, 0xCAFEBABE);
    let ident = b"\x00\x01ORIG IDENT BLOB".to_vec();
    hd::create_from_reader(
        Cursor::new(payload.clone()),
        &src,
        HdCreateOptions {
            logical_size: payload.len() as u64,
            codecs: [CHD_CODEC_ZLIB, 0, 0, 0],
            ident: Some(ident.clone()),
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let src_info = Chd::open(src.to_str().unwrap(), false, None)
        .unwrap()
        .info()
        .unwrap();

    // Copy to LZMA.
    let dst = dir.path().join("dst.chd");
    copy::copy(
        &src,
        &dst,
        CopyOptions {
            codecs: [CHD_CODEC_LZMA, 0, 0, 0],
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let dst_chd = Chd::open(dst.to_str().unwrap(), false, None).unwrap();
    let dst_info = dst_chd.info().unwrap();

    // Logical bytes, unit bytes, raw SHA1 must match. Codecs differ.
    assert_eq!(dst_info.logical_bytes, src_info.logical_bytes);
    assert_eq!(dst_info.unit_bytes, src_info.unit_bytes);
    assert_eq!(dst_info.raw_sha1, src_info.raw_sha1, "raw payload SHA1");
    assert_eq!(dst_info.codecs, [CHD_CODEC_LZMA, 0, 0, 0]);
    assert!(dst_info.is_hd);

    // Metadata records preserved verbatim (GDDD + IDNT).
    let gddd = dst_chd.read_metadata(HARD_DISK_METADATA_TAG, 0).unwrap();
    assert!(std::str::from_utf8(&gddd).unwrap().contains("CYLS:"));
    let ident_back = dst_chd.read_metadata(HARD_DISK_IDENT_METADATA_TAG, 0).unwrap();
    assert_eq!(ident_back, ident);

    // Re-extract and byte-compare.
    let mut extracted = Vec::new();
    hd::extract_to_writer(&dst, &mut extracted, &mut |_| {}).unwrap();
    assert_eq!(extracted, payload);
}

#[test]
fn copy_hd_changes_hunk_size() {
    let dir = tmpdir();
    let src = dir.path().join("src.chd");
    let payload = synth(64 * 1024, 0xDEADBEEF);
    hd::create_from_reader(
        Cursor::new(payload.clone()),
        &src,
        HdCreateOptions {
            logical_size: payload.len() as u64,
            hunk_size: 4096,
            codecs: [CHD_CODEC_NONE; 4],
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let dst = dir.path().join("dst.chd");
    copy::copy(
        &src,
        &dst,
        CopyOptions {
            hunk_size: Some(8192),
            codecs: [CHD_CODEC_ZSTD, 0, 0, 0],
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let dst_chd = Chd::open(dst.to_str().unwrap(), false, None).unwrap();
    assert_eq!(dst_chd.hunk_bytes(), 8192);
    assert_eq!(dst_chd.unit_bytes(), 512);

    let mut extracted = Vec::new();
    hd::extract_to_writer(&dst, &mut extracted, &mut |_| {}).unwrap();
    assert_eq!(extracted, payload);
}

#[test]
fn copy_dvd_uncompressed_to_compressed() {
    let _ = common::fixtures_dir();
    let iso_in = common::fixture_path(common::assets::DVD_ISO);
    let original = std::fs::read(&iso_in).unwrap();

    let dir = tmpdir();
    let src = dir.path().join("src.chd");
    dvd::create_from_iso(
        &iso_in,
        &src,
        dvd::DvdCreateOptions {
            codecs: [CHD_CODEC_NONE; 4],
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let dst = dir.path().join("dst.chd");
    copy::copy(
        &src,
        &dst,
        CopyOptions {
            codecs: [CHD_CODEC_LZMA, CHD_CODEC_ZLIB, 0, 0],
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let dst_chd = Chd::open(dst.to_str().unwrap(), false, None).unwrap();
    let info = dst_chd.info().unwrap();
    assert!(info.is_dvd, "DVD tag must be cloned during copy");
    assert_eq!(info.codecs, [CHD_CODEC_LZMA, CHD_CODEC_ZLIB, 0, 0]);

    // Round-trip extract.
    let mut out = Vec::new();
    dvd::extract_to_writer(&dst, &mut out, &mut |_| {}).unwrap();
    assert_eq!(out, original);
}

#[test]
fn copy_cd_preserves_track_metadata() {
    let _ = common::fixtures_dir();
    let cue = common::fixture_path(common::assets::CD_CUE);

    let dir = tmpdir();
    let src = dir.path().join("src.chd");
    cd::create_from_cue(
        &cue,
        &src,
        cd::CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();
    let src_chd = Chd::open(src.to_str().unwrap(), false, None).unwrap();
    let src_tracks = cd::list_tracks(&src_chd).unwrap();

    let dst = dir.path().join("dst.chd");
    copy::copy(
        &src,
        &dst,
        CopyOptions {
            // Recompress to a different CD codec set.
            codecs: [
                libchdman_rs::CHD_CODEC_CD_FLAC,
                libchdman_rs::CHD_CODEC_CD_ZLIB,
                0,
                0,
            ],
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let dst_chd = Chd::open(dst.to_str().unwrap(), false, None).unwrap();
    let dst_info = dst_chd.info().unwrap();
    assert!(dst_info.is_cd, "CD-ness preserved through copy");
    assert_eq!(dst_info.track_count, 2);

    let dst_tracks = cd::list_tracks(&dst_chd).unwrap();
    assert_eq!(dst_tracks.len(), src_tracks.len());
    for (a, b) in src_tracks.iter().zip(dst_tracks.iter()) {
        assert_eq!(a.track_num, b.track_num);
        assert_eq!(a.track_type, b.track_type);
        assert_eq!(a.frames, b.frames);
    }
}
