//! Regression guard for the OSD work-queue thread cap.
//!
//! Background: MAME's `chd_file_compressor` declares
//! `codec_class m_codecs[WORK_BUFFER_THREADS]` with `WORK_BUFFER_THREADS = 4`
//! and asserts `threadid < std::size(m_codecs)` per hunk. Our minimal OSD
//! shim (`sys/minimal_osd.cpp`) passes each worker's index as `threadid`,
//! so any host with more than 4 logical CPUs would overflow that array —
//! historically observed as a STATUS_STACK_BUFFER_OVERRUN abort on a 20-CPU
//! Windows host. The shim therefore caps `num_threads` at 4.
//!
//! CI runners have ≤4 vCPUs, so the existing round-trip tests can't trip
//! the OOB on their own. This file forces the shim's effective hardware
//! concurrency to 32 via `LIBCHDMAN_TEST_FORCE_NPROC` and then writes a
//! compressed CHD with the full 4-codec HD stack — exactly the rusty-backup
//! scenario. If the cap is ever removed or bumped above `WORK_BUFFER_THREADS`
//! the C++ assert fires and the test process aborts.
//!
//! This test lives in its own integration-test file so cargo runs it in
//! a dedicated process — setting `LIBCHDMAN_TEST_FORCE_NPROC` here can't
//! leak into other test binaries.

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
