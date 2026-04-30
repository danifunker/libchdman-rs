use std::sync::atomic::{AtomicU32, Ordering};

use libchdman_rs::cd::{self, CdCreateOptions, SubcodeType, TrackType};
use libchdman_rs::{
    Chd, ChdError, CHD_CODEC_CD_FLAC, CHD_CODEC_CD_LZMA, CHD_CODEC_CD_ZLIB, CHD_CODEC_CD_ZSTD,
    CHD_CODEC_NONE,
};

mod common;

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn create_from_real_cue_two_tracks() {
    // Force fixture extraction so the CUE's relative BIN paths resolve.
    let _ = common::fixtures_dir();
    let cue = common::fixture_path(common::assets::CD_CUE);

    let dir = tmpdir();
    let chd_path = dir.path().join("cd.chd");

    cd::create_from_cue(
        &cue,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .expect("create_from_cue");

    // Verify the produced CHD looks like a CD.
    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let info = chd.info().unwrap();
    assert!(info.is_cd, "produced CHD should report as a CD");
    assert_eq!(info.track_count, 2, "fixture CUE has 2 tracks");

    // list_tracks returns both, with the right types.
    let tracks = cd::list_tracks(&chd).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].track_num, 1);
    assert_eq!(tracks[0].track_type, TrackType::Mode1Raw); // MODE1/2352
    assert_eq!(tracks[0].subcode_type, SubcodeType::None);
    assert_eq!(tracks[1].track_num, 2);
    assert_eq!(tracks[1].track_type, TrackType::Audio);
    // Frame counts: data.bin = 101136 / 2352 = 43, audio.bin = 529200 / 2352 = 225.
    assert_eq!(tracks[0].frames, 43);
    assert_eq!(tracks[1].frames, 225);
}

#[test]
fn create_from_iso() {
    let _ = common::fixtures_dir();
    let iso = common::fixture_path(common::assets::CD_ISO);

    let dir = tmpdir();
    let chd_path = dir.path().join("from_iso.chd");

    cd::create_from_iso(
        &iso,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .expect("create_from_iso");

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let info = chd.info().unwrap();
    assert!(info.is_cd);
    assert_eq!(info.track_count, 1);

    let tracks = cd::list_tracks(&chd).unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].track_type, TrackType::Mode1);
    // ISO is 921600 bytes / 2048 = 450 sectors.
    assert_eq!(tracks[0].frames, 450);
}

#[test]
fn create_from_cue_codec_matrix() {
    let _ = common::fixtures_dir();
    let cue = common::fixture_path(common::assets::CD_CUE);

    // chdman's documented combos for CDs. CDFL pairs with CDZL because
    // FLAC only handles audio sectors; the second slot picks up the
    // data track. Uncompressed [NONE] proves the metadata path works
    // without any codec in the picture.
    let combos: &[[u32; 4]] = &[
        [CHD_CODEC_CD_LZMA, CHD_CODEC_CD_ZLIB, 0, 0],
        [CHD_CODEC_CD_FLAC, CHD_CODEC_CD_ZLIB, 0, 0],
        [CHD_CODEC_CD_ZSTD, CHD_CODEC_CD_ZLIB, 0, 0],
        [CHD_CODEC_NONE; 4],
    ];

    for codecs in combos {
        let dir = tmpdir();
        let chd_path = dir.path().join("cd.chd");
        let opts = CdCreateOptions {
            codecs: *codecs,
            ..Default::default()
        };
        cd::create_from_cue(&cue, &chd_path, opts, &mut |_| {}, &|| false)
            .unwrap_or_else(|e| panic!("codecs {:?} failed: {:?}", codecs, e));

        // Each combo must produce a CD with the same TOC.
        let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
        let info = chd.info().unwrap();
        assert!(info.is_cd, "codecs {:?}: not a CD", codecs);
        assert_eq!(info.track_count, 2, "codecs {:?}: track count", codecs);
        assert_eq!(info.codecs, *codecs, "codecs {:?}: stored codecs", codecs);

        let tracks = cd::list_tracks(&chd).unwrap();
        assert_eq!(tracks[0].track_type, TrackType::Mode1Raw);
        assert_eq!(tracks[0].frames, 43);
        assert_eq!(tracks[1].track_type, TrackType::Audio);
        assert_eq!(tracks[1].subcode_type, SubcodeType::None);
        assert_eq!(tracks[1].frames, 225);
    }
}

