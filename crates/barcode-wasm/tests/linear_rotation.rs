//! Quarter-turn gate for the linear tier: a 1D barcode printed sideways
//! (loyalty cards, receipts held in portrait) is invisible to a row
//! scanner. The tier now tries 90° and 270° after the upright and mirrored
//! passes, mapping the hit quads back into the original frame.
//!
//! Field anchor: a loyalty-card Code 128 read zero hits upright and
//! decoded at 90° at native resolution (the payload stays in the
//! gitignored field fixture, not in this gate).

use barcode_core::BitImage;
use barcode_fixtures::Canvas;
use barcode_wasm::{BarcodeScanner, Tiers};

/// Exact quarter/half turn of a packed bitmap.
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

fn assert_reads(name: &str, c: &Canvas, tiers: Tiers, want: &str) {
    for deg in [0usize, 90, 180, 270] {
        let (packed, w, h) = rotate(c.bits(), c.width as usize, c.height as usize, deg);
        let mut sc = BarcodeScanner::new();
        let hits = sc.decode_live_core(w, h, &packed, tiers);
        let hit = hits.iter().find(|h| h.text == want);
        let hit = hit.unwrap_or_else(|| {
            panic!(
                "{name} {deg}°: no hit (got {:?})",
                hits.iter().map(|h| &h.text).collect::<Vec<_>>()
            )
        });
        for p in &hit.position {
            assert!(
                p.x >= 0.0 && p.y >= 0.0 && p.x <= w as f32 && p.y <= h as f32,
                "{name} {deg}°: quad ({}, {}) outside {w}x{h}",
                p.x,
                p.y
            );
        }
    }
}

#[test]
fn linear_barcodes_read_at_every_quarter_turn() {
    let module = 4;
    let height = 120;
    let quiet = 10 * module;

    let cases: Vec<(&str, Canvas, Tiers, &str)> = vec![
        ("code128", barcode_fixtures::encode_code128("++123456789", module, height), Tiers { code128: true, ..Default::default() }, "++123456789"),
        ("ean13", barcode_fixtures::encode_ean13("4006381333931", module, height, quiet), Tiers { ean13: true, ..Default::default() }, "4006381333931"),
        ("itf", barcode_fixtures::encode_itf("12345670", module, 2.5, height), Tiers { itf: true, ..Default::default() }, "12345670"),
        ("code39", barcode_fixtures::encode_code39("ABC-123", module, 3.0, height), Tiers { code39: true, ..Default::default() }, "ABC-123"),
        ("codabar", barcode_fixtures::encode_codabar("40156", module, 2.5, height), Tiers { codabar: true, ..Default::default() }, "40156"),
    ];
    for (name, c, tiers, want) in cases {
        assert_reads(name, &c, tiers, want);
    }
}
