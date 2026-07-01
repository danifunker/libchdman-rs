use std::sync::atomic::{AtomicU32, Ordering};

use std::io::{Read, Seek, SeekFrom, Write};

use libchdman_rs::cd::{self, CdCookedReader, CdCreateOptions, SubcodeType, TrackType};
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
    assert_eq!(
        combined, expected,
        "extracted .bin must match original BIN concatenation"
    );

    // CUE shape: one FILE entry, two TRACK entries with INDEX 01 at
    // their offsets, TRACK 02 starts at MSF for 43 frames.
    let cue_text = std::fs::read_to_string(&cue_out).unwrap();
    assert!(
        cue_text.starts_with("FILE \"out.bin\" BINARY\n"),
        "cue: {cue_text}"
    );
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

#[test]
fn extract_to_cue_handles_pregaps_that_shift_logical_frames() {
    // Regression for the CD-Extra / mixed-mode bug: multiple audio tracks
    // each carrying a PREGAP push a track's *logical* frame offset above
    // its *physical* one (logofs += pregap). extract_to_cue reads via
    // physical CHD addressing (phys=true), so it must start each track at
    // `get_track_start_phys`, not `get_track_start`. Before the fix, the
    // logical start was fed to a physical read and the extraction ran off
    // the end of the stored frames, dying ~`pregap` frames early with
    // InvalidData. Here three audio tracks with pregaps reproduce that
    // exact geometry; extraction must complete and round-trip byte-exact.
    let dir = tmpdir();

    // Deterministic per-track audio payloads (LE; MAME byte-swaps to BE on
    // store, extract_to_cue swaps back, so the bytes must round-trip).
    let make_bin = |seed: u32, frames: u32| -> Vec<u8> {
        let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
        let mut v = Vec::with_capacity((frames as usize) * 2352);
        for _ in 0..(frames as usize) * 2352 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            v.push((x >> 16) as u8);
        }
        v
    };
    let frames = [30u32, 40, 50];
    let seeds = [1u32, 7, 99];
    let mut source = Vec::new();
    for i in 0..3 {
        let bin = make_bin(seeds[i], frames[i]);
        source.extend_from_slice(&bin);
        std::fs::write(dir.path().join(format!("t{}.bin", i + 1)), &bin).unwrap();
    }

    // Track 1 has no pregap; tracks 2 and 3 carry PREGAPs (pgdatasize == 0)
    // so cumulative logofs diverges from physofs by the running pregap sum.
    let cue = "\
FILE \"t1.bin\" BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE \"t2.bin\" BINARY
  TRACK 02 AUDIO
    PREGAP 00:02:00
    INDEX 01 00:00:00
FILE \"t3.bin\" BINARY
  TRACK 03 AUDIO
    PREGAP 00:03:00
    INDEX 01 00:00:00
";
    let cue_path = dir.path().join("cdextra.cue");
    std::fs::write(&cue_path, cue).unwrap();

    let chd_path = dir.path().join("cdextra.chd");
    cd::create_from_cue(
        &cue_path,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    // Sanity: the CHD really has pregaps on later tracks (the bug trigger).
    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let tracks = cd::list_tracks(&chd).unwrap();
    assert_eq!(tracks.len(), 3);
    assert!(
        tracks[1].pregap > 0 && tracks[2].pregap > 0,
        "fixture must carry pregaps to exercise the bug: {:?}",
        tracks.iter().map(|t| t.pregap).collect::<Vec<_>>()
    );
    drop(chd);

    let cue_out = dir.path().join("out.cue");
    let bin_out = dir.path().join("out.bin");
    let mut last = 0u64;
    // Before the fix this returned Err(InvalidData) partway through track 3.
    cd::extract_to_cue(&chd_path, &cue_out, &bin_out, &mut |b| last = b)
        .expect("extract_to_cue must not fail on pregap-bearing audio tracks");

    let extracted = std::fs::read(&bin_out).unwrap();
    let total_frames: u32 = frames.iter().sum();
    assert_eq!(
        extracted.len() as u64,
        total_frames as u64 * 2352,
        "extracted bin must contain every stored frame"
    );
    assert_eq!(
        last,
        extracted.len() as u64,
        "final progress == bytes written"
    );
    assert_eq!(
        extracted, source,
        "extracted audio must round-trip byte-exact to the source tracks"
    );
}

