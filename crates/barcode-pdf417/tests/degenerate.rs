//! Regression: a junk module grid whose cells all decode to codeword 0
//! (plus one spurious nonzero length descriptor) must not decode.
//!
//! Text compaction renders codeword 0 as `"AA"`, and Reed-Solomon treats
//! `[descriptor, 0, 0, ...]` as a single error away from the all-zero
//! codeword, so before the guard this exact grid decoded to `"AAAA…"`.
//! That is what a camera frame of an unreadable symbol looked like.

use barcode_pdf417::encode::pattern_of;
use barcode_pdf417::{decode_exact, stream};

/// 10 rows x 5 data columns, every cell codeword 0, only the descriptor
/// (18 -> 32 EC codewords) nonzero. Legal start/stop, 10 px bands.
fn degenerate_grid() -> (Vec<u8>, usize, usize) {
    let cols = 5usize;
    let rows = 10usize;
    let band = 10usize;
    let w = 69 + 17 * cols;
    let h = rows * band;
    let mut dark = vec![0u8; w * h];

    // elements are run lengths (dark first), 17 modules per cell
    let mut put = |y: usize, x: usize, runs: &[u8]| {
        let (mut px, mut ink) = (x, true);
        for &len in runs {
            for _ in 0..len {
                dark[y * w + px] = ink as u8;
                px += 1;
            }
            ink = !ink;
        }
    };

    for r in 0..rows {
        let y = r * band;
        let cluster = (r % 3) * 3;
        for dy in 0..band {
            put(y + dy, 0, &stream::START);
            put(y + dy, 17, pattern_of(cluster, 0)); // left indicator
            for c in 0..cols {
                let v = if r == 0 && c == 0 { 18 } else { 0 };
                put(y + dy, 34 + 17 * c, pattern_of(cluster, v));
            }
            put(y + dy, 34 + 17 * cols, pattern_of(cluster, 0)); // right indicator
            put(y + dy, 51 + 17 * cols, &stream::STOP);
        }
    }
    (dark, w, h)
}

#[test]
fn junk_zero_body_grid_does_not_decode() {
    let (dark, w, h) = degenerate_grid();
    // The exact reader still sees the (degenerate) stream.
    let raw = stream::read_exact_tolerant(&dark, w, h).expect("geometry is legal");
    assert_eq!(raw.len(), 50);
    assert_eq!(raw[0], 18);
    assert!(raw[1..].iter().all(|&c| c == 0));
    // ...but the decoder must reject it instead of printing "AAAA…".
    assert_eq!(decode_exact(&dark, w, h), None);
}

#[test]
fn all_placeholder_grid_does_not_decode() {
    // Make every data CELL start light: the run splitter then rejects the
    // row, so the tolerant reader fills the whole stream with placeholders
    // (the geometry still detects 10 bands because the indicators differ
    // per cluster). The placeholder-fraction guard must reject it.
    let (mut dark, w, h) = degenerate_grid();
    for y in 0..h {
        for c in 0..5usize {
            dark[y * w + 34 + 17 * c] = 0;
        }
    }
    let raw = stream::read_exact_tolerant(&dark, w, h).expect("geometry is legal");
    assert!(raw.iter().all(|&c| c == 0));
    assert_eq!(decode_exact(&dark, w, h), None);
}
