//! Glue between a Rust `Read` source and MAME's `chd_file_compressor`.
//!
//! `chd_file_compressor::read_data(dest, offset, length)` is invoked
//! sequentially: `offset` advances monotonically, always hunk-aligned,
//! and reads can span multiple hunks at a time (MAME's
//! `WORK_BUFFER_HUNKS / 2` ≈ 16 hunks per call). When the requested span
//! runs past the configured `logical_bytes`, MAME truncates `length`
//! itself before calling.
//!
//! [`StreamingSource`] consumes a `Read` and replays it into the
//! compressor's expected access pattern, zero-padding the tail if the
//! reader ends early.
//!
//! [`run_compression`] drives the begin/continue loop, samples the
//! caller's `cancel` predicate between iterations, and on cancellation
//! drops the compressor and unlinks the partial output file.

use std::fs;
use std::io::Read;
use std::path::Path;

use crate::{ChdCompressor, ChdDataHandler, ChdError, CompressStep, CompressionProgress, Result};

/// Adapts a `Read` to the [`ChdDataHandler`] pull model the C++
/// compressor expects.
///
/// **Contract.** MAME calls `read_data` with monotonically increasing,
/// hunk-aligned offsets. Each call requests up to several hunks at a
/// time. This adapter honours that contract and panics if it sees an
/// out-of-order offset, since MAME's invariant would already have been
/// broken upstream and silent corruption is a worse failure mode.
pub(crate) struct StreamingSource<R: Read> {
    reader: R,
    /// Bytes already pulled from the reader (or zero-padded past EOF).
    /// Equal to the offset the next call must arrive with.
    cursor: u64,
    /// The configured logical_bytes for the destination CHD. The compressor
    /// will not request beyond this; we use it only for sanity checks.
    total: u64,
}

impl<R: Read> StreamingSource<R> {
    pub fn new(reader: R, total: u64) -> Self {
        Self {
            reader,
            cursor: 0,
            total,
        }
    }
}

impl<R: Read> ChdDataHandler for StreamingSource<R> {
    fn read_data(&mut self, dest: &mut [u8], offset: u64) -> u32 {
        let length = dest.len();
        // MAME's compressor must call us strictly in order. Anything else
        // would mean MAME re-reads, which it does not, and silently
        // skipping ahead would land random data in the wrong hunks.
        assert_eq!(
            offset, self.cursor,
            "StreamingSource: out-of-order read (cursor {}, requested {})",
            self.cursor, offset
        );
        debug_assert!(
            offset.saturating_add(length as u64) <= self.total,
            "compressor requested {} bytes at {} past logical end {}",
            length,
            offset,
            self.total
        );

        // Drain into `dest`; on EOF, zero-pad the rest. Loop because
        // `Read::read` may return short.
        let mut filled = 0usize;
        while filled < length {
            match self.reader.read(&mut dest[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                // We have no error channel back to MAME beyond returning
                // a short count; map I/O errors to "EOF here" and let the
                // tail get zero-padded. Higher layers compare SHA1 to
                // catch silent truncation if the caller cares.
                Err(_) => break,
            }
        }
        for b in &mut dest[filled..] {
            *b = 0;
        }

        self.cursor = offset + length as u64;
        length as u32
    }
}

/// Drives a compressor through to completion, surfacing progress and
/// honouring cancellation.
///
/// On `Ok(())` the file at `output_path` is fully written and closed.
/// On cancellation: drops the compressor, unlinks `output_path`, and
/// returns `Err(ChdError::Cancelled)`.
/// On any underlying compressor error: best-effort unlinks the partial
/// file and returns the original error.
pub(crate) fn run_compression(
    mut compressor: ChdCompressor,
    output_path: &Path,
    progress: &mut dyn FnMut(CompressionProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    compressor.compress_begin();
    let mut cancelled = false;
    let mut compressor_error: Option<ChdError> = None;
    loop {
        if !cancelled && cancel() {
            cancelled = true;
        }
        match compressor.compress_continue() {
            Ok(CompressStep::Continue(p)) => {
                if !cancelled {
                    progress(p);
                }
            }
            Ok(CompressStep::Done(p)) => {
                if !cancelled {
                    progress(p);
                }
                break;
            }
            Err(e) => {
                compressor_error = Some(e);
                break;
            }
        }
    }
    // Compressor is now idle (workers fully drained). Safe to drop.
    drop(compressor);

    if let Some(e) = compressor_error {
        let _ = fs::remove_file(output_path);
        return Err(e);
    }
    if cancelled {
        let _ = fs::remove_file(output_path);
        return Err(ChdError::Cancelled);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn zero_pads_past_eof() {
        let src = vec![0xAAu8; 100];
        let mut s = StreamingSource::new(Cursor::new(src), 256);
        let mut buf = vec![0xFFu8; 256];
        let n = s.read_data(&mut buf, 0);
        assert_eq!(n, 256);
        assert!(buf[..100].iter().all(|&b| b == 0xAA));
        assert!(buf[100..].iter().all(|&b| b == 0));
    }

    #[test]
    fn sequential_reads_advance() {
        let src: Vec<u8> = (0..200u32).map(|i| i as u8).collect();
        let mut s = StreamingSource::new(Cursor::new(src.clone()), 200);

        let mut a = vec![0u8; 100];
        s.read_data(&mut a, 0);
        let mut b = vec![0u8; 100];
        s.read_data(&mut b, 100);

        let mut combined = a;
        combined.extend(b);
        assert_eq!(combined, src);
    }

    #[test]
    #[should_panic(expected = "out-of-order")]
    fn rejects_non_monotonic_offset() {
        let mut s = StreamingSource::new(Cursor::new(vec![0u8; 100]), 100);
        let mut buf = [0u8; 50];
        s.read_data(&mut buf, 0);
        // Going backwards is the failure we care about.
        s.read_data(&mut buf, 0);
    }

    #[test]
    fn handles_short_reads_from_underlying_reader() {
        // A reader that drips one byte at a time exercises the
        // refill loop inside `read_data`.
        struct Drip {
            data: Vec<u8>,
            pos: usize,
        }
        impl std::io::Read for Drip {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.pos >= self.data.len() || buf.is_empty() {
                    return Ok(0);
                }
                buf[0] = self.data[self.pos];
                self.pos += 1;
                Ok(1)
            }
        }
        let src: Vec<u8> = (0..32u8).collect();
        let mut s = StreamingSource::new(
            Drip {
                data: src.clone(),
                pos: 0,
            },
            32,
        );
        let mut buf = vec![0u8; 32];
        s.read_data(&mut buf, 0);
        assert_eq!(buf, src);
    }
}