#[test]
fn extract_to_iso_handles_data_track_with_pregap() {
    // A single MODE1 data track that carries a PREGAP has
    // logframeofs > physframeofs. extract_to_iso reads via physical
    // addressing (phys=true), so it must start at physframeofs. Before the
    // fix the logical start walked past the stored frames -> InvalidData.
    let dir = tmpdir();
    let frames = 64usize;
    let mut source = Vec::with_capacity(frames * 2048);
    for s in 0..frames {
        for i in 0..2048usize {
            source.push(
                (s.wrapping_mul(31)
                    .wrapping_add(i.wrapping_mul(7))
                    .wrapping_add(3)) as u8,
            );
        }
    }
    std::fs::write(dir.path().join("data.bin"), &source).unwrap();
    let cue = "\
FILE \"data.bin\" BINARY
  TRACK 01 MODE1/2048
    PREGAP 00:02:00
    INDEX 01 00:00:00
";
    let cue_path = dir.path().join("d.cue");
    std::fs::write(&cue_path, cue).unwrap();
    let chd_path = dir.path().join("d.chd");
    cd::create_from_cue(
        &cue_path,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    // Sanity: the track really carries a pregap (the bug trigger).
    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let tracks = cd::list_tracks(&chd).unwrap();
    assert_eq!(tracks.len(), 1);
    assert!(
        tracks[0].pregap > 0,
        "fixture must carry a pregap: {:?}",
        tracks[0]
    );
    drop(chd);

    let iso_out = dir.path().join("out.iso");
    let mut last = 0u64;
    cd::extract_to_iso(&chd_path, &iso_out, &mut |b| last = b)
        .expect("extract_to_iso must handle a pregap-bearing data track");
    let extracted = std::fs::read(&iso_out).unwrap();
    assert_eq!(
        extracted, source,
        "extracted ISO must round-trip to source user data"
    );
    assert_eq!(last, source.len() as u64);
}

// ---------- CdCookedReader: multi-track support ----------

#[test]
fn cd_cooked_reader_reads_data_track_after_pregap_tracks() {
    // CD-Extra-like layout: audio tracks (one carrying a PREGAP) followed
    // by a MODE1 data track. Cumulative pregap pushes the data track's
    // logframeofs above its physframeofs, so open_track (phys=true) must
    // use the physical start. Before the fix, reading the trailing data
    // track's filesystem errored / returned shifted data.
    let dir = tmpdir();
    let audio = |seed: u32, frames: usize| -> Vec<u8> {
        let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
        let mut v = Vec::with_capacity(frames * 2352);
        for _ in 0..frames * 2352 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            v.push((x >> 16) as u8);
        }
        v
    };
    std::fs::write(dir.path().join("a1.bin"), audio(1, 20)).unwrap();
    std::fs::write(dir.path().join("a2.bin"), audio(2, 25)).unwrap();

    let dframes = 40usize;
    let mut data = Vec::with_capacity(dframes * 2048);
    for s in 0..dframes {
        for i in 0..2048usize {
            data.push(
                (s.wrapping_mul(17)
                    .wrapping_add(i.wrapping_mul(3))
                    .wrapping_add(9)) as u8,
            );
        }
    }
    std::fs::write(dir.path().join("d.bin"), &data).unwrap();

    let cue = "\
FILE \"a1.bin\" BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE \"a2.bin\" BINARY
  TRACK 02 AUDIO
    PREGAP 00:02:00
    INDEX 01 00:00:00
FILE \"d.bin\" BINARY
  TRACK 03 MODE1/2048
    INDEX 01 00:00:00
";
    let cue_path = dir.path().join("mixed.cue");
    std::fs::write(&cue_path, cue).unwrap();
    let chd_path = dir.path().join("mixed.chd");
    cd::create_from_cue(
        &cue_path,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let tracks = cd::list_tracks(&chd).unwrap();
    assert_eq!(tracks.len(), 3);
    assert!(tracks[1].pregap > 0, "track 2 must carry a pregap");
    assert_eq!(tracks[2].track_type, TrackType::Mode1);

    // Read the trailing data track (index 2) as cooked 2048-byte sectors.
    let mut rdr = CdCookedReader::open_track(chd, 2).unwrap();
    assert_eq!(rdr.len(), data.len() as u64);
    let mut got = Vec::new();
    rdr.read_to_end(&mut got)
        .expect("cooked read of pregap-shifted data track must not fail");
    assert_eq!(
        got, data,
        "cooked reader must return the data track's user bytes"
    );
}

// MODE1/2352 raw sector layout: 12 sync + 4 header + 2048 user + 4 EDC
// + 8 reserved + 276 ECC. We only care about where the user payload
// sits when reading bytes back out of the original BIN.
const RAW_SECTOR_BYTES: usize = 2352;
const SYNC_HEADER_BYTES: usize = 16;
const USER_BYTES: usize = 2048;

/// Build a MODE1/2352 raw .bin from a sequence of cooked 2048-byte payloads.
/// We don't bother computing EDC/ECC — chdman will rewrite a fresh set of
/// MODE1_RAW sectors with valid syndromes when we extract or read back,
/// and CdCookedReader returns only the user bytes anyway.
fn synth_mode1_raw_bin(payloads: &[[u8; USER_BYTES]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payloads.len() * RAW_SECTOR_BYTES);
    for payload in payloads {
        // Sync pattern: 00 FF*10 00
        out.push(0);
        out.extend(std::iter::repeat_n(0xFFu8, 10));
        out.push(0);
        // Header (MSF + mode byte): zeros are fine; chdman recomputes
        // syndromes during MODE1_RAW encoding.
        out.extend([0u8; 3]);
        out.push(1);
        out.extend_from_slice(payload);
        // EDC + intermediate + ECC: 4 + 8 + 276 = 288 zero bytes.
        out.extend(std::iter::repeat_n(0u8, 288));
    }
    out
}

#[test]
fn open_track_single_track_parity() {
    // Build a single-track MODE1_RAW CHD from a synthetic .bin; compare
    // open(chd) and open_track(chd, 0) byte-for-byte over the first N
    // sectors. They must produce identical output.
    let dir = tmpdir();
    let bin_path = dir.path().join("single.bin");
    let cue_path = dir.path().join("single.cue");
    let chd_path = dir.path().join("single.chd");

    let mut payloads: Vec<[u8; USER_BYTES]> = Vec::new();
    for lba in 0u32..32 {
        let mut p = [0u8; USER_BYTES];
        for (i, b) in p.iter_mut().enumerate() {
            *b = ((lba as usize + i) & 0xFF) as u8;
        }
        payloads.push(p);
    }
    std::fs::write(&bin_path, synth_mode1_raw_bin(&payloads)).unwrap();
    let mut cue = std::fs::File::create(&cue_path).unwrap();
    writeln!(cue, "FILE \"single.bin\" BINARY").unwrap();
    writeln!(cue, "  TRACK 01 MODE1/2352").unwrap();
    writeln!(cue, "    INDEX 01 00:00:00").unwrap();
    drop(cue);

    cd::create_from_cue(
        &cue_path,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let read_full = |reader: &mut CdCookedReader| {
        let mut buf = vec![0u8; 16 * USER_BYTES];
        reader.read_exact(&mut buf).unwrap();
        buf
    };
    let chd_a = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let mut r_open = CdCookedReader::open(chd_a).unwrap();
    let bytes_open = read_full(&mut r_open);

    let chd_b = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let mut r_open_track = CdCookedReader::open_track(chd_b, 0).unwrap();
    let bytes_open_track = read_full(&mut r_open_track);

    assert_eq!(
        bytes_open, bytes_open_track,
        "open and open_track(_, 0) must produce identical bytes for a single-track CHD"
    );
}

#[test]
fn open_track_multi_data_track_distinguishable() {
    // Two MODE1/2352 data tracks with distinguishable LBA-0 payloads.
    // open_track(chd, 0) and open_track(chd, 1) must return different
    // bytes at offset 0 — proving the per-track LBA offset works.
    let dir = tmpdir();
    let chd_path = dir.path().join("multi_data.chd");

    let mut t1_payloads: Vec<[u8; USER_BYTES]> = Vec::with_capacity(8);
    let mut t2_payloads: Vec<[u8; USER_BYTES]> = Vec::with_capacity(8);
    for _ in 0..8 {
        t1_payloads.push([0xAA; USER_BYTES]);
        t2_payloads.push([0xBB; USER_BYTES]);
    }
    std::fs::write(dir.path().join("t1.bin"), synth_mode1_raw_bin(&t1_payloads)).unwrap();
    std::fs::write(dir.path().join("t2.bin"), synth_mode1_raw_bin(&t2_payloads)).unwrap();
    let cue_path = dir.path().join("multi.cue");
    let mut cue = std::fs::File::create(&cue_path).unwrap();
    writeln!(cue, "FILE \"t1.bin\" BINARY").unwrap();
    writeln!(cue, "  TRACK 01 MODE1/2352").unwrap();
    writeln!(cue, "    INDEX 01 00:00:00").unwrap();
    writeln!(cue, "FILE \"t2.bin\" BINARY").unwrap();
    writeln!(cue, "  TRACK 02 MODE1/2352").unwrap();
    writeln!(cue, "    INDEX 01 00:00:00").unwrap();
    drop(cue);

    cd::create_from_cue(
        &cue_path,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let chd0 = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let mut r0 = CdCookedReader::open_track(chd0, 0).unwrap();
    let mut b0 = [0u8; USER_BYTES];
    r0.read_exact(&mut b0).unwrap();
    assert_eq!(b0, [0xAA; USER_BYTES], "track 0 LBA 0 should be 0xAA");

    let chd1 = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let mut r1 = CdCookedReader::open_track(chd1, 1).unwrap();
    let mut b1 = [0u8; USER_BYTES];
    r1.read_exact(&mut b1).unwrap();
    assert_eq!(b1, [0xBB; USER_BYTES], "track 1 LBA 0 should be 0xBB");

    assert_ne!(
        b0, b1,
        "open_track at index 0 and 1 must yield different bytes at offset 0"
    );
}

#[test]
fn open_track_rejects_audio_track() {
    // Use the data + audio fixture. Track 1 is Audio — open_track must
    // return UnsupportedFormat, not panic.
    let _ = common::fixtures_dir();
    let cue = common::fixture_path(common::assets::CD_CUE);
    let dir = tmpdir();
    let chd_path = dir.path().join("data_audio.chd");
    cd::create_from_cue(
        &cue,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let res = CdCookedReader::open_track(chd, 1);
    assert!(matches!(res, Err(ChdError::UnsupportedFormat)));
}

#[test]
fn open_track_rejects_out_of_range_index() {
    let _ = common::fixtures_dir();
    let cue = common::fixture_path(common::assets::CD_CUE);
    let dir = tmpdir();
    let chd_path = dir.path().join("oor.chd");
    cd::create_from_cue(
        &cue,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let res = CdCookedReader::open_track(chd, 99);
    assert!(matches!(res, Err(ChdError::InvalidData)));
}

/// Build a MODE2/2352 raw .bin from 2048-byte payloads. The 24-byte
/// preamble is sync (12) + header (4) + subheader (8). MAME's
/// `cdrom_file::read_data` with `datatype=CD_TRACK_MODE1` (= 0) against
/// a `MODE2_RAW` track strips the first 24 bytes and returns the next
/// 2048 — so we just need to place our payload at offset 24 and zero
/// the trailing EDC/ECC region (280 bytes). chdman stores MODE2_RAW
/// sectors verbatim.
fn synth_mode2_raw_bin(payloads: &[[u8; USER_BYTES]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payloads.len() * RAW_SECTOR_BYTES);
    for payload in payloads {
        // 12 bytes sync
        out.push(0);
        out.extend(std::iter::repeat_n(0xFFu8, 10));
        out.push(0);
        // 4 bytes header: MSF + mode=2
        out.extend([0u8; 3]);
        out.push(2);
        // 8 bytes subheader. Form 1 indicated by submode bit 5 == 0 in both
        // copies; chdman doesn't care, MAME just blindly strips 24 bytes
        // for our datatype path.
        out.extend([0u8; 8]);
        out.extend_from_slice(payload);
        // 4 EDC + 276 ECC = 280 trailing bytes.
        out.extend(std::iter::repeat_n(0u8, 280));
    }
    out
}

#[test]
fn open_track_mode2_form1_data_track() {
    // Synthesize a single-track MODE2/2352 (a.k.a. MODE2_RAW) CHD with
    // distinguishable user data per sector. Confirm CdCookedReader at
    // LBA 16 returns the exact 2048 user bytes from the source .bin.
    let dir = tmpdir();
    let bin_path = dir.path().join("m2.bin");
    let cue_path = dir.path().join("m2.cue");
    let chd_path = dir.path().join("m2.chd");

    let mut payloads: Vec<[u8; USER_BYTES]> = Vec::new();
    for lba in 0u32..32 {
        let mut p = [0u8; USER_BYTES];
        for (i, b) in p.iter_mut().enumerate() {
            *b = ((lba.wrapping_mul(7) as usize).wrapping_add(i) & 0xFF) as u8;
        }
        payloads.push(p);
    }
    std::fs::write(&bin_path, synth_mode2_raw_bin(&payloads)).unwrap();
    let mut cue = std::fs::File::create(&cue_path).unwrap();
    writeln!(cue, "FILE \"m2.bin\" BINARY").unwrap();
    writeln!(cue, "  TRACK 01 MODE2/2352").unwrap();
    writeln!(cue, "    INDEX 01 00:00:00").unwrap();
    drop(cue);

    cd::create_from_cue(
        &cue_path,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    // Verify the trktype landed as MODE2_RAW so we know we're exercising
    // the Mode-2 conversion path, not falling back through Mode-1.
    {
        let chd_inspect = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
        let tracks = cd::list_tracks(&chd_inspect).unwrap();
        assert_eq!(
            tracks[0].track_type,
            TrackType::Mode2Raw,
            "MODE2/2352 in CUE must produce Mode2Raw trktype"
        );
    }

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let mut reader = CdCookedReader::open_track(chd, 0).unwrap();
    let lba = 16usize;
    reader
        .seek(SeekFrom::Start((lba * USER_BYTES) as u64))
        .unwrap();
    let mut got = vec![0u8; USER_BYTES];
    reader.read_exact(&mut got).unwrap();
    assert_eq!(
        &got[..],
        &payloads[lba][..],
        "Mode 2 Form 1 LBA 16 must round-trip via CHD encode + cooked read"
    );
}

#[test]
fn open_track_mode2_plus_audio_multi_track() {
    // PSX/Saturn-style layout: one MODE2/2352 data track + one AUDIO
    // track. open_track(chd, 0) reads the data; open_track(chd, 1)
    // returns UnsupportedFormat for the audio track.
    let dir = tmpdir();

    let mut data_payloads: Vec<[u8; USER_BYTES]> = Vec::new();
    for _ in 0..16 {
        data_payloads.push([0xCC; USER_BYTES]);
    }
    std::fs::write(
        dir.path().join("data.bin"),
        synth_mode2_raw_bin(&data_payloads),
    )
    .unwrap();

    // Minimal AUDIO track: 8 frames * 2352 bytes of zero.
    std::fs::write(
        dir.path().join("audio.bin"),
        vec![0u8; 8 * RAW_SECTOR_BYTES],
    )
    .unwrap();

    let cue_path = dir.path().join("psx_like.cue");
    let mut cue = std::fs::File::create(&cue_path).unwrap();
    writeln!(cue, "FILE \"data.bin\" BINARY").unwrap();
    writeln!(cue, "  TRACK 01 MODE2/2352").unwrap();
    writeln!(cue, "    INDEX 01 00:00:00").unwrap();
    writeln!(cue, "FILE \"audio.bin\" BINARY").unwrap();
    writeln!(cue, "  TRACK 02 AUDIO").unwrap();
    writeln!(cue, "    INDEX 01 00:00:00").unwrap();
    drop(cue);

    let chd_path = dir.path().join("psx_like.chd");
    cd::create_from_cue(
        &cue_path,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    // Data track 0: read and confirm payload.
    let chd0 = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let tracks = cd::list_tracks(&chd0).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].track_type, TrackType::Mode2Raw);
    assert_eq!(tracks[1].track_type, TrackType::Audio);

    let mut r0 = CdCookedReader::open_track(chd0, 0).unwrap();
    let mut b0 = [0u8; USER_BYTES];
    r0.read_exact(&mut b0).unwrap();
    assert_eq!(b0, [0xCC; USER_BYTES], "Mode 2 data track LBA 0 payload");

    // Audio track 1: must reject with UnsupportedFormat.
    let chd1 = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let res = CdCookedReader::open_track(chd1, 1);
    assert!(matches!(res, Err(ChdError::UnsupportedFormat)));
}

#[test]
fn open_track_lba_16_roundtrips_data_track() {
    // The 2-track fixture: track 0 is MODE1_RAW data, track 1 is Audio.
    // Sector 16 of the data track is what an ISO 9660 consumer reads
    // first (the Primary Volume Descriptor). Verify CdCookedReader's
    // bytes at position 16 * 2048 match the same offset in the raw .bin.
    let _ = common::fixtures_dir();
    let cue = common::fixture_path(common::assets::CD_CUE);
    let data_bin_path = common::fixture_path(common::assets::CD_DATA_BIN);
    let data_bin = std::fs::read(&data_bin_path).unwrap();
    let lba = 16usize;
    let raw_start = lba * RAW_SECTOR_BYTES + SYNC_HEADER_BYTES;
    let expected = &data_bin[raw_start..raw_start + USER_BYTES];

    let dir = tmpdir();
    let chd_path = dir.path().join("pvd.chd");
    cd::create_from_cue(
        &cue,
        &chd_path,
        CdCreateOptions::default(),
        &mut |_| {},
        &|| false,
    )
    .unwrap();

    let chd = Chd::open(chd_path.to_str().unwrap(), false, None).unwrap();
    let mut reader = CdCookedReader::open_track(chd, 0).unwrap();
    reader
        .seek(SeekFrom::Start((lba * USER_BYTES) as u64))
        .unwrap();
    let mut got = vec![0u8; USER_BYTES];
    reader.read_exact(&mut got).unwrap();
    assert_eq!(
        &got, expected,
        "LBA 16 user data must round-trip via CHD encode + cooked read"
    );
}
