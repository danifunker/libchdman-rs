//! Shared fixture builders for integration tests.
//!
//! All fixtures are generated at test runtime via the create/write API so
//! the repo stays free of binary blobs and licensing concerns. Sizes are
//! kept small (≤ 1 MB each) so the suite runs in well under a second.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use libchdman_rs::metadata::tags::{CDROM_TRACK_METADATA2_TAG, HARD_DISK_METADATA_TAG};
use libchdman_rs::Chd;
use tempfile::TempDir;

/// Filenames inside `tests/fixtures/fixtures.zip`. Tests reference these
/// constants rather than hard-coding the names.
pub mod assets {
    pub const DVD_ISO: &str = "libchdman-rs-test-dvd.iso";
    pub const CD_ISO: &str = "libchdman-rs-test-simple.iso";
    pub const CD_AUDIO_BIN: &str = "libchdman-rs-test-audio.bin";
    pub const CD_DATA_BIN: &str = "libchdman-rs-test-data.bin";
    pub const CD_CUE: &str = "libchdman-rs-cd.cue";
}

/// Returns a path to the extracted fixtures directory, extracting on first
/// call. Subsequent calls (in this process or any other) reuse the
/// extracted contents as long as the source zip hasn't changed.
///
/// Extraction target lives under `target/test-fixtures/` so it's cleaned by
/// `cargo clean` and never pollutes the source tree.
pub fn fixtures_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(extract_fixtures).as_path()
}

/// Convenience: full path to one of the [`assets`] entries.
pub fn fixture_path(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

fn extract_fixtures() -> PathBuf {
    let zip_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fixtures.zip");
    let zip_meta = std::fs::metadata(&zip_path).expect("fixtures.zip must exist");
    let stamp = format!(
        "{}-{}",
        zip_meta.len(),
        zip_meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-fixtures");
    let stamp_path = out_dir.join(".stamp");

    // Reuse if stamp matches.
    if let Ok(existing) = std::fs::read_to_string(&stamp_path) {
        if existing == stamp {
            return out_dir;
        }
    }

    // Extract into a sibling temp dir then atomically swap, so concurrent
    // test processes can't observe a half-extracted directory.
    std::fs::create_dir_all(&out_dir).expect("create test-fixtures dir");
    let staging = out_dir.with_extension("staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).expect("create staging dir");

    let file = std::fs::File::open(&zip_path).expect("open fixtures.zip");
    let mut archive = zip::ZipArchive::new(file).expect("read fixtures.zip");
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("zip entry");
        let name = entry
            .enclosed_name()
            .expect("safe zip entry name")
            .to_owned();
        let dest = staging.join(&name);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).expect("mkdir entry");
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        let mut out = std::fs::File::create(&dest).expect("create extracted file");
        std::io::copy(&mut entry, &mut out).expect("extract entry");
    }

    // Swap staging → out_dir. Remove old contents first; rename-over-dir
    // isn't portable.
    for entry in std::fs::read_dir(&out_dir).expect("read out_dir") {
        let p = entry.expect("dir entry").path();
        if p == staging {
            continue;
        }
        if p.is_dir() {
            let _ = std::fs::remove_dir_all(&p);
        } else {
            let _ = std::fs::remove_file(&p);
        }
    }
    for entry in std::fs::read_dir(&staging).expect("read staging") {
        let p = entry.expect("staging entry").path();
        let dest = out_dir.join(p.file_name().unwrap());
        std::fs::rename(&p, &dest).expect("move into out_dir");
    }
    let _ = std::fs::remove_dir_all(&staging);

    std::fs::write(&stamp_path, stamp).expect("write stamp");
    out_dir
}

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
