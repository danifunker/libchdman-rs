use libchdman_rs::{Chd, ChdError, make_tag};
use std::fs::File;
use std::io::{Read, Write};
use tempfile::tempdir;

#[test]
fn test_open_non_existent() {
    let result = Chd::open("non_existent.chd", false, None);
    assert!(result.is_err());
    assert_eq!(result.err().unwrap(), ChdError::InvalidFile);
}

#[test]
fn test_create_and_metadata() {
    let dir = tempdir().unwrap();
    let chd_path = dir.path().join("test.chd");
    let chd_path_str = chd_path.to_str().unwrap();

    // Create an uncompressed CHD
    // CHD_CODEC_NONE = 0
    let mut chd = Chd::create(chd_path_str, 1024 * 1024, 4096, 512, [0, 0, 0, 0]).expect("Failed to create CHD");

    assert_eq!(chd.version(), 5);
    assert_eq!(chd.hunk_bytes(), 4096);
    assert_eq!(chd.unit_bytes(), 512);
    assert_eq!(chd.logical_bytes(), 1024 * 1024);

    // Test metadata
    let tag = make_tag(b'T', b'E', b'S', b'T');
    let meta_data = b"Hello Metadata";
    chd.write_metadata(tag, 0, meta_data, 1).expect("Failed to write metadata");

    let read_meta = chd.read_metadata(tag, 0).expect("Failed to read metadata");
    assert_eq!(read_meta, meta_data);
}

#[test]
fn test_hunk_io() {
    let dir = tempdir().unwrap();
    let chd_path = dir.path().join("test_io.chd");
    let chd_path_str = chd_path.to_str().unwrap();

    let mut chd = Chd::create(chd_path_str, 8192, 4096, 4096, [0, 0, 0, 0]).expect("Failed to create CHD");

    let mut hunk0 = vec![0u8; 4096];
    for i in 0..4096 { hunk0[i] = (i % 256) as u8; }

    chd.write_hunk(0, &hunk0).expect("Failed to write hunk 0");

    let mut read_back = vec![0u8; 4096];
    chd.read_hunk(0, &mut read_back).expect("Failed to read hunk 0");
    assert_eq!(hunk0, read_back);

    // Partial read/write via bytes
    let mut hunk1_data = vec![0u8; 100];
    for i in 0..100 { hunk1_data[i] = 0xAA; }
    chd.write_bytes(4096 + 50, &hunk1_data).expect("Failed to write bytes");

    let mut read_back_bytes = vec![0u8; 100];
    chd.read_bytes(4096 + 50, &mut read_back_bytes).expect("Failed to read bytes");
    assert_eq!(hunk1_data, read_back_bytes);
}