#[test]
fn extract_to_iso_roundtrips_data_only_chd() {
    let _ = common::fixtures_dir();
    let iso_in = common::fixture_path(common::assets::CD_ISO);
    let original = std::fs::read(&iso_in).unwrap();

    let dir = tmpdir();
    let chd_path = dir.path().join("from_iso.chd");
    let iso_out = dir.path().join("out.iso");

    cd::create_from_iso(
        &iso_in,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let mut last_progress = 0u64;
    cd::extract_to_iso(&chd_path, &iso_out, &mut |b| last_progress = b).unwrap();
    let extracted = std::fs::read(&iso_out).unwrap();
    assert_eq!(extracted, original, "extracted ISO bytes mismatch original");
    assert_eq!(last_progress, original.len() as u64);
}

#[test]
fn extract_to_iso_rejects_multi_track() {
    let _ = common::fixtures_dir();
    let cue = common::fixture_path(common::assets::CD_CUE);
    let dir = tmpdir();
    let chd_path = dir.path().join("multi.chd");
    cd::create_from_cue(
        &cue,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();
    let iso_out = dir.path().join("out.iso");
    let res = cd::extract_to_iso(&chd_path, &iso_out, &mut |_| {});
    assert!(matches!(res, Err(ChdError::UnsupportedFormat)));
}

#[test]
fn extract_to_cue_roundtrips_two_track_chd() {
    let _ = common::fixtures_dir();
    let cue_in = common::fixture_path(common::assets::CD_CUE);
    let data_orig = std::fs::read(common::fixture_path(common::assets::CD_DATA_BIN)).unwrap();
    let audio_orig = std::fs::read(common::fixture_path(common::assets::CD_AUDIO_BIN)).unwrap();

    let dir = tmpdir();
    let chd_path = dir.path().join("cd.chd");
    cd::create_from_cue(
        &cue_in,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let cue_out = dir.path().join("out.cue");
    let bin_out = dir.path().join("out.bin");
    cd::extract_to_cue(&chd_path, &cue_out, &bin_out, &mut |_| {}).unwrap();

    // Combined .bin = data_track + audio_track. Both stored at 2352
    // bytes/sector in our fixture, so the bytes line up exactly.
    let combined = std::fs::read(&bin_out).unwrap();
    let mut expected = Vec::with_capacity(data_orig.len() + audio_orig.len());
    expected.extend_from_slice(&data_orig);
    expected.extend_from_slice(&audio_orig);
    assert_eq!(combined.len(), expected.len(), "bin size");
    assert_eq!(combined, expected, "extracted .bin must match original BIN concatenation");

    // CUE shape: one FILE entry, two TRACK entries with INDEX 01 at
    // their offsets, TRACK 02 starts at MSF for 43 frames.
    let cue_text = std::fs::read_to_string(&cue_out).unwrap();
    assert!(cue_text.starts_with("FILE \"out.bin\" BINARY\n"), "cue: {cue_text}");
    assert!(cue_text.contains("TRACK 01 MODE1/2352"));
    assert!(cue_text.contains("TRACK 02 AUDIO"));
    assert!(cue_text.contains("INDEX 01 00:00:00"));
    // 43 frames = 0:00.43 in MSF.
    assert!(cue_text.contains("INDEX 01 00:00:43"), "cue: {cue_text}");
}

#[test]
fn cancellation_deletes_partial_cd_chd() {
    let _ = common::fixtures_dir();
    let cue = common::fixture_path(common::assets::CD_CUE);

    let dir = tmpdir();
    let chd_path = dir.path().join("cancelled.chd");
    let polls = AtomicU32::new(0);

    // The fixture is small so compression finishes quickly. Tripping
    // cancel on the first poll guarantees we hit the cancelled-then-
    // unlinked branch in run_compression.
    let result = cd::create_from_cue(
        &cue,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| polls.fetch_add(1, Ordering::SeqCst) == 0,
    );

    assert!(matches!(result, Err(ChdError::Cancelled)));
    assert!(
        !chd_path.exists(),
        "partial CHD was not removed after cancel"
    );
}
