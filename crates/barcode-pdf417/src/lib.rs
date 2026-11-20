//! PDF417 (ISO/IEC 15438) decoding — clean-room implementation from the
//! public-domain USS-PDF417 spec data, validated against zint oracles.
//!
//! Module map:
//! - [`tables`]: the 3x929 bar-space pattern table (Appendix H-1) plus
//!   the cluster rule and pattern lookup.
//! - [`text_tables`]: the text sub-mode tables (spec Table 3).
//! - [`text`]: text compaction decoding.
//! - [`stream`]: codeword stream extraction from an exact module matrix.
//!
//! Bit conventions: input matrices are `row * cols + col`, 1 = dark;
//! codewords are 17 modules, 4 bars + 4 spaces, bar-first; the data
//! region is `[length descriptor, data, pads(900), ecc]` read row-major.

pub mod detect;
pub mod encode;
pub mod modes;
pub mod rs;
pub mod stream;
pub mod tables;
pub mod text;
pub mod text_tables;

/// Decode the first PDF417 symbol in a binarized frame.
pub use detect::{read_frame, read_frame_rectified};

/// Validate a codeword stream and apply Reed-Solomon correction.
///
/// Shared by the exact-grid and the scan-line paths. The guards matter:
/// a camera grid that is mostly unreadable turns into a stream of zeros,
/// and text compaction decodes codeword 0 as `"AA"`. With a spurious
/// nonzero length descriptor RS happily "corrects" that degenerate block
/// into `[0, 0, ...]` and the reader reports `"AAAA…"` (the live KLM
/// boarding-pass failure). A real symbol has a nonzero descriptor after
/// correction and a body that is not entirely zero.
///
/// PDF417 carries `2^(ec_level+1)` EC codewords (2, 4, ... 512); any
/// other count means the descriptor came from a wrong grid / row count.
pub(crate) fn correct_stream(mut stream: Vec<u16>, erasures: &[usize]) -> Option<Vec<u16>> {
    let desc = *stream.first()? as usize;
    if desc == 0 || desc >= stream.len() {
        return None;
    }
    let k = stream.len() - desc;
    if !matches!(k, 2 | 4 | 8 | 16 | 32 | 64 | 128 | 256 | 512) {
        return None;
    }
    // A real symbol always carries some EC ink; an all-zero body is the
    // degenerate grid signature, never a codeword of a real message.
    if stream[1..].iter().all(|&c| c == 0) {
        return None;
    }
    if !rs::rs_valid(&stream, k) {
        // Known positions first: each erasure costs one slot (up to k)
        // instead of two. A malformed PDF417 cell or a row that never
        // voted is exactly that. Mixed damage (unknown errors too) fails
        // the erasure solve's full verification, so fall back to the
        // error-only pass.
        let fixed = rs::rs_correct_mixed(&stream, k, erasures)
            .or_else(|| rs::rs_correct(&stream, k));
        match fixed {
            Some((fixed, _fixes)) => {
                stream = fixed;
            }
            None => {
                return None;
            }
        }
        // RS corrects *towards* the nearest codeword: for a junk grid the
        // nearest one is often the all-zero codeword, whose descriptor is
        // 0. Re-read it from the corrected stream instead of trusting the
        // pre-correction value (which was nonzero by construction).
        let desc = stream[0] as usize;
        if desc == 0 || desc >= stream.len() {
            return None;
        }
        let k = stream.len() - desc;
        if !matches!(k, 2 | 4 | 8 | 16 | 32 | 64 | 128 | 256 | 512) {
            return None;
        }
    }
    Some(stream)
}

/// Full decode of an exact symbol matrix: geometry -> codeword stream
/// -> RS verify/correct -> mode interpretation -> payload bytes.
/// Returns `None` for anything the v0 (exact-matrix) path cannot read;
/// detection comes later.
pub fn decode_exact(dark: &[u8], w: usize, h: usize) -> Option<Vec<u8>> {
    let Some((raw, placeholders)) = stream::read_exact_tolerant_with_stats(dark, w, h) else {
        return None;
    };
    // Mostly-unreadable grid: reject before RS. A genuine print carries a
    // few malformed codewords; >3/4 means the quad was not the symbol.
    if placeholders.len() * 4 > raw.len() * 3 {
        return None;
    }
    let Some(stream) = correct_stream(raw, &placeholders) else {
        return None;
    };
    let desc = stream[0] as usize;
    modes::interpret(stream::strip_pads(&stream[1..desc]))
}

#[cfg(test)]
mod tests {
    use crate::tables::cluster_of_row;

    /// Spec 2.2.3: row r uses cluster ((r-1) mod 3) * 3.
    #[test]
    fn cluster_sequence() {
        assert_eq!(
            (1..=7).map(cluster_of_row).collect::<Vec<_>>(),
            vec![0, 3, 6, 0, 3, 6, 0]
        );
    }

    /// A real encoded symbol passes the shared stream validation.
    #[test]
    fn correct_stream_accepts_a_real_symbol() {
        let sym = crate::encode::encode_full(b"HELLO", 2).unwrap();
        let stream = crate::stream::read_exact(&sym.matrix, sym.width, sym.height).unwrap();
        assert!(crate::correct_stream(stream, &[]).is_some());
    }

    /// A junk grid's stream: all-zero body plus a spurious descriptor that
    /// makes `k` a legal power of two. RS would "correct" it to all zeros
    /// and text compaction would render that as "AAAA…".
    #[test]
    fn correct_stream_rejects_degenerate_zero_body() {
        let mut stream = vec![0u16; 50];
        stream[0] = 18; // 50 - 18 = 32, a legal EC count
        assert!(crate::correct_stream(stream, &[]).is_none());
        // An all-zero stream is invalid even earlier (descriptor 0).
        assert!(crate::correct_stream(vec![0u16; 50], &[]).is_none());
    }

    /// Spec 2.2.3 worked example: 3 rows, 3 cols, ECC 1 ->
    /// L = (0, 5, 2), R = (2, 0, 5).
    #[test]
    fn spec_example_indicator_values() {
        let (rows, cols, ecc) = (3usize, 3usize, 1usize);
        let y = (rows - 1) / 3;
        let z = ecc * 3 + (rows - 1) % 3;
        let v = cols - 1;
        let x = |row: usize| (row - 1) / 3;
        let l = |row: usize| match cluster_of_row(row) {
            0 => 30 * x(row) + y,
            3 => 30 * x(row) + z,
            _ => 30 * x(row) + v,
        };
        let r = |row: usize| match cluster_of_row(row) {
            0 => 30 * x(row) + v,
            3 => 30 * x(row) + y,
            _ => 30 * x(row) + z,
        };
        assert_eq!((l(1), l(2), l(3)), (0, 5, 2));
        assert_eq!((r(1), r(2), r(3)), (2, 0, 5));
    }
}
