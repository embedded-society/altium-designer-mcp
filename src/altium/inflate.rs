//! Bounded zlib inflation for the compressed streams a library holds: its 3D
//! models, its embedded images and its pins' auxiliary entries.
//!
//! Each stream has a cap of its own, and a read's streams together are held to
//! a [`Budget`]: a fixed allowance plus a ratio of the compressed bytes read.
//! A per-stream cap alone would not do, as zlib inflates about a thousand
//! times, so a crafted library of many capped streams could claim memory out
//! of all proportion to its size and abort the process.

use std::io::Read as _;

use flate2::read::ZlibDecoder;

use super::error::{AltiumError, AltiumResult};

/// What a read may inflate at any ratio: one 3D model at its own cap.
const BASE: usize = 256 * 1024 * 1024;

/// What a read may inflate past [`BASE`], per compressed byte. Real libraries
/// stay far below it: across the reference collections and the fixtures, a
/// model inflates at most 8.8 times, and a library's inflated total is at most
/// 3.9 times its file's size. A crafted stream inflates about 1000 times.
const RATIO: usize = 32;

/// How an inflation to a limit ended.
enum Inflation {
    /// The whole stream, within the limit.
    Done(Vec<u8>),
    /// The stream inflates past the limit; reading stopped there.
    PastLimit,
    /// Not a zlib stream, or a damaged one.
    Damaged,
}

/// Inflates `data`, reading at most one byte past `limit`.
fn inflate_to(data: &[u8], limit: usize) -> Inflation {
    let take = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    let mut out = Vec::new();
    if ZlibDecoder::new(data)
        .take(take)
        .read_to_end(&mut out)
        .is_err()
    {
        return Inflation::Damaged;
    }
    if out.len() > limit {
        Inflation::PastLimit
    } else {
        Inflation::Done(out)
    }
}

/// Inflates `data`, a zlib stream, to at most `cap` bytes: `None` when it is
/// not zlib, is damaged or inflates past `cap`.
pub fn inflate_capped(data: &[u8], cap: usize) -> Option<Vec<u8>> {
    match inflate_to(data, cap) {
        Inflation::Done(out) => Some(out),
        Inflation::PastLimit | Inflation::Damaged => None,
    }
}

/// What a library read has inflated, against what its compressed bytes allow.
#[derive(Debug)]
pub struct Budget {
    base: usize,
    ratio: usize,
    compressed: usize,
    inflated: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self::with_limits(BASE, RATIO)
    }
}

impl Budget {
    /// A budget of `base` bytes, plus `ratio` times the compressed bytes read.
    pub const fn with_limits(base: usize, ratio: usize) -> Self {
        Self {
            base,
            ratio,
            compressed: 0,
            inflated: 0,
        }
    }

    /// Inflates `data`, a zlib stream, to at most `cap` bytes and to what the
    /// read has left.
    ///
    /// `Ok(None)` for a stream that is not zlib, is damaged or inflates past its
    /// own `cap`: the caller skips it, as for any damaged stream.
    ///
    /// # Errors
    ///
    /// Once the read's streams together inflate past what their compressed
    /// bytes allow. No real library comes near that, so the library is refused
    /// rather than read on towards an allocation that would abort the process.
    pub fn inflate(&mut self, data: &[u8], cap: usize) -> AltiumResult<Option<Vec<u8>>> {
        self.compressed = self.compressed.saturating_add(data.len());
        let allowance = self
            .base
            .saturating_add(self.compressed.saturating_mul(self.ratio));
        let left = allowance.saturating_sub(self.inflated);
        match inflate_to(data, cap.min(left)) {
            Inflation::Done(out) => {
                self.inflated = self.inflated.saturating_add(out.len());
                Ok(Some(out))
            }
            Inflation::PastLimit if left < cap => Err(AltiumError::compression_error(
                format!(
                    "embedded data inflates past {allowance} bytes from {} compressed bytes, \
                     far beyond any real library, so the library is refused",
                    self.compressed
                ),
                None,
            )),
            Inflation::PastLimit | Inflation::Damaged => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{inflate_capped, Budget};
    use std::io::Write as _;

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).expect("compress");
        encoder.finish().expect("finish")
    }

    #[test]
    fn a_stream_inflates_to_its_cap_and_no_further() {
        let stream = zlib(&[7; 100]);
        assert_eq!(inflate_capped(&stream, 100).as_deref(), Some(&[7; 100][..]));
        assert_eq!(inflate_capped(&stream, 99), None);
        assert_eq!(inflate_capped(b"not zlib", 100), None);
        assert_eq!(inflate_capped(&[], 100), None);
    }

    #[test]
    fn a_stream_past_its_own_cap_or_damaged_is_skipped_not_refused() {
        // A damaged stream, or one past its own cap, is a damaged stream: the
        // library still reads, without it.
        let mut budget = Budget::with_limits(1000, 0);
        assert_eq!(
            budget
                .inflate(&zlib(&[1; 200]), 100)
                .expect("within budget"),
            None
        );
        assert_eq!(
            budget.inflate(b"not zlib", 100).expect("within budget"),
            None
        );
        assert_eq!(
            budget
                .inflate(&zlib(&[2; 50]), 100)
                .expect("within budget")
                .as_deref(),
            Some(&[2; 50][..])
        );
    }

    #[test]
    fn a_read_is_refused_once_its_streams_inflate_past_the_budget() {
        // Every stream is within its own cap; together they pass the base plus
        // the ratio of their compressed bytes, as a crafted library of many
        // capped streams does, so the read is refused rather than allocated.
        let stream = zlib(&[0; 4096]);
        let mut budget = Budget::with_limits(10_000, 2);
        for _ in 0..2 {
            assert!(budget
                .inflate(&stream, 4096)
                .expect("within budget")
                .is_some());
        }
        let err = budget.inflate(&stream, 4096).expect_err("past the budget");
        assert!(err.to_string().contains("refused"), "{err}");
    }

    #[test]
    fn the_ratio_grows_the_budget_with_the_compressed_bytes_read() {
        // Data that hardly compresses earns its own room: real libraries
        // inflate a few times their size, far below the ratio.
        let noise: Vec<u8> = (0..20_000_u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13).to_le_bytes()[0])
            .collect();
        let stream = zlib(&noise);
        let mut budget = Budget::with_limits(0, 32);
        for _ in 0..10 {
            assert!(budget
                .inflate(&stream, 1 << 20)
                .expect("within budget")
                .is_some());
        }
    }
}
