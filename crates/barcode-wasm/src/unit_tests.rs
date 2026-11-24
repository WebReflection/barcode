//! Native (non-WASM) unit tests for the bridge. Kept out of `lib.rs` so the
//! production surface stays readable.

#[cfg(test)]
mod upside_down_tests {
    use crate::*;
    use crate::masks::*;
    use crate::render::*;
    use barcode_fixtures::Canvas;

    /// The render-side bytes builder round-trips through the tiers for
    /// every binary-capable symbology.
    #[test]
    fn fixture_bytes_canvas_round_trips() {
        let data: Vec<u8> = vec![0x00, 0x01, 0x7F, 0x80, 0xFF, 0xFE, b'#'];
        for sym in ["qrcode", "microqr", "datamatrix", "aztec", "pdf417"] {
            let canvas =
                fixture_bytes_canvas(sym, &data, 4, EncOptions::default()).unwrap_or_else(|e| panic!("{sym}: {e:?}"));
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert_eq!(hits.len(), 1, "{sym}: expected exactly one hit, got {:?}", hits.iter().map(|h| (&h.text, h.raw.len())).collect::<Vec<_>>());
            assert_eq!(hits[0].raw, data, "{sym}: raw mismatch");
        }
    }

    /// Invalid inputs to text and binary render must return Err, never panic/trap WASM.
    #[test]
    fn render_rejects_invalid_inputs_without_panicking() {
        let def = EncOptions::default();
        let bad_text_cases: &[(&str, &str)] = &[
            ("ean13", ""),
            ("ean13", "abc"),
            ("ean13", "123456789012"),
            ("ean13", "4006381333930"), // bad check digit
            ("ean13", "400638133393a"),
            ("upca", "123"),
            ("upca", "036000291450"), // bad check digit
            ("upca", "03600029145a"),
            ("ean8", "123"),
            ("ean8", "96385070"), // bad check digit
            ("ean8", "9638507x"),
            ("upce", "123"),
            ("upce", "242100005264"), // bad number system
            ("upce", "042100005260"), // bad check digit
            ("upce", "036000291452"), // valid UPC-A but not compressible to UPC-E
            ("code39", ""),
            ("code39", "BAD@CHAR"),
            ("code39", "A*B"),
            ("code39", "café"),
            ("code32", "BAD"),
            ("code32", "A000000001"), // bad check digit
            ("itf", ""),
            ("itf", "123"), // odd digits
            ("itf", "12AB"),
            ("code128", ""),
            ("code128", "café"),
            ("code128", "hello 🚀"),
            ("code93", ""),
            ("code93", "café"),
            ("codabar", ""),
            ("codabar", "HELLO"),
            ("codabar", "12x34"),
            ("databar", "123"),
            ("databar", "123456789012a"),
            ("databarltd", "4006381333931"), // indicator >= 2
            ("databarstk", "12345678901234"), // 14 digits (> 10^13)
            ("databarstackomni", "not-digits"),
            ("databarexpanded", ""),
            ("qrcode", ""),
            ("microqr", ""),
            ("microqr", "WAY-TOO-LONG-FOR-MICRO-QR-CAPACITY-1234567890"),
            ("datamatrix", ""),
            ("aztec", ""),
            ("pdf417", ""),
            ("unknown-sym", "123"),
        ];
        for &(sym, text) in bad_text_cases {
            assert!(
                encode_text_canvas(sym, text, 4, 120, def).is_err(),
                "expected Err for text render ({sym:?}, {text:?})"
            );
        }
        // Invalid module / height bounds must also return Err, never panic.
        assert!(encode_text_canvas("code128", "OK", 0, 120, def).is_err());
        assert!(encode_text_canvas("code128", "OK", 65, 120, def).is_err());
        assert!(encode_text_canvas("code128", "OK", 4, 0, def).is_err());
        assert!(encode_text_canvas("code128", "OK", 4, 9000, def).is_err());
        // Invalid binary inputs.
        assert!(fixture_bytes_canvas("qrcode", &[], 4, def).is_err());
        assert!(fixture_bytes_canvas("qrcode", &[0u8; 4000], 4, def).is_err());
        assert!(fixture_bytes_canvas("qrcode", b"ok", 0, def).is_err());
        assert!(fixture_bytes_canvas("ean13", b"4006381333931", 4, def).is_err());
        assert!(fixture_bytes_canvas("microqr", &[0u8; 100], 4, def).is_err());
    }

