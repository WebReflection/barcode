//! Deterministic robustness gate: the decoders must survive arbitrary and
//! corrupted masks without panicking, hanging, or returning structurally
//! invalid results. This is the native (Rust) half of the gate; the
//! browser half is tests/fuzz.html (the real wasm boundary).
//!
//! No external fuzz dependency: a fixed xorshift stream makes every run
//! reproducible, and the assertions only require "safe failure" (empty
//! results or a well-formed result), not a successful read.

use barcode_fixtures::{encode_code128, encode_ean13, encode_qr_text, Canvas};
use barcode_wasm::{BarcodeScanner, Tiers};

/// Deterministic xorshift32. Seed 0 is remapped so the stream never sticks.
fn rng(seed: u32) -> impl FnMut() -> u32 {
    let mut s = seed | 1;
    move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    }
}

fn assert_safe(results: &[barcode_core::DecodeResult]) {
    for r in results {
        assert!(
            !r.text.is_empty() || !r.raw.is_empty(),
            "empty result for {:?}",
            r.symbology
        );
        for p in &r.position {
            assert!(
                p.x.is_finite() && p.y.is_finite(),
                "non-finite position {p:?}"
            );
            assert!(p.x >= -1.0 && p.y >= -1.0, "negative position {p:?}");
        }
    }
}

fn decode(canvas: &Canvas, budget_ms: u32) -> Vec<barcode_core::DecodeResult> {
    let mut scanner = BarcodeScanner::new();
    scanner.decode_still_core(
        canvas.width as usize,
        canvas.height as usize,
        canvas.bits(),
        Tiers::all(),
        budget_ms,
    )
}

#[test]
fn random_masks_never_panic() {
    // Small frames keep the 2D junk-frame cost bounded; the point is the
    // full decoder set running on arbitrary bit patterns, not throughput.
    for &(w, h, density) in &[(64u32, 64u32, 20u32), (96, 96, 50), (128, 96, 80)] {
        for seed in 1..=3u32 {
            let mut next = rng(seed);
            let mut canvas = Canvas::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    if next() % 100 < density {
                        canvas.flip(x, y);
                    }
                }
            }
            assert_safe(&decode(&canvas, 250));
        }
    }
}

#[test]
fn mutated_fixtures_only_ever_read_the_original_text() {
    let cases: Vec<(&str, Canvas, &str)> = vec![
        (
            "ean13",
            encode_ean13("4006381333931", 3, 80, 9),
            "4006381333931",
        ),
        (
            "code128",
            encode_code128("INV-2026-0042", 3, 80),
            "INV-2026-0042",
        ),
        (
            "qrcode",
            encode_qr_text("ROBUSTNESS-QR", 1, 0, 4).expect("qr fixture"),
            "ROBUSTNESS-QR",
        ),
    ];

    for (sym, canvas, expected) in cases {
        // Sanity: the unmutated fixture reads.
        let clean = decode(&canvas, 1_000);
        assert!(
            clean
                .iter()
                .any(|r| r.symbology.name() == sym && r.text == expected),
            "clean {sym} fixture did not decode: {:?}",
            clean
                .iter()
                .map(|r| (r.symbology.name(), &r.text))
                .collect::<Vec<_>>()
        );

        // Flip 5% of the modules. A read of the SAME symbology must be the
        // original payload: checksums/RS may reject, but never rewrite.
        for seed in 1..=4u32 {
            let mut next = rng(seed * 7919);
            let mut corrupt = Canvas::new(canvas.width, canvas.height);
            for y in 0..canvas.height {
                for x in 0..canvas.width {
                    let bit = (canvas.bits()
                        [y as usize * canvas.bytes_per_row + (x >> 3) as usize]
                        >> (x & 7))
                        & 1;
                    if next() % 100 < 5 {
                        corrupt.flip(x, y);
                    } else if bit == 1 {
                        corrupt.flip(x, y);
                    }
                }
            }
            let out = decode(&corrupt, 1_000);
            assert_safe(&out);
            for r in &out {
                if r.symbology.name() == sym {
                    assert_eq!(r.text, expected, "false positive on mutated {sym}");
                }
            }
        }
    }
}
