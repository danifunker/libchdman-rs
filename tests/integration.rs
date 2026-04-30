//! End-to-end exercise of the chd-rs-parity surface (Phase B).
//!
//! Fixtures are generated at runtime; see `tests/common/mod.rs`.

mod common;

use std::io::{Read, Seek, SeekFrom};

use libchdman_rs::metadata::tags::{CDROM_TRACK_METADATA2_TAG, HARD_DISK_METADATA_TAG};
use libchdman_rs::{cdrom, metadata, Chd, ChdReader, HunkReader, Version};

// ---------------------------------------------------------------------------
// Raw CHD: hunks iterator, header introspection, ChdReader, HunkReader
// ---------------------------------------------------------------------------

#[test]
fn raw_header_introspection() {
    let f = common::build_raw();
    let chd = Chd::open(f.path_str(), false, None).unwrap();

    assert_eq!(chd.version_typed(), Version::V5);
    let info = chd.info().unwrap();
    assert!(!info.compressed);
    assert!(!info.has_parent);
    assert!(!info.is_hd);
    assert!(!info.is_cd);
    assert_eq!(info.codecs, [0, 0, 0, 0]);
}

#[test]
fn raw_hunks_iterator() {
    let f = common::build_raw();
    let chd = Chd::open(f.path_str(), false, None).unwrap();

    let hb = chd.hunk_bytes() as usize;
    let mut count = 0u32;
    for (idx, hunk) in chd.hunks().enumerate() {
        let hunk = hunk.expect("read hunk");
        let expected = common::raw_expected_hunk(idx as u32, hb);
        assert_eq!(hunk, expected, "hunk {} contents", idx);
        count += 1;
    }
    assert_eq!(count, chd.hunk_count());
}

#[test]
fn raw_chd_reader_read_and_seek() {
    let f = common::build_raw();
    let chd = Chd::open(f.path_str(), false, None).unwrap();
    let total = chd.logical_bytes();
    let hb = chd.hunk_bytes() as usize;

    let mut reader: ChdReader = chd.reader();

    // Read the entire CHD sequentially and check it matches the per-hunk
    // expected pattern.
    let mut all = Vec::with_capacity(total as usize);
    reader.read_to_end(&mut all).unwrap();
    assert_eq!(all.len() as u64, total);
    for h in 0..chd.hunk_count() {
        let expected = common::raw_expected_hunk(h, hb);
        let start = (h as usize) * hb;
        assert_eq!(&all[start..start + hb], &expected[..]);
    }

    // Seek to the middle and read 100 bytes; compare against expected.
    let mid = total / 2;
    reader.seek(SeekFrom::Start(mid)).unwrap();
    let mut buf = [0u8; 100];
    reader.read_exact(&mut buf).unwrap();
    let h = (mid / hb as u64) as u32;
    let off_in_h = (mid % hb as u64) as usize;
    let expected = common::raw_expected_hunk(h, hb);
    assert_eq!(&buf[..], &expected[off_in_h..off_in_h + 100]);

    // Reading past EOF returns 0.
    reader.seek(SeekFrom::End(0)).unwrap();
    let mut tail = [0u8; 16];
    let n = reader.read(&mut tail).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn raw_hunk_reader_buf_read() {
    use std::io::BufRead;

    let f = common::build_raw();
    let chd = Chd::open(f.path_str(), false, None).unwrap();
    let hb = chd.hunk_bytes() as usize;

    let mut hr = HunkReader::new(&chd, 3).unwrap();
    assert_eq!(hr.len(), hb);
    let buf = hr.fill_buf().unwrap();
    assert_eq!(buf, &common::raw_expected_hunk(3, hb)[..]);
    hr.consume(hb);
    assert_eq!(hr.fill_buf().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// HD CHD: is_hd, metadata iterator
// ---------------------------------------------------------------------------

#[test]
fn hd_typing_and_metadata() {
    let f = common::build_hd();
    let chd = Chd::open(f.path_str(), false, None).unwrap();

    let info = chd.info().unwrap();
    assert!(info.is_hd);
    assert!(!info.is_cd);

    let entries: Vec<_> = chd
        .metadata_iter()
        .collect::<Result<_, _>>()
        .expect("metadata iter");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tag, HARD_DISK_METADATA_TAG);
    let s = std::str::from_utf8(&entries[0].data)
        .unwrap()
        .trim_end_matches('\0');
    assert!(s.starts_with("CYLS:1024,HEADS:1,SECS:1,BPS:512"));
}

// ---------------------------------------------------------------------------
// CD CHD: is_cd, metadata iterator yields both tracks
// ---------------------------------------------------------------------------

#[test]
fn cd_typing_and_metadata() {
    let f = common::build_cd();
    let chd = Chd::open(f.path_str(), false, None).unwrap();

    let info = chd.info().unwrap();
    assert!(info.is_cd);
    assert!(!info.is_hd);
    assert_eq!(info.track_count, 2);

    let entries: Vec<_> = chd
        .metadata_iter()
        .collect::<Result<_, _>>()
        .expect("metadata iter");
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| metadata::is_cdrom(e.tag)));
    assert!(entries.iter().all(|e| e.tag == CDROM_TRACK_METADATA2_TAG));

    let tracks: Vec<String> = entries
        .iter()
        .map(|e| {
            std::str::from_utf8(&e.data)
                .unwrap()
                .trim_end_matches('\0')
                .to_string()
        })
        .collect();
    assert!(tracks[0].starts_with("TRACK:1 TYPE:MODE1"));
    assert!(tracks[1].starts_with("TRACK:2 TYPE:AUDIO"));
}

// ---------------------------------------------------------------------------
// Cross-cutting sanity
// ---------------------------------------------------------------------------

#[test]
fn cdrom_constants_self_consistent() {
    assert_eq!(
        cdrom::CD_FRAME_SIZE,
        cdrom::CD_MAX_SECTOR_DATA + cdrom::CD_MAX_SUBCODE_DATA
    );
    assert_eq!(cdrom::CD_SYNC_HEADER.len(), cdrom::CD_SYNC_NUM_BYTES);
}

#[test]
fn metadata_tag_to_chars_round_trips_make_tag() {
    let tag = libchdman_rs::make_tag(b'C', b'H', b'T', b'2');
    assert_eq!(metadata::tag_to_chars(tag), ['C', 'H', 'T', '2']);
}
