//! Regression: handing a non-CD CHD to a `cd::*` entry point must return an
//! error, not kill the process.
//!
//! MAME's `cdrom_file` reports "this isn't CD media" by throwing a bare
//! `nullptr` (cdrom.cpp:253-260) — a hard-disk CHD has `unit_bytes() == 512`
//! against the required 2448 and trips the very first check. Rust frames
//! cannot unwind a foreign exception, so before `sys/shim_guard.h` every one
//! of these calls aborted the whole process:
//!
//! ```text
//! libc++abi: terminating due to uncaught exception of type std::nullptr_t
//! fatal runtime error: Rust cannot catch foreign exceptions, aborting
//! ```
//!
//! **If this regresses, the test binary dies rather than failing an
//! assertion** — a red run shows up as `SIGABRT` / "test process didn't exit
//! successfully", not a nice assertion diff. Loud, but don't go hunting for
//! an assertion message that won't be there.
//!
//! Each `cd::` entry point that opens a `cdrom_file` gets its own case: they
//! are independent call sites and each one used to abort on its own.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use libchdman_rs::cd::{self, CdCookedReader, CD_FRAME_SIZE, DEFAULT_HUNK_SIZE};
use libchdman_rs::dvd::{self, DvdCreateOptions};
use libchdman_rs::hd::{self, HdCreateOptions};
use libchdman_rs::{Chd, ChdError, CHD_CODEC_NONE};

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i / 7) ^ i) as u8).collect()
}

/// A 256 KiB hard-disk CHD: 512-byte units, GDDD geometry, no CD tracks.
/// This is the shape that took down the reporting consumer.
fn make_hd_chd(dir: &Path) -> PathBuf {
    let path = dir.join("hd.chd");
    let data = payload(256 * 1024);
    hd::create_from_reader(
        Cursor::new(data.clone()),
        &path,
        HdCreateOptions {
            logical_size: data.len() as u64,
            codecs: [CHD_CODEC_NONE; 4],
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .expect("create hd chd");
    path
}

/// A DVD CHD: 2048-byte units, DVD metadata, also not CD media.
fn make_dvd_chd(dir: &Path) -> PathBuf {
    let path = dir.join("dvd.chd");
    let data = payload(128 * 1024);
    dvd::create_from_reader(
        Cursor::new(data.clone()),
        &path,
        DvdCreateOptions {
            logical_size: data.len() as u64,
            codecs: [CHD_CODEC_NONE; 4],
            ..Default::default()
        },
        &mut |_| {},
        &|| false,
    )
    .expect("create dvd chd");
    path
}

/// A CHD with CD geometry (2448-byte units) but no track metadata at all.
/// This is the case that actually reaches MAME's `throw nullptr` — the Rust
/// pre-screen can't reject it, so it exercises the shim's catch.
fn make_cd_shaped_chd_without_metadata(dir: &Path) -> PathBuf {
    let path = dir.join("headless.chd");
    let path_str = path.to_str().unwrap();
    let chd = Chd::create(
        path_str,
        u64::from(DEFAULT_HUNK_SIZE),
        DEFAULT_HUNK_SIZE,
        CD_FRAME_SIZE,
        [0; 4],
    )
    .expect("create cd-shaped chd");
    drop(chd);
    path
}

fn assert_all_cd_entry_points_err(chd_path: &Path, dir: &Path, expected: ChdError) {
    let path_str = chd_path.to_str().unwrap();

    // list_tracks — the call site in the original crash report.
    let chd = Chd::open(path_str, false, None).expect("open chd");
    assert_eq!(cd::list_tracks(&chd).unwrap_err(), expected);
    drop(chd);

    assert_eq!(
        cd::extract_to_cue(
            chd_path,
            &dir.join("out.cue"),
            &dir.join("out.bin"),
            &mut |_| {}
        )
        .unwrap_err(),
        expected
    );

    assert_eq!(
        cd::extract_to_iso(chd_path, &dir.join("out.iso"), &mut |_| {}).unwrap_err(),
        expected
    );

    assert_eq!(
        cd::extract_to_gdi(chd_path, &dir.join("out.gdi"), &mut |_| {}).unwrap_err(),
        expected
    );

    // CdCookedReader isn't Debug, so match rather than unwrap_err.
    let chd = Chd::open(path_str, false, None).expect("open chd");
    assert!(matches!(CdCookedReader::open(chd), Err(e) if e == expected));

    let chd = Chd::open(path_str, false, None).expect("open chd");
    assert!(matches!(CdCookedReader::open_track(chd, 0), Err(e) if e == expected));
}

#[test]
fn hd_chd_errors_from_every_cd_entry_point() {
    let dir = tmpdir();
    let chd_path = make_hd_chd(dir.path());
    assert_all_cd_entry_points_err(&chd_path, dir.path(), ChdError::NotCdMedia);
}

#[test]
fn dvd_chd_errors_from_every_cd_entry_point() {
    let dir = tmpdir();
    let chd_path = make_dvd_chd(dir.path());
    assert_all_cd_entry_points_err(&chd_path, dir.path(), ChdError::NotCdMedia);
}

#[test]
fn cd_shaped_chd_without_track_metadata_errors_from_every_cd_entry_point() {
    let dir = tmpdir();
    let chd_path = make_cd_shaped_chd_without_metadata(dir.path());
    // Geometry passes the Rust pre-screen, so MAME's constructor runs and
    // throws on `parse_metadata` (cdrom.cpp:260). Reaching this assertion at
    // all is the proof that the shim caught it.
    assert_all_cd_entry_points_err(&chd_path, dir.path(), ChdError::InvalidData);
}

#[test]
fn hd_chd_is_still_usable_as_a_hard_disk() {
    // Guard against "fixed" by breaking non-CD paths: the same CHD the cd::
    // calls reject must still read back through hd::.
    let dir = tmpdir();
    let chd_path = make_hd_chd(dir.path());
    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).expect("open chd");
    let info = chd.info().expect("info");
    assert!(!info.is_cd);
    assert_eq!(chd.unit_bytes(), 512);
    assert_eq!(chd.logical_bytes(), 256 * 1024);
}
