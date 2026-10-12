use std::io::Cursor;

use libchdman_rs::hd::{self, HdCreateOptions};
use libchdman_rs::{CHD_CODEC_FLAC, CHD_CODEC_HUFF, CHD_CODEC_LZMA, CHD_CODEC_ZLIB};

#[test]
fn compressed_write_holds_under_simulated_high_core_count() {
    std::env::set_var("LIBCHDMAN_TEST_FORCE_NPROC", "32");

    let dir = tempfile::tempdir().expect("tempdir");
    let chd_path = dir.path().join("hd.chd");

    // 1 MiB of pseudo-random bytes mirrors the rusty-backup repro: enough
    // hunks (256 at hunk_size=4096) for the work queue to fan out across
    // multiple worker threads, and incompressible enough to actually run
    // every codec.
    let payload: Vec<u8> = (0..(1024usize * 1024))
        .map(|i| ((i.wrapping_mul(31)) ^ (i >> 7)) as u8)
        .collect();

    let opts = HdCreateOptions {
        logical_size: payload.len() as u64,
        codecs: [
            CHD_CODEC_LZMA,
            CHD_CODEC_ZLIB,
            CHD_CODEC_HUFF,
            CHD_CODEC_FLAC,
        ],
        ..Default::default()
    };

    hd::create_from_reader(
        Cursor::new(payload.clone()),
        &chd_path,
        opts,
        &mut |_| {},
        &|| false,
    )
    .expect("compressed CHD write should not abort under high simulated nproc");

    let mut out = Vec::new();
    hd::extract_to_writer(&chd_path, &mut out, &mut |_| {}).expect("extract should succeed");
    assert_eq!(out, payload, "compressed CHD did not round-trip");
}
