//! The still-path wall must stay bounded by the latency budget even when
//! the tilt ladder runs. Regression for the 30s-wall STATUS item; also
//! documents the per-tier fixed costs on the receipt frame.

use barcode_core::BitImage;
use barcode_wasm::{BarcodeScanner, Tiers};
use std::time::Instant;

fn load_pgm() -> Option<(u32, u32, Vec<u8>)> {
    let raw = std::fs::read("../../packages/web/test-codes/field/viva-dm-rot0.pgm").ok()?;
    let mut parts = raw.splitn(4, |b| b == &b'\n');
    if parts.next()? != b"P5" {
        return None;
    }
    let dims = String::from_utf8(parts.next()?.to_vec()).ok()?;
    let mut it = dims.split_whitespace();
    let w: u32 = it.next()?.parse().ok()?;
    let h: u32 = it.next()?.parse().ok()?;
    let _max = parts.next()?;
    Some((w, h, parts.next()?.to_vec()))
}

#[test]
fn still_wall_bounded() {
    let Some((w, h, luma)) = load_pgm() else {
        eprintln!("skipped: field fixture absent");
        return;
    };
    let (w, h) = (w as usize, h as usize);
    let mut img = vec![0u8; w * h];
    for i in 0..w * h {
        img[i] = if luma[i] < 128 { 1 } else { 0 };
    }
    let frame = BitImage::new(w as u32, h as u32, &img);
    let mut scanner = BarcodeScanner::new();
    for budget in [500u32, 1000, 5000] {
        let t0 = Instant::now();
        let hits = scanner.decode_still_core(w, h, &frame.data, Tiers::all(), budget);
        let wall = t0.elapsed().as_millis();
        eprintln!("budget {budget}ms -> wall {wall}ms, hits={}", hits.len());
        // Wall clock + the fixed cost of the last in-flight DM step and
        // the linear tier on a junk-rich frame (measured ~2.9s at the
        // 500ms budget; the guard is for multi-second regressions).
        assert!(
            wall <= budget as u128 + 3000,
            "wall {wall}ms exceeded budget {budget}ms + slack"
        );
    }
    // the DM-only tier: the ladder's tilt retries used to multiply the
    // budget several-fold on this junk-rich frame
    let t0 = Instant::now();
    let hits = scanner.decode_still_core(
        w,
        h,
        &frame.data,
        Tiers {
            datamatrix: true,
            ..Default::default()
        },
        500,
    );
    let wall = t0.elapsed().as_millis();
    eprintln!("dm-only 500ms -> wall {wall}ms, hits={}", hits.len());
    assert!(wall <= 500 + 1500, "dm-only wall {wall}ms");
}