    /// Binary payloads through the 2D tiers: `raw` must carry the exact
    /// bytes while `text` is the lossy display form.
    #[test]
    fn binary_payloads_survive_the_2d_tiers() {
        let data: Vec<u8> = vec![0x00, 0xFF, 0x7F, 0x80, 0xFE];

        let enc = barcode_dm::encode::encode(&data).unwrap();
        let (r, c) = (enc.spec.rows, enc.spec.cols);
        let mut canvas = Canvas::new((c as u32 + 8) * 4, (r as u32 + 8) * 4);
        for y in 0..r {
            for x in 0..c {
                if enc.matrix[y * c + x] == 1 {
                    canvas.fill((4 + x as u32) * 4, (4 + y as u32) * 4, 4, 4);
                }
            }
        }
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let hits = decode_dm_at(&img, 4.0, false, u64::MAX);
        assert_eq!(hits.len(), 1, "dm");
        assert_eq!(hits[0].raw, data, "dm raw");

        let sym = barcode_pdf417::encode::encode_full(&data, barcode_pdf417::encode::DEFAULT_LEVEL)
            .unwrap();
        let m = 4;
        let mut canvas = Canvas::new((sym.width as u32 + 8) * m, (sym.height as u32 + 8) * m);
        for y in 0..sym.height {
            for x in 0..sym.width {
                if sym.matrix[y * sym.width + x] == 1 {
                    canvas.fill((4 + x as u32) * m, (4 + y as u32) * m, m, m);
                }
            }
        }
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let hits = decode_pdf417_mask(&img);
        assert_eq!(hits.len(), 1, "pdf417");
        assert_eq!(hits[0].raw, data, "pdf417 raw");

        let (spec, matrix) = barcode_aztec::encode::encode(&data).unwrap();
        let (r, c) = (spec.rows as usize, spec.cols as usize);
        let mut canvas = Canvas::new((c as u32 + 8) * 4, (r as u32 + 8) * 4);
        for y in 0..r {
            for x in 0..c {
                if matrix[y * c + x] == 1 {
                    canvas.fill((4 + x as u32) * 4, (4 + y as u32) * 4, 4, 4);
                }
            }
        }
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let hits = decode_aztec_mask(&img);
        assert_eq!(hits.len(), 1, "aztec");
        assert_eq!(hits[0].raw, data, "aztec raw");
    }

