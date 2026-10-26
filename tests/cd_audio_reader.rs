//! `CdAudioReader`: raw 2352-byte sectors read straight out of a CD CHD.
//!
//! The contract is byte-exact parity with `extract_to_cue`'s `.bin`: the
//! whole-disc view must equal the extracted BIN, and each track view must
//! equal that track's slice of it. Every test here extracts the BIN with
//! `extract_to_cue` and compares against it, so the reader and the
//! extractor can't drift apart silently.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use libchdman_rs::cd::{self, CdAudioReader, CdCreateOptions, TrackType};
use libchdman_rs::{Chd, ChdError};

mod common;

const SECTOR: usize = 2352;

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn open(chd_path: &Path) -> Chd {
    Chd::open(chd_path.to_str().unwrap(), false, None).unwrap()
}

/// Deterministic pseudo-random bytes (an LCG, so no rand dependency).
fn noise(seed: u32, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            (x >> 16) as u8
        })
        .collect()
}

/// A MODE1/2352 sector: sync + header + payload. EDC/ECC are left as noise;
/// MAME stores MODE1_RAW verbatim, so they round-trip either way.
fn mode1_raw_sectors(seed: u32, frames: usize) -> Vec<u8> {
    let mut out = noise(seed, frames * SECTOR);
    for s in out.as_chunks_mut::<SECTOR>().0 {
        s[0] = 0;
        s[1..11].fill(0xFF);
        s[11] = 0;
        s[15] = 1;
    }
    out
}

fn create_chd(cue: &Path, chd: &Path) {
    cd::create_from_cue(cue, chd, CdCreateOptions::default(), &mut |_| {}, &|| false)
        .expect("create_from_cue");
}

/// Extract `chd_path` with `extract_to_cue`; returns the `.bin` bytes.
fn extract_bin(chd_path: &Path, dir: &Path) -> Vec<u8> {
    let cue = dir.join("ref.cue");
    let bin = dir.join("ref.bin");
    cd::extract_to_cue(chd_path, &cue, &bin, &mut |_| {}).expect("extract_to_cue");
    std::fs::read(&bin).unwrap()
}

/// Byte ranges of each track in the extracted `.bin`, from the per-track
/// readers' lengths. Also asserts they tile the BIN exactly.
fn track_slices(chd_path: &Path, bin_len: usize) -> Vec<std::ops::Range<usize>> {
    let n = cd::list_tracks(&open(chd_path)).unwrap().len();
    let mut start = 0;
    let mut out = Vec::new();
    for t in 0..n {
        let r = CdAudioReader::open_track(open(chd_path), t as u32).unwrap();
        let len = r.len() as usize;
        out.push(start..start + len);
        start += len;
    }
    assert_eq!(start, bin_len, "track views must tile the whole BIN");
    out
}

fn read_all(r: &mut CdAudioReader) -> Vec<u8> {
    let mut buf = vec![0u8; r.len() as usize];
    r.read_sectors(0, r.sector_count(), &mut buf).unwrap();
    buf
}

/// Full parity check: whole-disc view == BIN, each track view == its slice
/// (via both `read_sectors` and `Read`).
fn assert_parity(chd_path: &Path, dir: &Path) -> Vec<u8> {
    let bin = extract_bin(chd_path, dir);

    let mut whole = CdAudioReader::open(open(chd_path)).unwrap();
    assert_eq!(whole.len(), bin.len() as u64);
    assert_eq!(whole.sector_count() as usize * SECTOR, bin.len());
    assert!(read_all(&mut whole) == bin, "whole-disc view != BIN");

    let mut via_read = Vec::new();
    whole.read_to_end(&mut via_read).unwrap();
    assert!(via_read == bin, "whole-disc Read != BIN");

    for (t, range) in track_slices(chd_path, bin.len()).into_iter().enumerate() {
        let mut r = CdAudioReader::open_track(open(chd_path), t as u32).unwrap();
        assert!(
            read_all(&mut r) == bin[range.clone()],
            "track {t} view != BIN slice"
        );
        let mut got = Vec::new();
        r.read_to_end(&mut got).unwrap();
        assert!(got == bin[range], "track {t} Read != BIN slice");
    }
    bin
}

// ---------- fixtures ----------

/// The shipped 2-track fixture: MODE1/2352 data + audio.
fn data_audio_chd(dir: &Path) -> PathBuf {
    let _ = common::fixtures_dir();
    let chd = dir.join("data_audio.chd");
    create_chd(&common::fixture_path(common::assets::CD_CUE), &chd);
    chd
}

