//! Rotation bank gate (roadmap item 4): DataMatrix, Aztec and PDF417 must
//! read at every cardinal AND diagonal orientation, on both budget laws.
//!
//! The fine tilt ladder only covers ±12°; 45/135/225/315° are handled by
//! the dedicated ±45° diagonal pass (a resample + the axis-aligned
//! locators, both signs because a 45°-resampled grid landing back on
//! 0/180° can lose the Aztec bullseye). Symbols are rendered on a square
//! canvas with room for the rotated bounding box, exactly like a camera
//! frame.

use barcode_core::BitImage;
use barcode_fixtures::Canvas;
use barcode_wasm::{BarcodeScanner, Tiers};

const ANGLES: [f64; 7] = [0.0, 45.0, 90.0, 135.0, 180.0, 270.0, 315.0];

/// Render a module matrix at `module` px per module, centred in a square
/// canvas big enough for the 45° rotated symbol.
fn render(matrix: &[u8], rows: usize, cols: usize, module: u32, margin: u32) -> Canvas {
    let side = ((rows.max(cols) as f64 * 1.45).ceil() as u32 + 2 * margin) * module;
    let mut c = Canvas::new(side, side);
    let ox = (side / module - cols as u32) / 2;
    let oy = (side / module - rows as u32) / 2;
    for y in 0..rows {
        for x in 0..cols {
            if matrix[y * cols + x] == 1 {
                c.fill((ox + x as u32) * module, (oy + y as u32) * module, module, module);
            }
        }
    }
    c
}

fn assert_bank(name: &str, canvas: &Canvas, tiers: Tiers, want: &str) {
    let img = BitImage::new(canvas.width, canvas.height, canvas.bits());
    for still in [true, false] {
        for deg in ANGLES {
            let rot = img.rotated(deg);
            let mut sc = BarcodeScanner::new();
            let hits = if still {
                sc.decode_still_core(
                    canvas.width as usize,
                    canvas.height as usize,
                    &rot,
                    tiers,
                    4000,
                )
            } else {
                sc.decode_live_core(canvas.width as usize, canvas.height as usize, &rot, tiers)
            };
            let hit = hits.iter().find(|h| h.text == want);
            let law = if still { "still" } else { "live" };
            let hit = hit.unwrap_or_else(|| {
                panic!("{name} {law} {deg}°: no hit (got {:?})", hits.iter().map(|h| &h.text).collect::<Vec<_>>())
            });
            for p in &hit.position {
                assert!(
                    p.x >= 0.0 && p.y >= 0.0 && p.x <= canvas.width as f32 && p.y <= canvas.height as f32,
                    "{name} {law} {deg}°: quad ({}, {}) outside {}x{}",
                    p.x,
                    p.y,
                    canvas.width,
                    canvas.height
                );
            }
        }
    }
}

#[test]
fn datamatrix_reads_every_cardinal_and_diagonal_rotation() {
    let sym = barcode_dm::encode::encode(b"ROT-DM-TEST").unwrap();
    let c = render(&sym.matrix, sym.spec.rows, sym.spec.cols, 5, 4);
    assert_bank("datamatrix", &c, Tiers { datamatrix: true, ..Default::default() }, "ROT-DM-TEST");
}

#[test]
fn aztec_reads_every_cardinal_and_diagonal_rotation() {
    let (spec, matrix) = barcode_aztec::encode::encode(b"ROT-AZTEC-TEST").unwrap();
    let c = render(&matrix, spec.rows as usize, spec.cols as usize, 5, 4);
    assert_bank("aztec", &c, Tiers { aztec: true, ..Default::default() }, "ROT-AZTEC-TEST");
}

#[test]
fn pdf417_reads_every_cardinal_and_diagonal_rotation() {
    let sym = barcode_pdf417::encode::encode_full(b"ROT-PDF417-TEST", 2).unwrap();
    let c = render(&sym.matrix, sym.height, sym.width, 5, 4);
    assert_bank("pdf417", &c, Tiers { pdf417: true, ..Default::default() }, "ROT-PDF417-TEST");
}
