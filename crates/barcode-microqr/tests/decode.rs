//! Decoder round-trips: encode → canvas → decode, across versions,
//! levels, modes, lengths, masks, cardinal rotations, tilts, and
//! Reed-Solomon error correction.

use barcode_core::BitImage;
use barcode_fixtures::Canvas;
use barcode_microqr::{decode, decoder::decode_grid, encode_bytes, Level, Mode, Symbol, Version};

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

fn roundtrip(payload: &[u8], version: Version, level: Level, mode: Option<Mode>, mask: usize) {
    let sym = encode_bytes(payload, version, level, mode, Some(mask))
        .unwrap_or_else(|| panic!("encode failed: {payload:?}"));
    for m in [4u32, 6] {
        let canvas = render_canvas(&sym, m, 4);
        let img = BitImage::new(canvas.width, canvas.height, canvas.bits());
        let results = decode(&img);
        assert_eq!(results.len(), 1, "no decode: {payload:?} m={m}");
        assert_eq!(results[0].symbology, barcode_core::Symbology::QRCodeMicro);
        assert_eq!(results[0].raw, payload, "raw mismatch: {payload:?} m={m}");
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
    roundtrip(b"12345", Version::M1, Level::L, None, 3);
    roundtrip(b"0", Version::M2, Level::L, None, 0);
    roundtrip(b"1234567890", Version::M2, Level::L, None, 3);
    roundtrip(b"12345678", Version::M2, Level::M, None, 1);
    roundtrip(b"7", Version::M3, Level::L, None, 0);
    roundtrip(b"12345678901234567890123", Version::M3, Level::L, None, 2);
    roundtrip(b"123456789012345678", Version::M3, Level::M, None, 0);
    roundtrip(
        b"12345678901234567890123456789012345",
        Version::M4,
        Level::L,
        None,
        1,
    );
    roundtrip(
        b"123456789012345678901234567890",
        Version::M4,
        Level::M,
        None,
        2,
    );
    roundtrip(b"123456789012345678901", Version::M4, Level::Q, None, 3);
}

#[test]
fn alnum_roundtrips() {
    roundtrip(b"A1", Version::M2, Level::L, Some(Mode::Alnum), 0);
    roundtrip(b"HI-9$", Version::M2, Level::L, Some(Mode::Alnum), 2);
    roundtrip(b"A B", Version::M3, Level::L, Some(Mode::Alnum), 0);
    roundtrip(
        b"HELLO-WORLD/26",
        Version::M3,
        Level::L,
        Some(Mode::Alnum),
        3,
    );
    roundtrip(b"X.Y:Z", Version::M2, Level::M, Some(Mode::Alnum), 0);
    roundtrip(
        b"MICROQR-M4-ALNUM-TEST",
        Version::M4,
        Level::L,
        Some(Mode::Alnum),
        1,
    );
    roundtrip(
        b"M4-Q-ALNUM-13",
        Version::M4,
        Level::Q,
        Some(Mode::Alnum),
        2,
    );
}

#[test]
fn byte_roundtrips() {
    roundtrip(b"\x00\x01", Version::M3, Level::L, Some(Mode::Byte), 0);
    roundtrip(b"aA\xfe", Version::M3, Level::L, Some(Mode::Byte), 1);
    roundtrip(b"\xff\x00\x7f", Version::M3, Level::M, Some(Mode::Byte), 0);
    roundtrip(b"HELLO", Version::M3, Level::L, Some(Mode::Byte), 2);
    roundtrip(
        b"15-byte-payload",
        Version::M4,
        Level::L,
        Some(Mode::Byte),
        3,
    );
    roundtrip(b"13-byte-m4-m!", Version::M4, Level::M, Some(Mode::Byte), 0);
    roundtrip(b"9-byte-q!", Version::M4, Level::Q, Some(Mode::Byte), 1);
}

#[test]
fn reed_solomon_error_correction_and_m1_detection() {
    // M1 is error-detection only: a flipped data module must be rejected, never misdecoded.
    let m1 = encode_bytes(b"12345", Version::M1, Level::L, None, None).unwrap();
    let mut bad_m1 = m1.cells.clone();
    bad_m1[10 * 11 + 10] ^= 1;
    assert_eq!(decode_grid(&bad_m1, 11), None);

    // M2-L (t=2): corrupt 2 data modules in distinct codewords + 2 format bits -> still decodes!
    let m2 = encode_bytes(b"HELLO", Version::M2, Level::L, None, None).unwrap();
    let mut dmg_m2 = m2.cells.clone();
    dmg_m2[12 * 13 + 12] ^= 1;
    dmg_m2[1 * 13 + 10] ^= 1;
    // Also flip 2 format strip modules (BCH corrects up to 3).
    dmg_m2[8 * 13 + 2] ^= 1;
    dmg_m2[4 * 13 + 8] ^= 1;
    assert_eq!(
        decode_grid(&dmg_m2, 13).as_deref(),
        Some(b"HELLO".as_slice())
    );

    // M4-Q (t=7): corrupt 5 spread modules across the symbol -> RS recovers cleanly!
    let m4 = encode_bytes(b"M4Q-RS-OK", Version::M4, Level::Q, Some(Mode::Byte), None).unwrap();
    let mut dmg_m4 = m4.cells.clone();
    for (x, y) in [(16, 16), (14, 10), (12, 5), (10, 14), (4, 12)] {
        dmg_m4[y * 17 + x] ^= 1;
    }
    assert_eq!(
        decode_grid(&dmg_m4, 17).as_deref(),
        Some(b"M4Q-RS-OK".as_slice())
    );
}

#[test]
fn cardinal_rotations_decode() {
    for (payload, version, level) in [
        (b"12345".as_slice(), Version::M1, Level::L),
        (b"HELLO".as_slice(), Version::M2, Level::L),
        (b"MICROQR-M3".as_slice(), Version::M3, Level::L),
        (b"M4-ROTATE-OK".as_slice(), Version::M4, Level::M),
    ] {
        let sym = encode_bytes(payload, version, level, None, None).unwrap();
        let c0 = render_canvas(&sym, 5, 6);
        let img0 = BitImage::new(c0.width, c0.height, c0.bits());
        let rot90 = img0.rot90_cw();
        let img90 = BitImage::new(c0.height, c0.width, &rot90);
        let rot180 = img0.rot180();
        let img180 = BitImage::new(c0.width, c0.height, &rot180);
        let rot270 = img90.rot180();
        let img270 = BitImage::new(c0.height, c0.width, &rot270);
        for (deg, img) in [(0, &img0), (90, &img90), (180, &img180), (270, &img270)] {
            let hits = decode(img);
            assert_eq!(
                hits.first().map(|r| r.raw.as_slice()),
                Some(payload),
                "{version:?} {deg}° failed"
            );
        }
    }
}

#[test]
fn no_false_positive_on_blank_or_random_grids() {
    let c = Canvas::new(200, 200);
    let img = BitImage::new(c.width, c.height, c.bits());
    assert!(decode(&img).is_empty());

    // Deterministic xorshift random grids across all 4 Micro QR sizes.
    let mut s = 0x1234_5678u32;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    for size in [11usize, 13, 15, 17] {
        let mut grid = vec![0u8; size * size];
        for _ in 0..2000 {
            for cell in &mut grid {
                *cell = (next() & 1) as u8;
            }
            assert_eq!(
                decode_grid(&grid, size),
                None,
                "random false positive at size {size}"
            );
        }
    }
}

#[test]
fn offset_still_decodes() {
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
            if sx >= 0
                && sy >= 0
                && sx < i64::from(w)
                && sy < i64::from(h)
                && ink(sx as u32, sy as u32)
            {
                out.fill(x, y, 1, 1);
            }
        }
    }
    out
}

#[test]
fn tilted_symbols_still_decode() {
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
