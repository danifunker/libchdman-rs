//! Extract a CD CHD to a CUE/BIN pair (chdman `extractcd` bin/cue parity).
//!
//! Usage:
//!   cargo run --example extract-cd -- <input.chd> <output.cue> <output.bin>
//!
//! Prints a running progress line and, on success, the total number of
//! bytes and 2352-byte sectors written. On failure it reports the last
//! byte offset reached so a partial-extraction bug can be localized to a
//! specific track/frame.

use std::path::Path;
use std::process::ExitCode;

use libchdman_rs::cd;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: {} <input.chd> <output.cue> <output.bin>", args[0]);
        return ExitCode::from(2);
    }
    let chd = Path::new(&args[1]);
    let cue = Path::new(&args[2]);
    let bin = Path::new(&args[3]);

    let mut last: u64 = 0;
    let mut ticks: u64 = 0;
    let result = cd::extract_to_cue(chd, cue, bin, &mut |written| {
        last = written;
        // Throttle stderr chatter; the exact value is captured in `last`.
        ticks += 1;
        if ticks.is_multiple_of(4096) {
            eprint!(
                "\r  extracted {} bytes ({} sectors)…",
                written,
                written / 2352
            );
        }
    });
    eprintln!();

    match result {
        Ok(()) => {
            println!("OK: wrote {} bytes ({} sectors of 2352)", last, last / 2352);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!(
                "ERROR: extract_to_cue failed with {:?} after {} bytes ({} sectors of 2352)",
                e,
                last,
                last / 2352
            );
            ExitCode::FAILURE
        }
    }
}
