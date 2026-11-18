//! Cardinal-rotation gate for the PDF417 tier: a code can be presented at
//! 0/90/180/270°, and the tier undoes the quarter turn before detection.

use barcode_core::BitImage;
use barcode_wasm::decode_pdf417_mask;

const PAYLOAD: &str = "ROTATION-TEST-90-180-270";

/// Render the encoder's symbol at `module` px per module with a quiet zone.
fn render(module: usize, quiet: usize) -> (Vec<u8>, usize, usize) {
    let sym = barcode_pdf417::encode::encode_full(PAYLOAD.as_bytes(), 2).unwrap();
    let (mw, mh) = (sym.width, sym.height);
    let (cw, ch) = (mw * module + 2 * quiet, mh * module + 2 * quiet);
    let bpr = cw.div_ceil(8).div_ceil(4) * 4;
    let mut packed = vec![0u8; bpr * ch];
    for my in 0..mh {
        for mx in 0..mw {
            if sym.matrix[my * mw + mx] == 0 {
                continue;
            }
            for dy in 0..module {
                for dx in 0..module {
                    let x = quiet + mx * module + dx;
                    let y = quiet + my * module + dy;
                    packed[y * bpr + (x >> 3)] |= 1 << (x & 7);
                }
            }
        }
    }
    (packed, cw, ch)
}

fn rotate(packed: &[u8], w: usize, h: usize, deg: usize) -> (Vec<u8>, usize, usize) {
    if deg == 0 {
        return (packed.to_vec(), w, h);
    }
    let img = BitImage::new(w as u32, h as u32, packed);
    if deg == 180 {
        return (img.rot180(), w, h);
    }
    let (dw, dh) = (h, w);
    let bpr = dw.div_ceil(8).div_ceil(4) * 4;
    let mut out = vec![0u8; bpr * dh];
    for y in 0..h {
        for x in 0..w {
            if !img.ink(x as u32, y as u32) {
                continue;
            }
            let (dx, dy) = if deg == 90 { (h - 1 - y, x) } else { (y, w - 1 - x) };
            out[dy * bpr + (dx >> 3)] |= 1 << (dx & 7);
        }
    }
    (out, dw, dh)
}

#[test]
fn pdf417_reads_every_cardinal_rotation() {
    let (packed, w, h) = render(5, 8);
    for deg in [0usize, 90, 180, 270] {
        let (rot, rw, rh) = rotate(&packed, w, h, deg);
        let img = BitImage::new(rw as u32, rh as u32, &rot);
        let hits = decode_pdf417_mask(&img);
        assert_eq!(hits.len(), 1, "{deg}°: one hit");
        assert_eq!(hits[0].text, PAYLOAD, "{deg}°: payload");
        for p in &hits[0].position {
            assert!(
                p.x >= 0.0 && p.y >= 0.0 && p.x <= rw as f32 && p.y <= rh as f32,
                "{deg}°: quad ({}, {}) outside {rw}x{rh}",
                p.x,
                p.y
            );
        }
    }
}
