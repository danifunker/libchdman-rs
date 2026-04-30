use std::io::{Cursor, Read};
use std::sync::atomic::{AtomicU32, Ordering};

use libchdman_rs::enhancements::metadata::tags::{
    HARD_DISK_IDENT_METADATA_TAG, HARD_DISK_METADATA_TAG,
};
use libchdman_rs::hd::{
    self, compute_chs, format_gddd, read_geometry, HdCreateOptions, HdGeometry,
};
use libchdman_rs::{Chd, ChdError, CHD_CODEC_LZMA, CHD_CODEC_NONE, CHD_CODEC_ZLIB, CHD_CODEC_ZSTD};

mod common;

/// Deterministic but compressible-ish payload: byte = (i / 7) ^ (i & 0xff).
fn synth(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i / 7) ^ i) as u8).collect()
}

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn compute_chs_matches_chdman_for_clean_size() {
    // 256 KiB / 512 = 512 sectors. 63 sectors don't divide 512; chdman's
    // loop walks down: 63→...→2. 8 divides 512, so sectors=8, then
    // total_heads = 64; heads=16 fits → cylinders = 4. We just verify
    // the product matches and the geometry is non-degenerate.
    let g = compute_chs(256 * 1024, 512).unwrap();
    assert_eq!(
        g.cylinders as u64 * g.heads as u64 * g.sectors as u64 * g.sector_bytes as u64,
        256 * 1024
    );
    assert!(g.heads >= 2 && g.heads <= 16);
    assert!(g.sectors >= 2 && g.sectors <= 63);
}

#[test]
fn format_gddd_matches_chdman_format() {
    let s = format_gddd(HdGeometry {
        cylinders: 1024,
        heads: 16,
        sectors: 63,
        sector_bytes: 512,
    });
    let trimmed = std::str::from_utf8(&s).unwrap().trim_end_matches('\0');
    assert_eq!(trimmed, "CYLS:1024,HEADS:16,SECS:63,BPS:512");
}

#[test]
fn roundtrip_uncompressed() {
    let dir = tmpdir();
    let chd_path = dir.path().join("hd.chd");

    let payload = synth(64 * 1024);
    let opts = HdCreateOptions {
        logical_size: payload.len() as u64,
        codecs: [CHD_CODEC_NONE; 4],
        ..Default::default()
    };

    hd::create_from_reader(
        Cursor::new(payload.clone()),
        &chd_path,
        opts,
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    // Verify metadata
    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let g = read_geometry(&chd).unwrap();
    assert_eq!(g.sector_bytes, 512);
    assert_eq!(g.logical_bytes(), payload.len() as u64);
    let info = chd.info().unwrap();
    assert!(info.is_hd);
    assert_eq!(info.logical_bytes, payload.len() as u64);

    // Round-trip extract
    let mut out = Vec::new();
    let mut last_progress = 0u64;
    hd::extract_to_writer(&chd_path, &mut out, &mut |b| last_progress = b).unwrap();
    assert_eq!(out, payload);
    assert_eq!(last_progress, payload.len() as u64);
}

#[test]
fn roundtrip_codec_matrix() {
    for codec in [CHD_CODEC_ZLIB, CHD_CODEC_ZSTD, CHD_CODEC_LZMA] {
        let dir = tmpdir();
        let chd_path = dir.path().join("hd.chd");
        let payload = synth(128 * 1024);
        let opts = HdCreateOptions {
            logical_size: payload.len() as u64,
            codecs: [codec, 0, 0, 0],
            ..Default::default()
        };
        hd::create_from_reader(
            Cursor::new(payload.clone()),
            &chd_path,
            opts,
            &mut |_| {},
            &|| false,
        )
        .unwrap_or_else(|e| panic!("create with codec {:#x} failed: {:?}", codec, e));

        let mut out = Vec::new();
        hd::extract_to_writer(&chd_path, &mut out, &mut |_| {}).unwrap();
        assert_eq!(out, payload, "codec {:#x} mismatch", codec);
    }
}

#[test]
fn zero_pads_short_reader() {
    // Reader provides 1000 bytes but logical_size = 4096 (one hunk).
    // The extracted output should be the 1000 bytes followed by 3096 zeros.
    let dir = tmpdir();
    let chd_path = dir.path().join("hd.chd");
    let payload = synth(1000);
    let opts = HdCreateOptions {
        logical_size: 4096,
        codecs: [CHD_CODEC_NONE; 4],
        ..Default::default()
    };
    hd::create_from_reader(
        Cursor::new(payload.clone()),
        &chd_path,
        opts,
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let mut out = Vec::new();
    hd::extract_to_writer(&chd_path, &mut out, &mut |_| {}).unwrap();
    assert_eq!(out.len(), 4096);
    assert_eq!(&out[..1000], &payload[..]);
    assert!(out[1000..].iter().all(|&b| b == 0));
}

#[test]
fn writes_ident_blob() {
    let dir = tmpdir();
    let chd_path = dir.path().join("hd.chd");
    let ident = b"\x12\x34TESTIDENTBLOB".to_vec();
    let payload = synth(8192);

    let opts = HdCreateOptions {
        logical_size: payload.len() as u64,
        codecs: [CHD_CODEC_NONE; 4],
        ident: Some(ident.clone()),
        ..Default::default()
    };
    hd::create_from_reader(Cursor::new(payload), &chd_path, opts, &mut |_| {}, &|| {
        false
    })
    .unwrap();

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let read_back = chd.read_metadata(HARD_DISK_IDENT_METADATA_TAG, 0).unwrap();
    assert_eq!(read_back, ident);
    // GDDD still present.
    assert!(chd.read_metadata(HARD_DISK_METADATA_TAG, 0).is_ok());
}

#[test]
fn cancellation_deletes_partial_file() {
    let dir = tmpdir();
    let chd_path = dir.path().join("hd.chd");
    let payload = synth(1024 * 1024); // 1 MiB → many hunks.
    let calls = AtomicU32::new(0);

    let opts = HdCreateOptions {
        logical_size: payload.len() as u64,
        codecs: [CHD_CODEC_LZMA, 0, 0, 0],
        ..Default::default()
    };

    let result =
        hd::create_from_reader(Cursor::new(payload), &chd_path, opts, &mut |_| {}, &|| {
            // Trip cancel after the second progress poll.
            calls.fetch_add(1, Ordering::SeqCst) > 0
        });

    assert!(matches!(result, Err(ChdError::Cancelled)));
    assert!(
        !chd_path.exists(),
        "partial output file was not removed on cancel"
    );
}

#[test]
fn roundtrip_real_iso_fixture() {
    let iso_path = common::fixture_path(common::assets::CD_ISO);
    let mut iso_bytes = Vec::new();
    std::fs::File::open(&iso_path)
        .unwrap()
        .read_to_end(&mut iso_bytes)
        .unwrap();

    let dir = tmpdir();
    let chd_path = dir.path().join("hd_from_iso.chd");

    let opts = HdCreateOptions {
        logical_size: iso_bytes.len() as u64,
        // ISO is 2048-byte sectors but we treat it as raw bytes here.
        unit_size: 2048,
        hunk_size: 8 * 2048,
        codecs: [CHD_CODEC_LZMA, CHD_CODEC_ZLIB, 0, 0],
        ..Default::default()
    };

    hd::create_from_path(&iso_path, &chd_path, opts, &mut |_| {}, &|| false).unwrap();

    let mut out = Vec::new();
    hd::extract_to_writer(&chd_path, &mut out, &mut |_| {}).unwrap();
    assert_eq!(out, iso_bytes);
}
