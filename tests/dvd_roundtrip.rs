use std::sync::atomic::{AtomicU32, Ordering};

use libchdman_rs::dvd::{self, DvdCreateOptions, DVD_SECTOR_SIZE};
use libchdman_rs::{Chd, ChdError, CHD_CODEC_LZMA, CHD_CODEC_NONE, CHD_CODEC_ZLIB, CHD_CODEC_ZSTD};

mod common;

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn create_and_extract_real_dvd_iso() {
    let _ = common::fixtures_dir();
    let iso_in = common::fixture_path(common::assets::DVD_ISO);
    let original = std::fs::read(&iso_in).unwrap();
    assert_eq!(
        original.len() as u32 % DVD_SECTOR_SIZE,
        0,
        "fixture must be DVD-aligned"
    );

    let dir = tmpdir();
    let chd_path = dir.path().join("dvd.chd");
    let iso_out = dir.path().join("out.iso");

    dvd::create_from_iso(
        &iso_in,
        &chd_path,
        DvdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let info = chd.info().unwrap();
    assert!(info.is_dvd, "produced CHD must report as DVD");
    assert!(!info.is_cd);
    assert!(!info.is_hd);
    assert_eq!(info.logical_bytes, original.len() as u64);

    let mut last_progress = 0u64;
    dvd::extract_to_iso(&chd_path, &iso_out, &mut |b| last_progress = b).unwrap();
    let extracted = std::fs::read(&iso_out).unwrap();
    assert_eq!(extracted, original, "extracted DVD ISO must round-trip");
    assert_eq!(last_progress, original.len() as u64);
}

#[test]
fn rejects_non_2048_aligned_size() {
    let dir = tmpdir();
    let chd_path = dir.path().join("dvd.chd");
    let opts = DvdCreateOptions {
        logical_size: 4097, // not a multiple of 2048
        ..Default::default()
    };
    let res = dvd::create_from_reader(
        std::io::Cursor::new(vec![0u8; 4097]),
        &chd_path,
        opts,
        &mut |_| {},
        &|| false,
    );
    assert!(matches!(res, Err(ChdError::InvalidData)));
    assert!(!chd_path.exists());
}

#[test]
fn extract_rejects_non_dvd_chd() {
    // Build an HD CHD then try to extract it as a DVD — must be refused.
    let _ = common::fixtures_dir();
    let dir = tmpdir();
    let hd_path = dir.path().join("hd.chd");

    libchdman_rs::hd::create_from_reader(
        std::io::Cursor::new(vec![0xAAu8; 64 * 1024]),
        &hd_path,
        libchdman_rs::hd::HdCreateOptions {
            logical_size: 64 * 1024,
            codecs: [CHD_CODEC_NONE; 4],
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let iso_out = dir.path().join("out.iso");
    let res = dvd::extract_to_iso(&hd_path, &iso_out, &mut |_| {});
    assert!(matches!(res, Err(ChdError::UnsupportedFormat)));
}

#[test]
fn codec_matrix() {
    let _ = common::fixtures_dir();
    let iso_in = common::fixture_path(common::assets::DVD_ISO);
    let original = std::fs::read(&iso_in).unwrap();

    for codecs in [
        [CHD_CODEC_LZMA, 0, 0, 0],
        [CHD_CODEC_ZSTD, 0, 0, 0],
        [CHD_CODEC_ZLIB, 0, 0, 0],
        [CHD_CODEC_NONE; 4],
    ] {
        let dir = tmpdir();
        let chd_path = dir.path().join("dvd.chd");
        let opts = DvdCreateOptions {
            codecs,
            ..Default::default()
        };
        dvd::create_from_iso(&iso_in, &chd_path, opts, &mut |_| {}, &|| false)
            .unwrap_or_else(|e| panic!("codecs {:?} failed: {:?}", codecs, e));

        let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
        let info = chd.info().unwrap();
        assert!(info.is_dvd, "codecs {:?}", codecs);
        assert_eq!(info.codecs, codecs);

        let mut out = Vec::new();
        dvd::extract_to_writer(&chd_path, &mut out, &mut |_| {}).unwrap();
        assert_eq!(out, original, "codecs {:?} round-trip mismatch", codecs);
    }
}

#[test]
fn cancellation_deletes_partial_dvd_chd() {
    let _ = common::fixtures_dir();
    let iso_in = common::fixture_path(common::assets::DVD_ISO);

    let dir = tmpdir();
    let chd_path = dir.path().join("cancelled.chd");
    let polls = AtomicU32::new(0);

    let res = dvd::create_from_iso(
        &iso_in,
        &chd_path,
        DvdCreateOptions::default(),
        &mut |_| {},
        &|| polls.fetch_add(1, Ordering::SeqCst) == 0,
    );
    assert!(matches!(res, Err(ChdError::Cancelled)));
    assert!(!chd_path.exists());
}