/// CD-Extra-like: audio tracks with PREGAPs (logical start > physical
/// start), an audio track whose pregap is stored data (INDEX 00), then a
/// MODE1/2352 data track in a later session-ish position.
fn cd_extra_chd(dir: &Path) -> PathBuf {
    std::fs::write(dir.join("a1.bin"), noise(1, 30 * SECTOR)).unwrap();
    std::fs::write(dir.join("a2.bin"), noise(2, 41 * SECTOR)).unwrap();
    std::fs::write(dir.join("a3.bin"), noise(3, 77 * SECTOR)).unwrap();
    std::fs::write(dir.join("d.bin"), mode1_raw_sectors(4, 37)).unwrap();
    let cue = "\
FILE \"a1.bin\" BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE \"a2.bin\" BINARY
  TRACK 02 AUDIO
    PREGAP 00:02:00
    INDEX 01 00:00:00
FILE \"a3.bin\" BINARY
  TRACK 03 AUDIO
    INDEX 00 00:00:00
    INDEX 01 00:00:10
FILE \"d.bin\" BINARY
  TRACK 04 MODE1/2352
    PREGAP 00:02:00
    INDEX 01 00:00:00
";
    let cue_path = dir.join("cdextra.cue");
    std::fs::write(&cue_path, cue).unwrap();
    let chd = dir.join("cdextra.chd");
    create_chd(&cue_path, &chd);
    chd
}

/// GD-ROM via a `.gdi` whose tracks aren't LBA-contiguous, so MAME stores
/// `padframes` that extraction skips.
fn gdrom_chd(dir: &Path) -> PathBuf {
    std::fs::write(dir.join("t1.bin"), mode1_raw_sectors(5, 43)).unwrap();
    std::fs::write(dir.join("t2.raw"), noise(6, 75 * SECTOR)).unwrap();
    std::fs::write(dir.join("t3.bin"), mode1_raw_sectors(7, 51)).unwrap();
    let gdi = dir.join("disc.gdi");
    let mut f = std::fs::File::create(&gdi).unwrap();
    writeln!(f, "3").unwrap();
    writeln!(f, "1 0 4 2352 t1.bin 0").unwrap();
    writeln!(f, "2 193 0 2352 t2.raw 0").unwrap();
    writeln!(f, "3 45000 4 2352 t3.bin 0").unwrap();
    drop(f);
    let chd = dir.join("gdrom.chd");
    create_chd(&gdi, &chd);
    chd
}

// ---------- parity ----------

#[test]
fn whole_disc_and_tracks_match_bin_data_plus_audio() {
    let dir = tmpdir();
    let chd = data_audio_chd(dir.path());
    let bin = assert_parity(&chd, dir.path());

    // The audio track's slice is the original little-endian audio.bin.
    let audio_orig = std::fs::read(common::fixture_path(common::assets::CD_AUDIO_BIN)).unwrap();
    let mut audio = CdAudioReader::open_track(open(&chd), 1).unwrap();
    assert!(read_all(&mut audio) == audio_orig);
    assert!(bin.ends_with(&audio_orig));
}

#[test]
fn whole_disc_and_tracks_match_bin_cd_extra_with_pregaps() {
    let dir = tmpdir();
    let chd = cd_extra_chd(dir.path());
    let tracks = cd::list_tracks(&open(&chd)).unwrap();
    assert_eq!(tracks.len(), 4);
    assert!(
        tracks[1].pregap > 0 && tracks[2].pregap > 0 && tracks[3].pregap > 0,
        "fixture must carry pregaps: {:?}",
        tracks.iter().map(|t| t.pregap).collect::<Vec<_>>()
    );
    assert_eq!(tracks[3].track_type, TrackType::Mode1Raw);
    let bin = assert_parity(&chd, dir.path());

    // Track 2 (no stored pregap) reads back as the source audio verbatim.
    let mut a2 = CdAudioReader::open_track(open(&chd), 1).unwrap();
    assert!(read_all(&mut a2) == noise(2, 41 * SECTOR));
    // Track 3 stores its INDEX 00 pregap, so its view is the whole file.
    let mut a3 = CdAudioReader::open_track(open(&chd), 2).unwrap();
    assert!(read_all(&mut a3) == noise(3, 77 * SECTOR));
    assert!(!bin.is_empty());
}

