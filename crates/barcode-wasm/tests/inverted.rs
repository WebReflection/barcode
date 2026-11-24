//! Polarity gate (roadmap item 5): white-on-black codes — the binarizer
//! reports the black background as ink, so the mask reaches the decoders
//! inverted. Every 2D family and the 1D set must read both polarities.

use barcode_fixtures::Canvas;
use barcode_wasm::{BarcodeScanner, Tiers};

/// Invert the packed mask (padding bits become 1; BitImage ignores them).
fn inverted(c: &Canvas) -> Vec<u8> {
    c.bits().iter().map(|b| !b).collect()
}

fn assert_reads(name: &str, c: &Canvas, tiers: Tiers, want: &str) {
    let inv = inverted(c);
    let mut sc = BarcodeScanner::new();
    let hits = sc.decode_still_core(c.width as usize, c.height as usize, &inv, tiers, 4000);
    let hit = hits.iter().find(|h| h.text == want);
    assert!(
        hit.is_some(),
        "{name}: inverted fixture did not read (got {:?})",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
    // Live law too: the polarity second opinion is gated by ink fraction.
    let mut sc = BarcodeScanner::new();
    let hits = sc.decode_live_core(c.width as usize, c.height as usize, &inv, tiers);
    assert!(
        hits.iter().any(|h| h.text == want),
        "{name}: inverted fixture did not read live (got {:?})",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
    // Also assert the upright fixture still reads (no polarity regression).
    let mut sc = BarcodeScanner::new();
    let hits = sc.decode_still_core(c.width as usize, c.height as usize, c.bits(), tiers, 4000);
    assert!(
        hits.iter().any(|h| h.text == want),
        "{name}: upright fixture stopped reading"
    );
}

/// Render a module matrix with a quiet zone (2D families).
fn render(matrix: &[u8], rows: usize, cols: usize, module: u32, quiet: u32) -> Canvas {
    let mut c = Canvas::new(
        (cols as u32 + 2 * quiet) * module,
        (rows as u32 + 2 * quiet) * module,
    );
    for y in 0..rows {
        for x in 0..cols {
            if matrix[y * cols + x] == 1 {
                c.fill(
                    (quiet + x as u32) * module,
                    (quiet + y as u32) * module,
                    module,
                    module,
                );
            }
        }
    }
    c
}

fn microqr_canvas(text: &str, module: u32, quiet: u32) -> Canvas {
    use barcode_microqr::{encode, Level, Version};
    let sym = encode(text, Version::M3, Level::L, None).expect("microqr payload");
    render(&sym.cells, sym.size, sym.size, module, quiet)
}

#[test]
fn inverted_2d_families_read() {
    let module = 5;
    let quiet = 4;

    let qr = barcode_fixtures::encode_qr_text("INVERTED-QR", 0, 2, module).unwrap();
    assert_reads(
        "qrcode",
        &qr,
        Tiers {
            qr: true,
            ..Default::default()
        },
        "INVERTED-QR",
    );

    let mq = microqr_canvas("INVERTED", module, quiet);
    assert_reads(
        "microqr",
        &mq,
        Tiers {
            microqr: true,
            ..Default::default()
        },
        "INVERTED",
    );

    let dm = barcode_dm::encode::encode(b"INVERTED-DM").unwrap();
    let dm_c = render(&dm.matrix, dm.spec.rows, dm.spec.cols, module, quiet);
    assert_reads(
        "datamatrix",
        &dm_c,
        Tiers {
            datamatrix: true,
            ..Default::default()
        },
        "INVERTED-DM",
    );

    let (spec, matrix) = barcode_aztec::encode::encode(b"INVERTED-AZTEC").unwrap();
    let az_c = render(
        &matrix,
        spec.rows as usize,
        spec.cols as usize,
        module,
        quiet,
    );
    assert_reads(
        "aztec",
        &az_c,
        Tiers {
            aztec: true,
            ..Default::default()
        },
        "INVERTED-AZTEC",
    );

    let pf = barcode_pdf417::encode::encode_full(b"INVERTED-PDF417", 2).unwrap();
    let pf_c = render(&pf.matrix, pf.height, pf.width, module, 2);
    assert_reads(
        "pdf417",
        &pf_c,
        Tiers {
            pdf417: true,
            ..Default::default()
        },
        "INVERTED-PDF417",
    );
}

#[test]
fn inverted_1d_families_read() {
    let module = 4;
    let height = 80;
    let quiet = 10 * module;

    let cases: Vec<(&str, Canvas, Tiers, &str)> = vec![
        (
            "ean13",
            barcode_fixtures::encode_ean13("4006381333931", module, height, quiet),
            Tiers {
                ean13: true,
                ..Default::default()
            },
            "4006381333931",
        ),
        (
            "upca",
            barcode_fixtures::encode_upca("036000291452", module, height, quiet),
            Tiers {
                upca: true,
                ..Default::default()
            },
            "036000291452",
        ),
        (
            "ean8",
            barcode_fixtures::encode_ean8("96385074", module, height, quiet),
            Tiers {
                ean8: true,
                ..Default::default()
            },
            "96385074",
        ),
        (
            "upce",
            barcode_fixtures::encode_upce("042100005264", module, height, quiet),
            Tiers {
                upce: true,
                ..Default::default()
            },
            "042100005264",
        ),
        (
            "code39",
            barcode_fixtures::encode_code39("ABC-123", module, 3.0, height),
            Tiers {
                code39: true,
                ..Default::default()
            },
            "ABC-123",
        ),
        (
            "code93",
            barcode_fixtures::encode_code93("TEST93", module, height),
            Tiers {
                code93: true,
                ..Default::default()
            },
            "TEST93",
        ),
        (
            "code128",
            barcode_fixtures::encode_code128("CODE128", module, height),
            Tiers {
                code128: true,
                ..Default::default()
            },
            "CODE128",
        ),
        (
            "itf",
            barcode_fixtures::encode_itf("12345670", module, 2.5, height),
            Tiers {
                itf: true,
                ..Default::default()
            },
            "12345670",
        ),
        (
            "codabar",
            barcode_fixtures::encode_codabar("40156", module, 2.5, height),
            Tiers {
                codabar: true,
                ..Default::default()
            },
            "40156",
        ),
    ];
    for (name, c, tiers, want) in cases {
        assert_reads(name, &c, tiers, want);
    }
}

#[test]
fn inverted_1d_live_framing_reads() {
    // Camera framing: the code occupies a small strip of a full frame; the
    // inverted paper around it is ink. The polarity gate must still fire.
    let code = barcode_fixtures::encode_code128("INVERTED-CODE128", 4, 120);
    let mut frame = Canvas::new(1056, 1056);
    // white frame first (Canvas is all zero = background/light)
    let x0 = (1056 - code.width) / 2;
    let y0 = (1056 - code.height) / 2;
    for y in 0..code.height {
        for x in 0..code.width {
            let i = y as usize * code.bytes_per_row + (x as usize >> 3);
            if (code.bits()[i] >> (x & 7)) & 1 == 1 {
                frame.fill(x0 + x, y0 + y, 1, 1);
            }
        }
    }
    let inv = inverted(&frame);
    let mut sc = BarcodeScanner::new();
    let tiers = Tiers {
        code128: true,
        ..Default::default()
    };
    let hits = sc.decode_live_core(frame.width as usize, frame.height as usize, &inv, tiers);
    assert!(
        hits.iter().any(|h| h.text == "INVERTED-CODE128"),
        "inverted code128 with live framing did not read (got {:?})",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );

    // The confirmation close-up: the scanner remembers the inverted
    // polarity, and the close-up of a bar strip is only ~40% ink (below
    // the full-frame heuristic). Re-feed the tight code to the SAME
    // scanner: it must read through the remembered polarity.
    let tight_inv = inverted(&code);
    let hits = sc.decode_live_core(code.width as usize, code.height as usize, &tight_inv, tiers);
    assert!(
        hits.iter().any(|h| h.text == "INVERTED-CODE128"),
        "inverted code128 close-up (remembered polarity) did not read (got {:?})",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
}

#[test]
fn inverted_databar_families_read() {
    let module = 4;
    let height = 60;
    let text = "0123456789012";
    let want = "(01)01234567890128"; // GS1 AI (01) + check digit
    let v: u64 = text.parse().unwrap();

    let runs = barcode_fixtures::databar_omni_runs(v);
    let c = barcode_fixtures::databar_omni_canvas(&runs, module, height, 10 * module);
    assert_reads(
        "databar",
        &c,
        Tiers {
            databar: true,
            ..Default::default()
        },
        want,
    );

    let runs = barcode_fixtures::databar_ltd_runs(v);
    let c = barcode_fixtures::databar_ltd_canvas(&runs, module, height, 10 * module);
    assert_reads(
        "databarltd",
        &c,
        Tiers {
            databarltd: true,
            ..Default::default()
        },
        want,
    );

    let runs = barcode_fixtures::databar_omni_runs(v);
    let c = barcode_fixtures::databar_stacked_canvas(&runs, module, 10 * module, false);
    assert_reads(
        "databarstk",
        &c,
        Tiers {
            databarstk: true,
            ..Default::default()
        },
        want,
    );
}

/// Paste the INVERTED rendering of a fixture into the middle of a white
/// page: the black block carrying the white modules is a small part of the
/// page, so the full-frame ink fraction stays far below 50%. This is what a
/// screenshot/photo of a white-on-black code with white margins looks like
/// — and the live polarity heuristic deliberately skips inversion there, so
/// the still path must carry it.
fn inverted_on_white_page(c: &Canvas, page: u32) -> Canvas {
    let mut out = Canvas::new(page, page);
    let x0 = (page - c.width) / 2;
    let y0 = (page - c.height) / 2;
    // black block = the code's paper
    out.fill(x0, y0, c.width, c.height);
    for y in 0..c.height {
        for x in 0..c.width {
            let i = y as usize * c.bytes_per_row + (x as usize >> 3);
            // a dark module becomes white (the inverted rendering)
            if (c.bits()[i] >> (x & 7)) & 1 == 1 {
                out.clear(x0 + x, y0 + y, 1, 1);
            }
        }
    }
    out
}

#[test]
fn inverted_code_on_a_light_page_reads_still() {
    let module = 4;
    let qr = barcode_fixtures::encode_qr_text("ISLAND-QR", 0, 2, module).unwrap();
    let page = inverted_on_white_page(&qr, 1056);
    // The premise: the page is mostly light, so the live ink heuristic
    // (>= 50%) would not even consider the inverted sweep.
    let ones: u32 = page.bits().iter().map(|b| b.count_ones()).sum();
    assert!(
        (ones as f64) / (1056.0 * 1056.0) < 0.5,
        "fixture is not a light page"
    );
    let mut sc = BarcodeScanner::new();
    let hits = sc.decode_still_core(
        page.width as usize,
        page.height as usize,
        page.bits(),
        Tiers {
            qr: true,
            ..Default::default()
        },
        4000,
    );
    assert!(
        hits.iter().any(|h| h.text == "ISLAND-QR"),
        "inverted QR on a light page did not read still (got {:?})",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
}

#[test]
fn still_inverted_does_not_leak_polarity_and_live_ttl_decays() {
    let code = barcode_fixtures::encode_code128("INV-LEAK-1", 4, 80);
    let inv = inverted(&code);
    let tiers = Tiers {
        code128: true,
        ..Default::default()
    };

    // 1. A still inverted read must NOT set remembered polarity for future live scans.
    let mut sc = BarcodeScanner::new();
    let hits = sc.decode_still_core(code.width as usize, code.height as usize, &inv, tiers, 4000);
    assert!(hits.iter().any(|h| h.text == "INV-LEAK-1"));
    // Construct a light-page inverted island (<50% ink): if polarity had
    // leaked from the still scan, decode_live_core would try inverted first
    // and decode it; without a leak, live does not invert a <50% ink frame.
    let island = inverted_on_white_page(&code, 1056);
    let leaked = sc.decode_live_core(
        island.width as usize,
        island.height as usize,
        island.bits(),
        tiers,
    );
    assert!(
        leaked.is_empty(),
        "still inverted leaked polarity into live scan"
    );

    // 2. A live inverted hit remembers polarity for close-up confirmation,
    //    then decays cleanly after a few blank misses so it does not linger.
    let mut sc2 = BarcodeScanner::new();
    let full_inv = inverted(&{
        let mut f = Canvas::new(1056, 1056);
        let x0 = (1056 - code.width) / 2;
        let y0 = (1056 - code.height) / 2;
        for y in 0..code.height {
            for x in 0..code.width {
                let i = y as usize * code.bytes_per_row + (x as usize >> 3);
                if (code.bits()[i] >> (x & 7)) & 1 == 1 {
                    f.fill(x0 + x, y0 + y, 1, 1);
                }
            }
        }
        f
    });
    assert!(sc2
        .decode_live_core(1056, 1056, &full_inv, tiers)
        .iter()
        .any(|h| h.text == "INV-LEAK-1"));
    // Feed 3 blank (normal white) frames to expire the TTL.
    let blank = Canvas::new(128, 128);
    for _ in 0..3 {
        assert!(sc2
            .decode_live_core(128, 128, blank.bits(), tiers)
            .is_empty());
    }
    // After decay, a <50% ink inverted island is no longer decoded via stale polarity.
    assert!(
        sc2.decode_live_core(
            island.width as usize,
            island.height as usize,
            island.bits(),
            tiers
        )
        .is_empty(),
        "live inverted TTL failed to decay after blank misses"
    );
}
