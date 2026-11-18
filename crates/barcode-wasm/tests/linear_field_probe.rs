//! Field probe for the linear tier: a saved 1bpp mask of a real receipt.
//! Skipped when the personal fixture is absent (field photos are local-only).
//!
//! Regenerate the mask with the same pipeline the browser uses (CPU
//! binarizer mirror) or `tools/…`; see docs/STATUS.md field notes.

use barcode_wasm::{BarcodeScanner, Tiers};

fn load_pgm(path: &str) -> Option<(usize, usize, Vec<u8>)> {
    let raw = std::fs::read(path).ok()?;
    let mut parts = raw.splitn(4, |b| b == &b'\n');
    if parts.next()? != b"P5" {
        return None;
    }
    let dims = String::from_utf8(parts.next()?.to_vec()).ok()?;
    let mut it = dims.split_whitespace();
    let w: usize = it.next()?.parse().ok()?;
    let h: usize = it.next()?.parse().ok()?;
    let _max = parts.next()?;
    Some((w, h, parts.next()?.to_vec()))
}

fn pack(w: usize, h: usize, luma: &[u8]) -> Vec<u8> {
    let bpr = w.div_ceil(8).div_ceil(4) * 4;
    let mut data = vec![0u8; bpr * h];
    for y in 0..h {
        for x in 0..w {
            if luma[y * w + x] > 127 {
                data[y * bpr + (x >> 3)] |= 1 << (x & 7);
            }
        }
    }
    data
}

fn check(name: &str, want: &str) {
    let path = format!("../../packages/web/test-codes/field/phone-2026-11-17/mask-2112-{name}.pgm");
    let Some((w, h, luma)) = load_pgm(&path) else {
        eprintln!("skipped: {path} absent");
        return;
    };
    let data = pack(w, h, &luma);
    let mut sc = BarcodeScanner::new();
    let hits = sc.decode_still_core(w, h, &data, Tiers { code128: true, ..Default::default() }, 5000);
    eprintln!("{name} hits: {:?}", hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>());
    assert!(hits.iter().any(|r| r.text == want), "{name}: barcode not read");
}

#[test]
fn receipt_mask_reads_through_the_linear_tier() {
    // Personal field fixtures (gitignored); each skips when absent.
    check("112238", "108717112609540270746300");
    check("112253", "108716112611390340236700");
}