#[test]
fn whole_disc_and_tracks_match_bin_gdrom_padframes() {
    let dir = tmpdir();
    let chd = gdrom_chd(dir.path());
    assert_parity(&chd, dir.path());
}

// ---------- random access ----------

#[test]
fn shuffled_sector_ranges_match_bin() {
    let dir = tmpdir();
    let chd = cd_extra_chd(dir.path());
    let bin = extract_bin(&chd, dir.path());
    let mut r = CdAudioReader::open(open(&chd)).unwrap();
    let total = r.sector_count();

    // Ranges of 1..=9 sectors (crossing hunk and track boundaries), visited
    // in a shuffled order.
    let mut ranges = Vec::new();
    let mut s = 0;
    let mut k = 1;
    while s < total {
        let n = k.min(total - s);
        ranges.push((s, n));
        s += n;
        k = k % 9 + 1;
    }
    let mut x: u32 = 0x1234_5678;
    for i in (1..ranges.len()).rev() {
        x = x.wrapping_mul(1103515245).wrapping_add(12345);
        ranges.swap(i, (x >> 8) as usize % (i + 1));
    }

    for (start, count) in ranges {
        let mut buf = vec![0u8; count as usize * SECTOR];
        r.read_sectors(start, count, &mut buf).unwrap();
        let off = start as usize * SECTOR;
        assert!(
            buf == bin[off..off + buf.len()],
            "sectors {start}..{} differ",
            start + count
        );
    }
}

#[test]
fn seek_then_read_to_end_matches_bin_slice() {
    let dir = tmpdir();
    let chd = cd_extra_chd(dir.path());
    let bin = extract_bin(&chd, dir.path());
    let mut r = CdAudioReader::open(open(&chd)).unwrap();

    for &pos in &[
        0u64,
        1,
        2351,
        2352,
        5000,
        19584 * 3 + 7,
        bin.len() as u64 - 3,
    ] {
        assert_eq!(r.seek(SeekFrom::Start(pos)).unwrap(), pos);
        let mut got = Vec::new();
        r.read_to_end(&mut got).unwrap();
        assert!(got == bin[pos as usize..], "read_to_end after seek {pos}");
    }

    // Relative seeks.
    r.seek(SeekFrom::End(-(SECTOR as i64))).unwrap();
    let mut last = vec![0u8; SECTOR];
    r.read_exact(&mut last).unwrap();
    assert!(last == bin[bin.len() - SECTOR..]);
    r.seek(SeekFrom::Current(-(2 * SECTOR as i64) - 5)).unwrap();
    let mut b = [0u8; 10];
    r.read_exact(&mut b).unwrap();
    let off = bin.len() - 2 * SECTOR - 5;
    assert_eq!(b[..], bin[off..off + 10]);

    // Past the end: reads return EOF, before the start errors.
    r.seek(SeekFrom::Start(bin.len() as u64 + 100)).unwrap();
    assert_eq!(r.read(&mut b).unwrap(), 0);
    assert!(r
        .seek(SeekFrom::Current(-(bin.len() as i64) - 1000))
        .is_err());

    // A track view seeks within its own slice.
    let ranges = track_slices(&chd, bin.len());
    let mut t3 = CdAudioReader::open_track(open(&chd), 3).unwrap();
    t3.seek(SeekFrom::Start(3 * SECTOR as u64 + 11)).unwrap();
    let mut got = Vec::new();
    t3.read_to_end(&mut got).unwrap();
    assert!(got == bin[ranges[3].start + 3 * SECTOR + 11..ranges[3].end]);
}

// ---------- errors ----------

#[test]
fn out_of_range_track_and_sectors_are_invalid_data() {
    let dir = tmpdir();
    let chd = data_audio_chd(dir.path());
    assert!(matches!(
        CdAudioReader::open_track(open(&chd), 2),
        Err(ChdError::InvalidData)
    ));

    let mut r = CdAudioReader::open_track(open(&chd), 1).unwrap();
    let n = r.sector_count();
    let mut one = vec![0u8; SECTOR];
    assert_eq!(r.read_sectors(n, 1, &mut one), Err(ChdError::InvalidData));
    let mut two = vec![0u8; 2 * SECTOR];
    assert_eq!(
        r.read_sectors(n - 1, 2, &mut two),
        Err(ChdError::InvalidData)
    );
    assert_eq!(
        r.read_sectors(u32::MAX, 2, &mut two),
        Err(ChdError::InvalidData)
    );
    // Buffer length must be exactly count * 2352.
    assert_eq!(r.read_sectors(0, 1, &mut two), Err(ChdError::InvalidData));
    // Zero sectors at the end is a valid empty read.
    assert_eq!(r.read_sectors(n, 0, &mut []), Ok(()));
    // The last sector is readable.
    assert_eq!(r.read_sectors(n - 1, 1, &mut one), Ok(()));
}

