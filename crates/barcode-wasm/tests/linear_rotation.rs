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
        ("upca", barcode_fixtures::encode_upca("036000291452", module, height, quiet), Tiers { upca: true, ..Default::default() }, "036000291452"),
        ("itf", barcode_fixtures::encode_itf("12345670", module, 2.5, height), Tiers { itf: true, ..Default::default() }, "12345670"),
        ("code39", barcode_fixtures::encode_code39("ABC-123", module, 3.0, height), Tiers { code39: true, ..Default::default() }, "ABC-123"),
        ("codabar", barcode_fixtures::encode_codabar("40156", module, 2.5, height), Tiers { codabar: true, ..Default::default() }, "40156"),
    ];
    for (name, c, tiers, want) in cases {
        assert_reads(name, &c, tiers, want);
    }
}

/// Stride-2 live scanning must still confirm a 2-row-tall symbol placed at
/// any row parity (even or odd) via immediate adjacent-row corroboration.
#[test]
fn live_strided_scan_confirms_two_row_thin_symbol() {
    let sym = barcode_fixtures::encode_code128("THIN-2PX", 3, 2);
    let w = sym.width as usize;
    let h = 128usize;
    let bpr = w.div_ceil(8).div_ceil(4) * 4;
    for y_off in [17usize, 18, 63, 64, 110, 111] {
        let mut data = vec![0u8; bpr * h];
        for r in 0..2 {
            data[(y_off + r) * bpr..(y_off + r + 1) * bpr]
                .copy_from_slice(&sym.bits()[r * sym.bytes_per_row..(r + 1) * sym.bytes_per_row]);
        }
        let mut sc = BarcodeScanner::new();
        let hits = sc.decode_live_core(w, h, &data, Tiers { code128: true, ..Default::default() });
        assert_eq!(
            hits.first().map(|r| r.text.as_str()),
            Some("THIN-2PX"),
            "y_off={y_off}"
        );
    }
}
