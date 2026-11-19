//! Fast locator regression for downscaled camera masks. `locate_binary_fast`
//! derives the candidate spec from the finder legs; a 12x36 and a 16x48 are
//! both 3:1, so equal leg-pitch ratios cannot separate them. The finder leg
//! is exactly one module thick, and erosion shortens leg LENGTHS without
//! thinning the stroke, so the spec whose implied pitch agrees with the
//! measured thickness must be tried first. Otherwise 12x36 burns the
//! per-corner budget at a wrong pitch and the 16x48 symbol is never reached.
//!
//! The field case is a portrait phone frame: the scanner sees a 1600x2112
//! mask, so the eroded 16x48 regression places the symbol in a portrait
//! canvas rather than a tight square crop.

use barcode_dm::encode::encode_as;
use barcode_dm::locator::{locate_binary_fast_budget, BinaryMap};

const BUDGET: u64 = 6_000_000;
const QUIET: usize = 4;

/// Place a symbol at (x0, y0) in a canvas of `width` x `height` mask pixels.
fn place(
    width: usize,
    height: usize,
    matrix: &[u8],
    rows: usize,
    cols: usize,
    pitch: usize,
    x0: usize,
    y0: usize,
) -> BinaryMap {
    let mut dark = vec![false; width * height];
    for r in 0..rows {
        for c in 0..cols {
            if matrix[r * cols + c] == 0 {
                continue;
            }
            for y in 0..pitch {
                for x in 0..pitch {
                    dark[(y0 + r * pitch + y) * width + x0 + c * pitch + x] = true;
                }
            }
        }
    }
    BinaryMap { w: width, h: height, dark }
}

fn tight(matrix: &[u8], rows: usize, cols: usize, pitch: usize) -> BinaryMap {
    let (w, h) = ((cols + 2 * QUIET) * pitch, (rows + 2 * QUIET) * pitch);
    place(w, h, matrix, rows, cols, pitch, QUIET * pitch, QUIET * pitch)
}

fn located(map: &BinaryMap, text: &str) -> bool {
    locate_binary_fast_budget(map, BUDGET)
        .iter()
        .any(|l| l.decoded.payload.as_deref() == Some(text.as_bytes()))
}

#[test]
fn fast_locates_every_dmre_size() {
    for (rows, cols) in [(8, 18), (8, 32), (12, 26), (12, 36), (16, 36), (16, 48)] {
        let text = "A";
        let enc = encode_as(text.as_bytes(), rows, cols).unwrap();
        for pitch in [6usize, 8, 10] {
            let map = tight(&enc.matrix, rows, cols, pitch);
            assert!(located(&map, text), "{rows}x{cols}: not located at pitch {pitch}");
        }
    }
}

#[test]
fn fast_locates_eroded_16x48_in_portrait() {
    let text = "A";
    let enc = encode_as(text.as_bytes(), 16, 48).unwrap();
    for pitch in [8usize, 9, 10] {
        // The scanner's real mask is portrait (1600x2112); the symbol sits
        // in the middle of the frame, not against the origin.
        let x0 = 300;
        let y0 = 700;
        let mut map = place(1080, 1920, &enc.matrix, 16, 48, pitch, x0, y0);
        // Erode ~0.75 module off the left end of the solid bottom leg: the
        // field photo's horizontal finder leg measured ~10% short, which
        // biased the old average leg pitch below the true pitch.
        let y = y0 + (16 - 1) * pitch;
        for x in x0..x0 + (pitch * 3) / 4 {
            for yy in y..y + pitch {
                map.dark[yy * map.w + x] = false;
            }
        }
        assert!(located(&map, text), "eroded 16x48: not located at pitch {pitch}");
    }
}