#[test]
fn cooked_data_track_is_unsupported() {
    // A MODE1/2048 track has no 2352-byte form in the BIN, so the whole
    // disc can't be a uniform sector run. Its audio tracks still open.
    let dir = tmpdir();
    std::fs::write(dir.path().join("a.bin"), noise(9, 20 * SECTOR)).unwrap();
    std::fs::write(dir.path().join("d.bin"), noise(10, 16 * 2048)).unwrap();
    let cue = "\
FILE \"a.bin\" BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE \"d.bin\" BINARY
  TRACK 02 MODE1/2048
    INDEX 01 00:00:00
";
    let cue_path = dir.path().join("cooked.cue");
    std::fs::write(&cue_path, cue).unwrap();
    let chd = dir.path().join("cooked.chd");
    create_chd(&cue_path, &chd);

    assert!(matches!(
        CdAudioReader::open(open(&chd)),
        Err(ChdError::UnsupportedFormat)
    ));
    assert!(matches!(
        CdAudioReader::open_track(open(&chd), 1),
        Err(ChdError::UnsupportedFormat)
    ));

    let bin = extract_bin(&chd, dir.path());
    let mut a = CdAudioReader::open_track(open(&chd), 0).unwrap();
    let audio = read_all(&mut a);
    assert!(audio == noise(9, 20 * SECTOR));
    assert!(bin.starts_with(&audio));
}

#[test]
fn into_inner_returns_a_usable_chd() {
    let dir = tmpdir();
    let chd = data_audio_chd(dir.path());
    let mut r = CdAudioReader::open(open(&chd)).unwrap();
    let mut buf = vec![0u8; SECTOR];
    r.read_sectors(0, 1, &mut buf).unwrap();
    let chd = r.into_inner();
    assert_eq!(cd::list_tracks(&chd).unwrap().len(), 2);
    // And it can be wrapped again.
    let r = CdAudioReader::open_track(chd, 1).unwrap();
    assert_eq!(r.sector_count(), 225);
}

#[test]
fn reader_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<CdAudioReader>();
}

#[test]
fn whole_disc_and_tracks_match_bin_redump_gdrom_cue() {
    // Redump multi-CUE GD-ROM: MAME strips each INDEX 00 pregap into the
    // previous track (`splitframes` at parse time) and pads the
    // single-density area out to LBA 45000.
    let dir = tmpdir();
    let d = dir.path();
    std::fs::write(d.join("t1.bin"), mode1_raw_sectors(11, 300)).unwrap();
    std::fs::write(d.join("t2.bin"), noise(12, 375 * SECTOR)).unwrap();
    std::fs::write(d.join("t3.bin"), mode1_raw_sectors(13, 200)).unwrap();
    std::fs::write(d.join("t4.bin"), noise(14, 260 * SECTOR)).unwrap();
    let cue = "\
REM SINGLE-DENSITY AREA
FILE \"t1.bin\" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
FILE \"t2.bin\" BINARY
  TRACK 02 AUDIO
    INDEX 00 00:00:00
    INDEX 01 00:02:00
REM HIGH-DENSITY AREA
FILE \"t3.bin\" BINARY
  TRACK 03 MODE1/2352
    INDEX 01 00:00:00
FILE \"t4.bin\" BINARY
  TRACK 04 AUDIO
    INDEX 00 00:00:00
    INDEX 01 00:02:00
";
    std::fs::write(d.join("gd.cue"), cue).unwrap();
    let chd = d.join("gd.chd");
    create_chd(&d.join("gd.cue"), &chd);
    let tracks = cd::list_tracks(&open(&chd)).unwrap();
    assert_eq!(tracks.len(), 4);
    // Single-density padding: track 2 is stored out to LBA 45000.
    assert!(tracks[1].frames > 40_000, "{:?}", tracks[1]);
    assert_parity(&chd, d);
}
