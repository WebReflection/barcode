//! Erasure-aware RS gate (roadmap item 6): the reader knows exactly which
//! cells it could not decode, so those become erasures (one slot each)
//! instead of errors (two slots each). With five damaged data codewords
//! and EC level 2 (k = 8) the error-only pass is already over capacity
//! (3); the erasure pass must recover the payload.

use barcode_pdf417::encode::{encode_full, pattern_of};
use barcode_pdf417::rs;
use barcode_pdf417::tables::cluster_of_row;
use barcode_pdf417::{decode_exact, stream};

const PAYLOAD: &[u8] = b"ERASURE-RS-TEST";
const DAMAGED_CELLS: usize = 5;

/// A run pattern that no cluster table contains (element 7 never occurs),
/// but still totals 17 modules and starts dark / ends light, so the cell
/// boundaries and the row's run structure stay intact.
const INVALID: [u8; 8] = [7, 1, 1, 1, 1, 1, 1, 4];

fn write_cell(matrix: &mut [u8], w: usize, band: usize, row: usize, col: usize) {
    let x0 = 34 + 17 * col; // start (17) + left indicator (17)
    let y0 = row * band;
    for dy in 0..band {
        let mut x = x0;
        let mut ink = true;
        for &len in &INVALID {
            for _ in 0..len {
                matrix[(y0 + dy) * w + x] = ink as u8;
                x += 1;
            }
            ink = !ink;
        }
    }
}

/// Overwrite one data cell with a *legal* pattern for a different value:
/// the reader decodes it as a wrong codeword (an unknown error), unlike
/// the illegal-pattern cells above.
fn corrupt_cell(matrix: &mut [u8], w: usize, band: usize, row: usize, col: usize, value: u16) {
    let runs = pattern_of(cluster_of_row(row + 1), value);
    let x0 = 34 + 17 * col;
    let y0 = row * band;
    for dy in 0..band {
        let mut x = x0;
        let mut ink = true;
        for &len in runs {
            for _ in 0..len {
                matrix[(y0 + dy) * w + x] = ink as u8;
                x += 1;
            }
            ink = !ink;
        }
    }
}

#[test]
fn mixed_unknown_errors_and_erasures_decode() {
    let sym = encode_full(PAYLOAD, 2).expect("encode");
    let band = sym.height / sym.rows;
    let raw = stream::read_exact(&sym.matrix, sym.width, sym.height).expect("clean stream");
    let cols = sym.cols;

    let mut damaged = sym.matrix.clone();
    // 3 known erasures (illegal pattern: the reader sees placeholders)
    let erased = [3usize, 6, 9];
    for &idx in &erased {
        write_cell(&mut damaged, sym.width, band, idx / cols, idx % cols);
    }
    // 2 silent unknown errors: legal patterns for a DIFFERENT value
    let silent = [1usize, 12];
    for &idx in &silent {
        let value = (raw[idx] + 100) % 929;
        corrupt_cell(&mut damaged, sym.width, band, idx / cols, idx % cols, value);
    }

    // reader reports exactly the three placeholders, not the silent errors
    let (_, placeholders) =
        stream::read_exact_tolerant_with_stats(&damaged, sym.width, sym.height).unwrap();
    assert_eq!(placeholders.len(), erased.len());
    assert_eq!(decode_exact(&damaged, sym.width, sym.height), Some(PAYLOAD.to_vec()));
}

#[test]
fn five_damaged_cells_decode_through_known_erasures() {
    let sym = encode_full(PAYLOAD, 2).expect("encode");
    let band = sym.height / sym.rows;
    assert!(sym.rows * sym.cols > DAMAGED_CELLS, "symbol too small");

    let mut damaged = sym.matrix.clone();
    // stream index = row * cols + col; index 0 is the length descriptor
    for idx in 1..=DAMAGED_CELLS {
        write_cell(&mut damaged, sym.width, band, idx / sym.cols, idx % sym.cols);
    }

    // The tolerant reader marks exactly those cells.
    let (raw, erasures) =
        stream::read_exact_tolerant_with_stats(&damaged, sym.width, sym.height).unwrap();
    let desc = raw[0] as usize;
    let k = raw.len() - desc;
    assert_eq!(erasures.len(), DAMAGED_CELLS);
    assert!(
        rs::rs_correct(&raw, k).is_none(),
        "5 unknown errors must exceed the error-only capacity"
    );
    assert!(
        rs::rs_correct_erasures(&raw, k, &erasures).is_some(),
        "5 known erasures must be within the parity budget"
    );

    // End-to-end: the decoder uses the erasure path and returns the payload.
    assert_eq!(decode_exact(&damaged, sym.width, sym.height), Some(PAYLOAD.to_vec()));
}

#[test]
fn too_many_erasures_still_reject() {
    let sym = encode_full(PAYLOAD, 2).expect("encode");
    let band = sym.height / sym.rows;
    let mut damaged = sym.matrix.clone();
    // k = 8 parity codewords: nine known erasures are beyond capacity.
    for idx in 1..=9 {
        write_cell(&mut damaged, sym.width, band, idx / sym.cols, idx % sym.cols);
    }
    assert_eq!(decode_exact(&damaged, sym.width, sym.height), None);
}
