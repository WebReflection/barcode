//! Item 6b: GF(256) erasures for DataMatrix, anchored on the field
//! invoice (12x36 DMRE, payload from the repo's own fixture text) plus a
//! rendered damaged symbol through the real locator.
//!
//! The damage is deliberate and *detectable*: a smeared module is drawn as
//! a half-dark/half-light cell, so the sampler's quadrant vote ties 2–2.
//! That vote is the only erasure evidence a hard binarized mask carries.

use barcode_dm::encode::{encode_as, Encoded};
use barcode_dm::locator::{locate_binary, BinaryMap};
use barcode_dm::placement;
use barcode_dm::{decode_upright, decode_upright_with_erasures, tables};

/// Ground truth from `packages/web/test-codes/field/viva-dm-expected.txt`
/// (the invoice DataMatrix). Fills 12x36 exactly: 21 codewords + pad.
const INVOICE: &[u8] = b"00385F00010004L51QZWWY0003294720";

/// Stream-module positions of the first codewords that are not part of
/// the finder L or the clock track.
fn interior_codewords(rows: usize, cols: usize, n: usize) -> Vec<usize> {
    let spec = tables::spec_for(rows, cols).unwrap();
    let positions = placement::stream_positions(spec);
    (0..spec.codewords())
        .filter(|&k| {
            let (r, c) = positions[8 * k];
            r > 1 && c > 1 && (r as usize) < rows - 2 && (c as usize) < cols - 2
        })
        .take(n)
        .collect()
}

#[test]
fn invoice_metadata_matches_the_field_anchor() {
    let enc = encode_as(INVOICE, 12, 36).expect("invoice payload encodes");
    assert_eq!((enc.spec.rows, enc.spec.cols), (12, 36), "DMRE 12x36");
    let decoded = decode_upright(&enc.matrix, 12, 36).expect("clean decode");
    assert_eq!(decoded.payload.as_deref(), Some(INVOICE));
}

#[test]
fn invoice_survives_more_damage_as_known_erasures() {
    let enc = encode_as(INVOICE, 12, 36).expect("encode");
    let (rows, cols) = (enc.spec.rows, enc.spec.cols);
    let nn = enc.spec.nn(); // 18 parity codewords, t = 9 errors
    let positions = placement::stream_positions(enc.spec);

    // 12 damaged codewords: one module each, rendered as a wrong value
    // (as a reader would sample it) and declared uncertain.
    let damaged_cw = interior_codewords(rows, cols, 12);
    assert_eq!(damaged_cw.len(), 12);
    let mut matrix = enc.matrix.clone();
    let mut uncertain = vec![0u8; rows * cols];
    for &k in &damaged_cw {
        let (r, c) = positions[8 * k];
        let idx = r as usize * cols + c as usize;
        matrix[idx] ^= 1; // wrong hard bit
        uncertain[idx] = 1; // …but the sampler saw it tie
    }

    // 12 > t = 9: error-only decoding must fail.
    assert!(decode_upright(&matrix, rows, cols).is_none(), "12 errors > t=9");
    // With the same 12 codewords as erasures (e + 2t = 12 <= nn) it reads.
    let decoded = decode_upright_with_erasures(&matrix, rows, cols, &uncertain)
        .expect("erasure decode");
    assert_eq!(decoded.payload.as_deref(), Some(INVOICE));
    assert!(nn >= 12, "nn {nn}");

    // Mixed: 8 erasures + 2 silent errors = 8 + 2·2 = 12 <= 18.
    let mut mixed = matrix.clone();
    let mut unc = vec![0u8; rows * cols];
    for &k in &damaged_cw[..8] {
        let (r, c) = positions[8 * k];
        unc[r as usize * cols + c as usize] = 1;
    }
    for &k in &damaged_cw[8..10] {
        let (r, c) = positions[8 * k + 1];
        mixed[r as usize * cols + c as usize] ^= 1;
        unc[r as usize * cols + c as usize] = 0;
    }
    let decoded = decode_upright_with_erasures(&mixed, rows, cols, &unc)
        .expect("2 unknown errors + 8 erasures (2t + e = 12)");
    assert_eq!(decoded.payload.as_deref(), Some(INVOICE));

    // One erasure too many for the remaining redundancy: reject, do not
    // invent a payload.
    let mut over = enc.matrix.clone();
    let mut unc = vec![0u8; rows * cols];
    for &k in &interior_codewords(rows, cols, 17) {
        let (r, c) = positions[8 * k];
        over[r as usize * cols + c as usize] ^= 1;
        unc[r as usize * cols + c as usize] = 1;
    }
    let _ = nn;
    assert!(decode_upright_with_erasures(&over, rows, cols, &unc).is_none());
}

/// Render the encoded symbol at `pitch` px/module with a quiet zone.
/// Modules listed in `smear` are drawn half dark/half light (a print
/// smear the binarizer had to guess on).
fn render(enc: &Encoded, pitch: usize, quiet: usize, smear: &[usize]) -> BinaryMap {
    let (rows, cols) = (enc.spec.rows, enc.spec.cols);
    let (w, h) = ((cols + 2 * quiet) * pitch, (rows + 2 * quiet) * pitch);
    let mut dark = vec![false; w * h];
    for r in 0..rows {
        for c in 0..cols {
            let x0 = (c + quiet) * pitch;
            let y0 = (r + quiet) * pitch;
            let idx = r * cols + c;
            for dy in 0..pitch {
                for dx in 0..pitch {
                    let on = if smear.contains(&idx) {
                        // left half dark, right half light: quadrant
                        // samples tie 2–2 whatever the true bit was
                        dx < pitch / 2
                    } else {
                        enc.matrix[idx] != 0
                    };
                    dark[(y0 + dy) * w + x0 + dx] = on;
                }
            }
        }
    }
    BinaryMap { w, h, dark }
}

#[test]
fn locator_reads_a_smeared_invoice_through_erasure_sampling() {
    let enc = encode_as(INVOICE, 12, 36).expect("encode");
    let (rows, cols) = (enc.spec.rows, enc.spec.cols);
    let positions = placement::stream_positions(enc.spec);
    let damaged_cw = interior_codewords(rows, cols, 12);
    // Smear one module per damaged codeword (alternate bits so the whole
    // codeword is corrupt, not just its first bit).
    let smear: Vec<usize> = damaged_cw
        .iter()
        .enumerate()
        .map(|(j, &k)| {
            let (r, c) = positions[8 * k + (j % 8)];
            r as usize * cols + c as usize
        })
        .collect();
    for pitch in [4usize, 6, 8] {
        let map = render(&enc, pitch, 5, &smear);
        let hits = locate_binary(&map);
        assert!(
            hits.iter()
                .any(|l| l.decoded.payload.as_deref() == Some(INVOICE)),
            "smeared invoice not located at pitch {pitch} ({} hits)",
            hits.len()
        );
    }
}