    #[test]
    fn code128_fixture_decodes_short_payloads() {
        // ISO/IEC 15417 permits one data codeword ("A") and two ("AB").
        // The old gate required three and rejected those symbols even
        // though the check digit validated.
        for text in ["A", "AB", "1", "12"] {
            let canvas = barcode_fixtures::encode_code128(text, 4, 80);
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert!(
                hits.iter().any(|r| r.text == text),
                "short {text:?}: got {:?}",
                hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn code128_fixture_round_trips_every_control_byte() {
        // Not one GS1 payload: every C0 control plus DEL must survive the
        // fixture encoder -> scanner path, so the encoder cannot be
        // special-cased to a single string. Chunked to stay inside the
        // decoder's existing length heuristics (see the tracked issue for
        // the separate short-symbol and >30-codeword limits).
        let controls: Vec<char> = (0u8..=31).chain([127]).map(char::from).collect();
        for chunk in controls.chunks(8) {
            let mid: String = chunk.iter().collect();
            let text = format!("A{mid}Z");
            let canvas = barcode_fixtures::encode_code128(&text, 4, 80);
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert!(
                hits.iter().any(|r| r.text == text),
                "control-byte round-trip {text:?}: got {:?}",
                hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn databar_stacked_tier_upright_and_upside_down() {
        let v = 4006381333931u64;
        for &omni in &[false, true] {
            let runs = barcode_fixtures::databar_omni_runs(v);
            let canvas = barcode_fixtures::databar_stacked_canvas(&runs, 4, 2, omni);
            let want = "(01)40063813339314";

            // Upright, through the full tiered scan.
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert!(
                hits.iter()
                    .any(|r| r.symbology == barcode_core::Symbology::DataBarStacked
                        && r.text == want),
                "upright tier miss (omni={omni}): {hits:?}"
            );

            // Upside down: only the 180° pass may read it.
            let rot_bits = rot180(&canvas);
            let rot = BitImage::new(canvas.width, canvas.height, &rot_bits);
            let mut sc2 = BarcodeScanner::new();
            let hits2 = sc2.scan_tiered(&rot, &Tiers::all(), false, 0);
            assert!(
                hits2.iter().any(|r| r.text == want),
                "rot180 tier miss (omni={omni}): {hits2:?}"
            );
        }
    }

    #[test]
    fn databar_ltd_tier_upright_and_upside_down() {
        let v = 1_234_567_890_123u64;
        let runs = barcode_fixtures::databar_ltd_runs(v);
        let canvas = barcode_fixtures::databar_ltd_canvas(&runs, 4, 40, 2);
        let body = format!("{v:013}");
        let sum: usize = body
            .bytes()
            .enumerate()
            .map(|(i, b)| (b - b'0') as usize * if i % 2 == 0 { 3 } else { 1 })
            .sum();
        let want = format!("(01){body}{}", (10 - sum % 10) % 10);

        // Upright, through the full tiered scan.
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut sc = BarcodeScanner::new();
        let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
        assert!(
            hits.iter()
                .any(|r| r.symbology == barcode_core::Symbology::DataBarLimited && r.text == want),
            "upright tier miss: {hits:?}"
        );

        // Upside down: the mirrored linear pass must read it.
        let rot_bits = rot180(&canvas);
        let rot = BitImage::new(canvas.width, canvas.height, &rot_bits);
        let mut sc2 = BarcodeScanner::new();
        let hits2 = sc2.scan_tiered(&rot, &Tiers::all(), false, 0);
        assert!(
            hits2.iter().any(|r| r.text == want),
            "rot180 tier miss: {hits2:?}"
        );

        // The formats filter keeps the tier addressable.
        let mut sc3 = BarcodeScanner::new();
        let only = parse_formats(&["databarltd".into()]).unwrap();
        assert!(sc3.scan_tiered(&img, &only, false, 0).iter().any(|r| r.text == want));
        let off = parse_formats(&["ean13".into()]).unwrap();
        assert!(sc3.scan_tiered(&img, &off, false, 0).is_empty());
    }

    /// Draw a packed module row into a canvas with a quiet zone.
    fn expanded_canvas(mods: &str) -> barcode_fixtures::Canvas {
        let scale = 3;
        let margin = 6;
        let mut canvas = barcode_fixtures::Canvas::new(
            (mods.len() as u32 + 2 * margin) * scale,
            3 * scale,
        );
        for (i, ch) in mods.chars().enumerate() {
            if ch == '1' {
                canvas.fill((margin + i as u32) * scale, 0, scale, 3 * scale);
            }
        }
        canvas
    }

    #[test]
    fn databar_expanded_tier_decodes_all_methods() {
        // Real module rows: the compressed methods come from the public
        // bardecode.com examples (zint does not emit those), the method 1
        // and method 00 rows from zint.
        let cases = [
            (
                "0100",
                "10111001000001001101111111100001011100001001100010101111011100110001011110000001110010111000111011101111010111100011000111111000010101100001001111101",
                "(01)90012345678908(3103)001750",
            ),
            (
                "0111000",
                "1001000100001110110111111110000101000110111000010010111100110111110101111110000111000101100001101110001111011010111100011111100001011000011010110000111001100110001001011111111001110001101111010000101",
                "(01)90012345678908(3103)012233(11)991231",
            ),
            (
                "01100",
                "10110000010001011101111111100001010011100000101100101111001101111101011111100001110001011000011011100011110110101111000111111000010100111101110100001100011011100100010111111110011101",
                "(01)90012345678908(3922)795",
            ),
            (
                "01101",
                "1000111101010000010111111110000101110100000110010010111100110111110101111110000111000101100001101110001111011010111100011111100001011100011001101100100111110001011101011111111001110001101111001011101",
                "(01)90012345678908(3932)0401234",
            ),
            (
                "method 1 + other AIs",
                "01000011001011000110111111110000101111010000100100010000101111101110101111100000011000101100000110010111100001101100100011111100001011100011001000010100111011111101101011111111001110010111110110111101010111101111111000111100000010100011110010011101",
                "(01)40063813339314(10)ABC(21)123",
            ),
            (
                "method 00",
                "010100000101000001101111111100001010000010000100110100111110001011101011110000000010111000011000110101",
                "(21)12345",
            ),
        ];
        for (name, mods, want) in cases {
            let canvas = expanded_canvas(mods);
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert!(
                hits.iter().any(|r| {
                    r.symbology == barcode_core::Symbology::DataBarExpanded && r.text == want
                }),
                "{name}: got {:?}",
                hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>(),
            );

            // The formats filter keeps the tier addressable and excludes it.
            let only = parse_formats(&["databarexpanded".into()]).unwrap();
            assert!(sc.scan_tiered(&img, &only, false, 0).iter().any(|r| r.text == want));
            let off = parse_formats(&["ean13".into()]).unwrap();
            assert!(sc.scan_tiered(&img, &off, false, 0).is_empty());
        }
    }

    #[test]
    fn databar_expanded_encoder_roundtrips() {
        let cases = [
            "(21)12345",
            "(10)ABC123",
            "(90)abc",
            "(21)0123456789",
            "(01)90012345678908(3103)001750",
            "(01)90012345678908(3103)001234",
            "(01)90012345678908(3202)000150",
            "(01)90012345678908(3203)001234",
            "(01)90012345678908(3922)12345",
            "(01)90012345678908(3932)97812345",
            "(01)90012345678908(3103)001234(15)261231",
            "(01)90012345678908(3100)000123",
            "(01)40063813339314(10)ABC",
            "(01)40063813339314(10)ABCDEFGHIJKLMNOPQRST",
            "(01)40063813339314(3103)001750(10)ABC123",
            "(90)abcdefghijklmnopqrstuvwxyz",
        ];
        let only = parse_formats(&["databarexpanded".into()]).unwrap();
        for text in cases {
            let canvas =
                barcode_fixtures::databar_exp::encode_databarexpanded(text, 3, 40).unwrap();
            let img = BitImage::new(canvas.width, canvas.height, canvas.bits());
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &only, false, 0);
            assert!(
                hits.iter().any(|r| r.text == text),
                "{text}: got {:?}",
                hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>(),
            );
        }
    }

    fn rot180(canvas: &barcode_fixtures::Canvas) -> Vec<u8> {
        // A 180° rotation of the packed bitmap: what the camera sees when a
        // barcode is presented upside down.
        let w = canvas.width as usize;
        let h = canvas.height as usize;
        let bpr = canvas.bytes_per_row as usize;
        let src = canvas.bits();
        let mut out = vec![0u8; bpr * h];
        for y in 0..h {
            for x in 0..w {
                let v = (src[(h - 1 - y) * bpr + ((w - 1 - x) >> 3)] >> ((w - 1 - x) & 7)) & 1;
                if v == 1 {
                    out[y * bpr + (x >> 3)] |= 1 << (x & 7);
                }
            }
        }
        out
    }

    #[test]
    fn decodes_linear_barcodes_presented_upside_down() {
        // (digits, encoder, expected symbology via the decoder that hits)
        let cases: Vec<(String, barcode_fixtures::Canvas)> = vec![
            (
                "4006381333931".into(),
                barcode_fixtures::encode_ean13("4006381333931", 4, 80, 4),
            ),
            (
                "CODE 39",
                barcode_fixtures::encode_code39("CODE 39", 4, 2.0, 80),
            )
            .into_text(),
            (
                "12345670".into(),
                barcode_fixtures::encode_ean8("12345670", 4, 80, 4),
            ),
            (
                "1234567895".into(),
                barcode_fixtures::encode_itf("1234567895", 4, 2.0, 80),
            )
            .into_text(),
            (
                "9501101531000".into(),
                barcode_fixtures::encode_ean13("9501101531000", 4, 80, 4),
            ),
            (
                "Code 128!".into(),
                barcode_fixtures::encode_code128("Code 128!", 4, 80),
            ),
            (
                // GS1-128: FNC1 decodes back to GS (0x1D), so the payload
                // must survive a re-encode after a scan.
                "0112345678901231\u{1d}10ABC".into(),
                barcode_fixtures::encode_code128("0112345678901231\u{1d}10ABC", 4, 80),
            ),
            (
                "CODE 93".into(),
                barcode_fixtures::encode_code93("CODE 93", 4, 80),
            ),
            (
                // Codabar results carry the body only — sentinels are structure.
                "40156".into(),
                barcode_fixtures::encode_codabar("40156", 4, 2.5, 80),
            ),
        ];
        for (want, canvas) in cases {
            let bits = rot180(&canvas);
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            // The QR tier must stay silent on linear input...
            assert!(
                sc.qr.decode(&img).is_empty(),
                "2D tier fired on a linear canvas"
            );
            let hits = decode_linear(sc.linear_decoders(), &img, &Tiers::all(), true, true);
            assert!(
                hits.iter().any(|r| r.text == want),
                "upside-down decode of {want:?}: got {:?}",
                hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>(),
            );
            // The hit must sit inside the frame (x coordinates mirrored back).
            for r in hits.iter().filter(|r| r.text == want) {
                let xs: Vec<f32> = r.position.iter().map(|p| p.x).collect();
                assert!(xs.iter().cloned().fold(f32::MAX, f32::min) >= 0.0);
                assert!(
                    xs.iter().cloned().fold(f32::MIN, f32::max) <= img.width as f32,
                    "x out of frame: {xs:?}"
                );
            }
        }
    }

    #[test]
    fn decodes_italian_pharmacode_code32() {
        // A real box: the six Code 39 symbols VYTH1V carry the printed AIC
        // value A938261563. `code32` must relabel it; plain `code39` must
        // keep the carrier payload.
        let canvas = barcode_fixtures::encode_code39("VYTH1V", 4, 3.0, 80);
        let img = BitImage::new(canvas.width, canvas.height, canvas.bits());
        let mut sc = BarcodeScanner::new();

        let all = sc.scan_tiered(&img, &Tiers::all(), false, 0);
        let hit = all.first().expect("code32 specimen did not decode");
        assert_eq!(hit.symbology, barcode_core::Symbology::Code32, "{all:?}");
        assert_eq!(hit.text, "A938261563");

        let only32 = parse_formats(&["code32".into()]).unwrap();
        assert!(sc.scan_tiered(&img, &only32, false, 0).iter().any(|r| r.text == "A938261563"));

        let only39 = parse_formats(&["code39".into()]).unwrap();
        assert!(sc.scan_tiered(&img, &only39, false, 0).iter().any(|r| r.text == "VYTH1V"));
    }

    trait IntoText {
        fn into_text(self) -> (String, barcode_fixtures::Canvas);
    }
    impl IntoText for (&'static str, barcode_fixtures::Canvas) {
        fn into_text(self) -> (String, barcode_fixtures::Canvas) {
            (self.0.into(), self.1)
        }
    }
}

#[cfg(test)]
mod formats_tests {
    use crate::*;

    #[test]
    fn formats_filter_selects_decode_tiers() {
        let qr_canvas = {
            let data: Vec<u8> = (0..19u8).collect(); // v1-L capacity
            barcode_fixtures::encode_qr(1, 1, 2, &data, 4)
        };
        let ean_canvas = barcode_fixtures::encode_ean13("4006381333931", 4, 80, 4);

        let qr_img = BitImage::new(qr_canvas.width, qr_canvas.height, qr_canvas.bits());
        let ean_img = BitImage::new(ean_canvas.width, ean_canvas.height, ean_canvas.bits());
        let mut sc = BarcodeScanner::new();

        let count = |r: &Vec<barcode_core::DecodeResult>| r.len();

        // QR canvas, everything: decodes.
        assert_eq!(count(&sc.scan_tiered(&qr_img, &Tiers::all(), false, 0)), 1);

        // QR canvas, 1D-only formats: the QR tier is off — nothing.
        let one_d = parse_formats(&["ean13".into(), "code128".into()]).unwrap();
        assert_eq!(count(&sc.scan_tiered(&qr_img, &one_d, false, 0)), 0, "1D filter must silence the QR tier");

        // QR canvas, qrcode-only: decodes.
        let qr_only = parse_formats(&["qr".into()]).unwrap();
        assert_eq!(count(&sc.scan_tiered(&qr_img, &qr_only, false, 0)), 1, "qrcode filter must keep the QR tier");

        // EAN-13 canvas, qrcode-only: nothing (and no junk 1D votes).
        assert_eq!(count(&sc.scan_tiered(&ean_img, &qr_only, false, 0)), 0, "qrcode filter must not fire on 1D input");

        // EAN-13 canvas, ean13-only: decodes.
        let ean_only = parse_formats(&["ean13".into()]).unwrap();
        let hits = sc.scan_tiered(&ean_img, &ean_only, false, 0);
        assert_eq!(count(&hits), 1, "ean13 filter must decode EAN-13");
        assert_eq!(hits[0].text, "4006381333931");

        // Unknown names throw.
        assert!(parse_formats(&["qrcodes".into()]).is_err());
        // Alias + case/spacing normalization.
        assert!(parse_formats(&["  QR ".into()]).unwrap().qr);
        assert!(parse_formats(&["datamatrix".into()]).unwrap().datamatrix);
    }

    /// Embedded zint 12x36 canonical matrix for b"DETECTOR-TEST-123"
    /// (independent of the field fixture; col 0 + row 11 are the finder L).
    const TEST_MATRIX: [&str; 12] = [
        "#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.",
        "#.#.#.##....##..#######.####.#.#.#.#",
        "##.#...###.######.#.####...##..#.##.",
        "##.##..##.#.###.#########.##...#.###",
        "#.#..#...#..##....##..##.#..#.#.##..",
        "#.##...#.###.#...####..#..#.########",
        "##.######.#..##...#...#.....#.#...#.",
        "###.#.##......#.######....#...#..#.#",
        "##..#.#...#.##.##.##.####....##.##..",
        "#.........##..#####.#.#...#.###...##",
        "###..##.#..#####..###.####...#.#....",
        "####################################",
    ];

    #[test]
    fn datamatrix_tier_runs_2d_first_and_respects_formats() {
        // draw the 12x36 symbol upright (L bottom-left) at pitch 8 on a
        // white canvas, packed as the GPU mask
        let (w, h) = (420usize, 420usize);
        let pitch = 8usize;
        let rows = TEST_MATRIX.len();
        let cols = TEST_MATRIX[0].len();
        let bpr = w.div_ceil(8).div_ceil(4) * 4;
        let mut mask = vec![0u8; bpr * h];
        for r in 0..rows {
            for c in 0..cols {
                if TEST_MATRIX[r].as_bytes()[c] != b'#' {
                    continue;
                }
                // upright: L corner at bottom-left; module (r,c) center at
                // (x0 + (c+0.5)*pitch, y0 + (r+0.5)*pitch) with the L
                // corner at (x0, y0 + rows*pitch)
                let cx = 40 + (c * pitch) + pitch / 2;
                let cy = 40 + (r * pitch) + pitch / 2;
                for dy in 0..pitch {
                    for dx in 0..pitch {
                        let x = cx - pitch / 2 + dx;
                        let y = cy - pitch / 2 + dy;
                        mask[y * bpr + (x >> 3)] |= 1 << (x & 7);
                    }
                }
            }
        }
        let img = BitImage::new(w as u32, h as u32, &mask);
        let mut sc = BarcodeScanner::new();
        let count = |r: &Vec<barcode_core::DecodeResult>| r.len();

        // datamatrix-only filter: decodes.
        let dm_only = parse_formats(&["datamatrix".into()]).unwrap();
        let hits = sc.scan_tiered(&img, &dm_only, false, 0);
        assert_eq!(count(&hits), 1, "datamatrix filter must decode DM");
        assert_eq!(hits[0].symbology, barcode_core::Symbology::DataMatrix);
        assert_eq!(hits[0].text, "DETECTOR-TEST-123");

        // 1D-only filter: the DM tier is off - nothing.
        let one_d = parse_formats(&["ean13".into()]).unwrap();
        assert_eq!(count(&sc.scan_tiered(&img, &one_d, false, 0)), 0, "1D filter must silence the DM tier");

        // all tiers: DM hits 2D-first.
        assert_eq!(count(&sc.scan_tiered(&img, &Tiers::all(), false, 0)), 1);
    }

    /// Embedded zint 'B' vers-2 compact oracle (19x19, docs/STATUS.md;
    /// same matrix as barcode-aztec's golden test).
    const AZTEC_B: [&str; 19] = [
        "..#.#.##.##..##..#.",
        ".#.#..##..###..#.#.",
        "##.#..#.#.#.#..###.",
        "..#..####.##.####.#",
        "..####.#......#...#",
        "#...############..#",
        "#....#.......#.###.",
        "######.#####.#.###.",
        "###..#.#...#.#.##.#",
        "#...##.#.#.#.#.##..",
        "##..##.#...#.###.#.",
        "#.#.##.#####.#..#.#",
        "...#.#.......####..",
        ".##..##########....",
        "##.....#...#....#..",
        ".#...#..##..####...",
        "#.######..#.###.#.#",
        "#.##...#####..#...#",
        "..####..#.#.##..##.",
    ];

    fn draw_aztec(rows: &[&str], pitch: usize, quiet: usize) -> (Vec<u8>, u32, u32) {
        let size = rows.len();
        let dim = (size + 2 * quiet) * pitch;
        let bpr = dim.div_ceil(8).div_ceil(4) * 4;
        let mut mask = vec![0u8; bpr * dim];
        for r in 0..size {
            for c in 0..size {
                if rows[r].as_bytes()[c] != b'#' {
                    continue;
                }
                for dy in 0..pitch {
                    for dx in 0..pitch {
                        let x = (c + quiet) * pitch + dx;
                        let y = (r + quiet) * pitch + dy;
                        mask[y * bpr + (x >> 3)] |= 1 << (x & 7);
                    }
                }
            }
        }
        (mask, dim as u32, dim as u32)
    }

    #[test]
    fn aztec_tier_decodes_and_respects_formats() {
        let (mask, w, h) = draw_aztec(&AZTEC_B, 8, 4);
        let img = BitImage::new(w, h, &mask);
        let mut sc = BarcodeScanner::new();
        let count = |r: &Vec<barcode_core::DecodeResult>| r.len();

        let az_only = parse_formats(&["aztec".into()]).unwrap();
        let hits = sc.scan_tiered(&img, &az_only, false, 0);
        assert_eq!(count(&hits), 1, "aztec filter must decode Aztec");
        assert_eq!(hits[0].symbology, barcode_core::Symbology::Aztec);
        assert_eq!(hits[0].text, "B");
        // quad: TL corner module center ≈ half a module into the quiet zone.
        assert!((hits[0].position[0].x - 36.0).abs() < 1.0, "{:?}", hits[0].position);

        // 1D-only filter: the Aztec tier is off - nothing.
        let one_d = parse_formats(&["ean13".into()]).unwrap();
        assert_eq!(count(&sc.scan_tiered(&img, &one_d, false, 0)), 0, "1D filter must silence the Aztec tier");

        // All tiers: still exactly one hit (QR/DM tiers find nothing here).
        assert_eq!(count(&sc.scan_tiered(&img, &Tiers::all(), false, 0)), 1);

        // 90°-rotated symbol still decodes.
        let size = AZTEC_B.len();
        let mut rot = vec![vec![0u8; size]; size];
        for r in 0..size {
            for c in 0..size {
                rot[c][size - 1 - r] = (AZTEC_B[r].as_bytes()[c] == b'#') as u8;
            }
        }
        let rot_rows: Vec<String> = rot.iter().map(|r| r.iter().map(|&b| if b == 1 { '#' } else { '.' }).collect()).collect();
        let rot_refs: Vec<&str> = rot_rows.iter().map(|s| s.as_str()).collect();
        let (rmask, rw, rh) = draw_aztec(&rot_refs, 8, 4);
        let rimg = BitImage::new(rw, rh, &rmask);
        let rhits = sc.scan_tiered(&rimg, &az_only, false, 0);
        assert_eq!(count(&rhits), 1, "rotated aztec must decode");
        assert_eq!(rhits[0].text, "B");
    }
}

#[cfg(test)]
mod oracle_fixture_tests {
    use crate::*;
    use crate::masks::*;
    use crate::render::*;
    use barcode_fixtures::Canvas;

    /// Every banked oracle specimen must decode through the production
    /// mask decoders. Validates the bank's polarity AND keeps the fixture
    /// bank (used by show.html / e2e / harden) in sync with the decoders.
    fn img_of(canvas: &Canvas) -> BitImage<'static> {
        // leak for the test's lifetime; the canvases are small
        let bits: &'static [u8] = Box::leak(canvas.bits().to_vec().into_boxed_slice());
        BitImage::new(canvas.width, canvas.height, bits)
    }

    #[test]
    fn oracle_specimens_round_trip() {
        let mut failures = Vec::new();
        for (sym, text, m) in barcode_fixtures::oracle_matrices::bank() {
            let canvas = matrix_canvas(m.pattern, m.rows, m.cols, 4, 4);
            let mut hits = match *sym {
                "datamatrix" => decode_datamatrix_mask(&img_of(&canvas)),
                "aztec" => decode_aztec_mask(&img_of(&canvas)),
                "pdf417" => decode_pdf417_mask(&img_of(&canvas)),
                other => panic!("uncovered symbology {other} in oracle bank"),
            };
            if hits.is_empty() && *sym == "datamatrix" {
                // experiment: does the tight (no quiet) render decode?
                let tight = matrix_canvas(m.pattern, m.rows, m.cols, 4, 0);
                hits = decode_datamatrix_mask(&img_of(&tight));
                if !hits.is_empty() {
                    failures.push(format!("{sym} {text:?}: DECODES ONLY WITHOUT QUIET"));
                    continue;
                }
            }
            let ok = hits.len() == 1 && hits[0].text == *text;
            if !ok {
                failures.push(format!(
                    "{sym} {text:?} ({}x{}): {:?}",
                    m.rows,
                    m.cols,
                    hits.iter().map(|h| &h.text).collect::<Vec<_>>(),
                ));
            }
        }
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    /// The Micro QR fixture arm's tier pick must produce a decodable symbol.
    #[test]
    fn microqr_fixture_round_trip() {
        for text in ["12345", "HELLO", "ABC-DEF"] {
            let canvas = microqr_canvas(text, 4).unwrap_or_else(|e| panic!("{text}: {e:?}"));
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let hits = decode_microqr(&img);
            assert_eq!(hits.len(), 1, "{text:?}: {} hits", hits.len());
            assert_eq!(hits[0].text, text, "{text:?} payload");
        }
    }
}

#[cfg(test)]
mod frame_dump_tests {
    use crate::*;

    /// Native diagnosis of real camera frames: DUMP=<file.bits>
    /// [FORMATS=csv] cargo test decode_dump_frame -- --nocapture
    /// Dump layout: u32 w, h, bytes_per_row, flags LE, then mask bytes.
    #[test]
    fn decode_dump_frame() {
        let Ok(path) = std::env::var("DUMP") else { return };
        let formats: Vec<String> = std::env::var("FORMATS")
            .map(|s| s.split(',').map(str::to_string).collect())
            .unwrap_or_default();
        let raw = std::fs::read(&path).expect("read dump");
        let w = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        let h = u32::from_le_bytes(raw[4..8].try_into().unwrap());
        let bpr = u32::from_le_bytes(raw[8..12].try_into().unwrap());
        let bits = &raw[16..];
        assert_eq!(bits.len(), bpr as usize * h as usize, "dump size mismatch");
        let img = BitImage::new(w, h, bits);
        println!("frame {w}x{h} bpr {bpr}");
        let tiers = if formats.is_empty() { Tiers::all() } else { parse_formats(&formats).unwrap() };
        let t0 = std::time::Instant::now();
        let hits = BarcodeScanner::new().scan_tiered(&img, &tiers, false, 0);
        println!("{:?}: {} hits in {:?}", formats, hits.len(), t0.elapsed());
        for hit in &hits {
            println!("  {:?}: {:?}", hit.symbology, hit.text);
        }
        assert!(!hits.is_empty(), "no decode");
    }
}

