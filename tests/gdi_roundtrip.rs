//! GD-ROM (.gdi) input + output parity.
//!
//! Input: MAME's `parse_toc` dispatches `.gdi` to `parse_gdi`, so
//! `create_from_cue` accepts a GDI directly. Output: `extract_to_gdi`
//! mirrors chdman's `extractcd` MODE_GDI (a `.gdi` index plus per-track
//! split `.bin`/`.raw` files).

use std::io::Write;

use libchdman_rs::cd::{self, CdCreateOptions, TrackType};
use libchdman_rs::Chd;

mod common;

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// Copy the 2-track CD fixture's BIN files into `dir` and write a `.gdi`
/// referencing them. Track 1 is MODE1_RAW data (43 frames), track 2 is
/// audio (225 frames) — LBAs contiguous so no padframes are introduced.
/// Returns the path to the written `.gdi`.
fn write_gdi_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let _ = common::fixtures_dir();
    let data_src = common::fixture_path(common::assets::CD_DATA_BIN);
    let audio_src = common::fixture_path(common::assets::CD_AUDIO_BIN);
    std::fs::copy(&data_src, dir.join("data.bin")).unwrap();
    std::fs::copy(&audio_src, dir.join("audio.bin")).unwrap();

    let data_frames = std::fs::metadata(dir.join("data.bin")).unwrap().len() / 2352;
    assert_eq!(data_frames, 43, "fixture data.bin should be 43 frames");

    let gdi_path = dir.join("disc.gdi");
    let mut f = std::fs::File::create(&gdi_path).unwrap();
    writeln!(f, "2").unwrap();
    // track lba type sectorsize filename offset
    writeln!(f, "1 0 4 2352 data.bin 0").unwrap();
    writeln!(f, "2 {} 0 2352 audio.bin 0", data_frames).unwrap();
    drop(f);
    gdi_path
}

#[test]
fn create_from_gdi_two_tracks() {
    let dir = tmpdir();
    let gdi = write_gdi_fixture(dir.path());

    let chd_path = dir.path().join("from_gdi.chd");
    cd::create_from_cue(
        &gdi,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .expect("create_from_cue on a .gdi");

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let info = chd.info().unwrap();
    // A .gdi sets CD_FLAG_GDROM, so the CHD is written as a GD-ROM
    // (CHGD metadata) and reports is_gd, not is_cd.
    assert!(info.is_gd, "GDI-derived CHD should report as a GD-ROM");
    assert_eq!(info.track_count, 2);

    let tracks = cd::list_tracks(&chd).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].track_type, TrackType::Mode1Raw);
    assert_eq!(tracks[0].frames, 43);
    assert_eq!(tracks[1].track_type, TrackType::Audio);
    assert_eq!(tracks[1].frames, 225);
}

#[test]
fn extract_to_gdi_roundtrips_two_track_chd() {
    // Build a CHD from the GDI fixture, extract it back to GDI, and
    // confirm the per-track files match the originals byte-for-byte.
    // MODE1_RAW data is stored verbatim; audio is byte-swapped on input
    // (GDI swap=true) and swapped back on output (v5+), so both round-trip.
    let dir = tmpdir();
    let gdi = write_gdi_fixture(dir.path());
    let data_orig = std::fs::read(dir.path().join("data.bin")).unwrap();
    let audio_orig = std::fs::read(dir.path().join("audio.bin")).unwrap();

    let chd_path = dir.path().join("roundtrip.chd");
    cd::create_from_cue(
        &gdi,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let out_dir = tmpdir();
    let gdi_out = out_dir.path().join("out.gdi");
    let mut last_progress = 0u64;
    cd::extract_to_gdi(&chd_path, &gdi_out, &mut |b| last_progress = b).unwrap();

    // Per-track split files: out01.bin (data), out02.raw (audio).
    let data_out = std::fs::read(out_dir.path().join("out01.bin")).unwrap();
    let audio_out = std::fs::read(out_dir.path().join("out02.raw")).unwrap();
    assert_eq!(data_out, data_orig, "extracted data track mismatch");
    assert_eq!(audio_out, audio_orig, "extracted audio track mismatch");
    assert_eq!(
        last_progress,
        (data_orig.len() + audio_orig.len()) as u64,
        "progress should total all written bytes"
    );

    // GDI index: count line, then one entry per track.
    let gdi_text = std::fs::read_to_string(&gdi_out).unwrap();
    let lines: Vec<&str> = gdi_text.lines().collect();
    assert_eq!(lines[0], "2", "first line is the track count");
    // track# lba type datasize file offset
    assert_eq!(lines[1], "1 0 4 2352 out01.bin 0");
    assert_eq!(lines[2], "2 43 0 2352 out02.raw 0");
}

#[test]
fn extract_to_gdi_quotes_filenames_with_spaces() {
    // A gdi stem containing a space must be quoted in the index line so
    // the file is parseable back (matches chdman's quoting rule).
    let dir = tmpdir();
    let gdi = write_gdi_fixture(dir.path());
    let chd_path = dir.path().join("q.chd");
    cd::create_from_cue(
        &gdi,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let out_dir = tmpdir();
    let gdi_out = out_dir.path().join("my disc.gdi");
    cd::extract_to_gdi(&chd_path, &gdi_out, &mut |_| {}).unwrap();

    assert!(out_dir.path().join("my disc01.bin").exists());
    assert!(out_dir.path().join("my disc02.raw").exists());
    let gdi_text = std::fs::read_to_string(&gdi_out).unwrap();
    assert!(
        gdi_text.contains("\"my disc01.bin\""),
        "spaced filename must be quoted: {gdi_text}"
    );
}
