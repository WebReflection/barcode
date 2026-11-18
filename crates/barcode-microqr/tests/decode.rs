//! Decoder round-trips: encode → canvas → decode, across versions,
//! levels, modes, lengths and mask classes. Every case uses a frame
//! that the generator verified against the oracle.

use barcode_core::BitImage;
use barcode_fixtures::Canvas;
use barcode_microqr::{decode, encode_bytes, Level, Mode, Symbol, Version};

fn render_canvas(sym: &Symbol, m: u32, quiet: u32) -> Canvas {
    let dim = (sym.size as u32 + 2 * quiet) * m;
    let mut c = Canvas::new(dim, dim);
    for y in 0..sym.size {
        for x in 0..sym.size {
            if sym.cells[y * sym.size + x] == 1 {
                c.fill((quiet + x as u32) * m, (quiet + y as u32) * m, m, m);
            }
        }
    }
    c
}

fn roundtrip(payload: &[u8], version: Version, level: Level, mode: Option<Mode>, class: usize) {
    let sym = encode_bytes(payload, version, level, mode, Some(class))
        .unwrap_or_else(|| panic!("encode failed: {payload:?}"));
    for m in [4u32, 6] {
        let canvas = render_canvas(&sym, m, 4);
        let img = BitImage::new(canvas.width, canvas.height, canvas.bits());
        let results = decode(&img);
        assert_eq!(results.len(), 1, "no decode: {payload:?} m={m}");
        assert_eq!(results[0].symbology, barcode_core::Symbology::QRCodeMicro);
        assert_eq!(
            results[0].text,
            barcode_core::payload_text(payload),
            "payload mismatch: {payload:?} m={m}"
        );
    }
}

#[test]
fn numeric_roundtrips() {
    roundtrip(b"5", Version::M1, Level::L, None, 0);
    roundtrip(b"42", Version::M1, Level::L, None, 2);
    roundtrip(b"12345", Version::M1, Level::L, None, 4);
    roundtrip(b"0", Version::M2, Level::L, None, 0);
    roundtrip(b"1234567890", Version::M2, Level::L, None, 3);
    roundtrip(b"12345678", Version::M2, Level::M, None, 1);
    roundtrip(b"7", Version::M3, Level::L, None, 0);
    roundtrip(b"12345678901234567890123", Version::M3, Level::L, None, 2);
    roundtrip(b"123456789012345678", Version::M3, Level::M, None, 0);
}

#[test]
fn alnum_roundtrips() {
    roundtrip(b"A1", Version::M2, Level::L, Some(Mode::Alnum), 0);
    roundtrip(b"HI-9$", Version::M2, Level::L, Some(Mode::Alnum), 2);
    roundtrip(b"A B", Version::M3, Level::L, Some(Mode::Alnum), 0);
    roundtrip(b"HELLO-WORLD/26", Version::M3, Level::L, Some(Mode::Alnum), 3);
    roundtrip(b"X.Y:Z", Version::M2, Level::M, Some(Mode::Alnum), 0);
}

#[test]
fn byte_roundtrips() {
    // no 2+ digit runs (oracle would segment those)
    roundtrip(b"\x00\x01", Version::M3, Level::L, Some(Mode::Byte), 0);
    roundtrip(b"aA\xfe", Version::M3, Level::L, Some(Mode::Byte), 1);
    roundtrip(b"\xff\x00\x7f", Version::M3, Level::M, Some(Mode::Byte), 0);
}

#[test]
fn no_false_positive_on_blank() {
    let c = Canvas::new(200, 200);
    let img = BitImage::new(c.width, c.height, c.bits());
    assert!(decode(&img).is_empty());
}

#[test]
fn offset_still_decodes() {
    // symbol drawn off-origin (uneven quiet zone)
    let sym = encode_bytes(b"12345", Version::M1, Level::L, None, Some(0)).unwrap();
    let m = 4u32;
    let mut c = Canvas::new((11 + 10 + 11) * m, (11 + 10 + 11) * m);
    let (ox, oy) = (10 * m, 10 * m);
    for y in 0..sym.size {
        for x in 0..sym.size {
            if sym.cells[y * sym.size + x] == 1 {
                c.fill(ox + x as u32 * m, oy + y as u32 * m, m, m);
            }
        }
    }
    let img = BitImage::new(c.width, c.height, c.bits());
    let results = decode(&img);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "12345");
}

/// Rotate a packed canvas about its centre (nearest sample, white
/// outside) — the synthetic stand-in for a wrist roll.
fn rotated(src: &Canvas, deg: f32) -> Canvas {
    let (w, h) = (src.width, src.height);
    let rad = deg.to_radians();
    let (sin, cos) = (rad.sin(), rad.cos());
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let ink = |x: u32, y: u32| -> bool {
        (src.bits()[y as usize * src.bytes_per_row + (x >> 3) as usize] >> (x & 7)) & 1 == 1
    };
    let mut out = Canvas::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let sx = (cos * dx - sin * dy + cx).round() as i64;
            let sy = (sin * dx + cos * dy + cy).round() as i64;
            if sx >= 0 && sy >= 0 && sx < i64::from(w) && sy < i64::from(h) && ink(sx as u32, sy as u32) {
                out.fill(x, y, 1, 1);
            }
        }
    }
    out
}

#[test]
fn tilted_symbols_still_decode() {
    // A rolled symbol is the phone's normal viewing condition. The old
    // axis-aligned sampler drifted a whole module across an 11-module
    // symbol at 4° (pitch 4), so ±4/8° missed even though the symbol was
    // perfectly legible. Decoding now follows the symbol's own axes,
    // seeded by the finder's centre block.
    for (payload, version) in [
        (b"12345".as_slice(), Version::M1),
        (b"HELLO-WORLD".as_slice(), Version::M3),
    ] {
        for m in [4u32, 6] {
            let sym = encode_bytes(payload, version, Level::L, None, Some(0))
                .unwrap_or_else(|| panic!("encode failed: {payload:?}"));
            let flat = render_canvas(&sym, m, 6);
            for deg in [-8.0f32, -4.0, 4.0, 8.0] {
                let c = rotated(&flat, deg);
                let img = BitImage::new(c.width, c.height, c.bits());
                let results = decode(&img);
                assert_eq!(
                    results.first().map(|r| r.text.clone()),
                    Some(barcode_core::payload_text(payload)),
                    "tilted {deg}° at m={m} failed: {payload:?}"
                );
            }
        }
    }
}
