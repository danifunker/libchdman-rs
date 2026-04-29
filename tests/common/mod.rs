//! Shared fixture builders for integration tests.
//!
//! All fixtures are generated at test runtime via the create/write API so
//! the repo stays free of binary blobs and licensing concerns. Sizes are
//! kept small (≤ 1 MB each) so the suite runs in well under a second.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use libchdman_rs::metadata::tags::{CDROM_TRACK_METADATA2_TAG, HARD_DISK_METADATA_TAG};
use libchdman_rs::Chd;
use tempfile::TempDir;

/// Owns a temp directory plus the path to a generated CHD inside it.
pub struct Fixture {
    pub dir: TempDir,
    pub path: PathBuf,
}

impl Fixture {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn path_str(&self) -> &str {
        self.path.to_str().unwrap()
    }
}

/// Deterministic LCG so fixtures are byte-identical across runs.
fn fill_pseudo(buf: &mut [u8], seed: u64) {
    let mut s: u64 = seed;
    for b in buf.iter_mut() {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (s >> 33) as u8;
    }
}

/// Synthetic raw CHD: 256 KB, 4 KB hunks, 512 B units, uncompressed.
/// All 64 hunks filled with deterministic pseudo-random bytes derived from
/// the hunk index, so tests can predict expected reads.
pub fn build_raw() -> Fixture {
    const LOGICAL: u64 = 256 * 1024;
    const HUNK: u32 = 4096;
    const UNIT: u32 = 512;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("raw.chd");
    let mut chd =
        Chd::create(path.to_str().unwrap(), LOGICAL, HUNK, UNIT, [0, 0, 0, 0]).expect("create raw");

    let hunk_count = (LOGICAL / HUNK as u64) as u32;
    let mut buf = vec![0u8; HUNK as usize];
    for h in 0..hunk_count {
        fill_pseudo(&mut buf, 0x1000 + h as u64);
        chd.write_hunk(h, &buf).expect("write hunk");
    }
    drop(chd);

    Fixture { dir, path }
}

/// Returns the expected bytes that `build_raw` writes for hunk `n`.
pub fn raw_expected_hunk(hunknum: u32, hunk_bytes: usize) -> Vec<u8> {
    let mut v = vec![0u8; hunk_bytes];
    fill_pseudo(&mut v, 0x1000 + hunknum as u64);
    v
}

/// Synthetic hard-disk CHD: 512 KB with valid HARD_DISK_METADATA_TAG so
/// `Chd::is_hd()` returns true.
///
/// Geometry chosen to match the size: 1024 cylinders × 1 head × 1 sector
/// × 512 bytes = 512 KB.
pub fn build_hd() -> Fixture {
    const CYL: u32 = 1024;
    const HEADS: u32 = 1;
    const SECS: u32 = 1;
    const BPS: u32 = 512;
    const LOGICAL: u64 = (CYL as u64) * (HEADS as u64) * (SECS as u64) * (BPS as u64);
    const HUNK: u32 = 4096;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hd.chd");

    let mut chd =
        Chd::create(path.to_str().unwrap(), LOGICAL, HUNK, BPS, [0, 0, 0, 0]).expect("create hd");

    // MAME's HD metadata format: ASCII string, NUL-terminated.
    let meta = format!("CYLS:{},HEADS:{},SECS:{},BPS:{}", CYL, HEADS, SECS, BPS);
    let mut bytes = meta.into_bytes();
    bytes.push(0);
    chd.write_metadata(HARD_DISK_METADATA_TAG, 0, &bytes, 1)
        .expect("write hd meta");

    // Fill a couple of hunks with a recognisable pattern.
    let mut buf = vec![0u8; HUNK as usize];
    fill_pseudo(&mut buf, 0xCAFE);
    chd.write_hunk(0, &buf).expect("write hd hunk 0");
    fill_pseudo(&mut buf, 0xBABE);
    chd.write_hunk(1, &buf).expect("write hd hunk 1");

    drop(chd);
    Fixture { dir, path }
}

/// Synthetic CD CHD: two CDROM_TRACK_METADATA2_TAG entries
/// (track 1 data MODE1/2048, track 2 audio). Logical size sized for two
/// tiny tracks of 75 frames each (1 second).
///
/// MAME's CD frame size on disc is 2448 bytes (sector + subcode), padded
/// per-track to a 4-frame boundary.
pub fn build_cd() -> Fixture {
    const FRAMES_PER_TRACK: u32 = 75;
    const CD_FRAME: u32 = 2448; // CD_MAX_SECTOR_DATA + CD_MAX_SUBCODE_DATA
                                // Pad each track up to a 4-frame multiple: 75 → 76.
    const PADDED_FRAMES: u32 = 76;
    const HUNK: u32 = CD_FRAME * 8; // chdman default for CDs: 8 frames/hunk
    const TRACKS: u32 = 2;
    const LOGICAL: u64 = (PADDED_FRAMES as u64) * (CD_FRAME as u64) * (TRACKS as u64);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cd.chd");

    let mut chd = Chd::create(
        path.to_str().unwrap(),
        LOGICAL,
        HUNK,
        CD_FRAME,
        [0, 0, 0, 0],
    )
    .expect("create cd");

    let track1 = format!(
        "TRACK:1 TYPE:MODE1 SUBTYPE:NONE FRAMES:{} PREGAP:0 PGTYPE:MODE1 PGSUB:NONE POSTGAP:0",
        FRAMES_PER_TRACK
    );
    let track2 = format!(
        "TRACK:2 TYPE:AUDIO SUBTYPE:NONE FRAMES:{} PREGAP:0 PGTYPE:AUDIO PGSUB:NONE POSTGAP:0",
        FRAMES_PER_TRACK
    );
    for (idx, s) in [track1, track2].iter().enumerate() {
        let mut bytes = s.clone().into_bytes();
        bytes.push(0);
        chd.write_metadata(CDROM_TRACK_METADATA2_TAG, idx as u32, &bytes, 1)
            .expect("write cd meta");
    }

    drop(chd);
    Fixture { dir, path }
}
