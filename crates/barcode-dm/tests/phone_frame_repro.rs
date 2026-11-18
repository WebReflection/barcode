//! Temporary repro for the phone frame: the wasm path (GPU-style Bradley
//! mask -> decode_datamatrix_mask_budget) hits at 180/270 but misses at
//! 0/90. Mirror that natively, per rotation.

use barcode_dm::locator::{locate_binary, BinaryMap};

fn load_pgm(path: &str) -> Option<(usize, usize, Vec<u8>)> {
    let raw = std::fs::read(path).ok()?;
    let mut parts = raw.splitn(4, |b| b == &b'\n');
    if parts.next()? != b"P5" { return None; }
    let dims = String::from_utf8(parts.next()?.to_vec()).ok()?;
    let mut it = dims.split_whitespace();
    let w: usize = it.next()?.parse().ok()?;
    let h: usize = it.next()?.parse().ok()?;
    let _max = parts.next()?;
    Some((w, h, parts.next()?.to_vec()))
}

fn rotated(w: usize, h: usize, luma: &[u8], deg: u32) -> (usize, usize, Vec<u8>) {
    let (sw, sh) = if deg == 90 || deg == 270 { (h, w) } else { (w, h) };
    let rad = (deg as f64).to_radians();
    let (sin, cos) = (rad.sin(), rad.cos());
    let cx = w as f64 / 2.0; let cy = h as f64 / 2.0;
    let ncx = sw as f64 / 2.0; let ncy = sh as f64 / 2.0;
    let mut out = vec![0u8; sw * sh];
    for y in 0..sh { for x in 0..sw {
        let dx = x as f64 - ncx; let dy = y as f64 - ncy;
        let sx = (cos * dx - sin * dy + cx) as usize;
        let sy = (sin * dx + cos * dy + cy) as usize;
        out[y * sw + x] = if sx < w && sy < h { luma[sy * w + sx] } else { 255 };
    }}
    (sw, sh, out)
}

/// Bradley rule matching the GPU binarizer config (maxSize 1056,
/// windowDivisor 20, threshold 0.92): dark iff luma < 0.92 * local mean.
fn bradley(w: usize, h: usize, luma: &[u8]) -> BinaryMap {
    let radius = (w.max(h) / 20).max(1);
    let stride = w + 1;
    let mut ii = vec![0u64; stride * (h + 1)];
    for y in 0..h {
        let mut row = 0u64;
        for x in 0..w {
            row += luma[y * w + x] as u64;
            ii[(y + 1) * stride + x + 1] = ii[y * stride + x + 1] + row;
        }
    }
    let area = |x0: i64, y0: i64, x1: i64, y1: i64| -> u64 {
        let (x0, y0, x1, y1) = (x0.max(0) as usize, y0.max(0) as usize, (x1 as usize).min(w), (y1 as usize).min(h));
        ii[y1 * stride + x1] + ii[y0 * stride + x0] - ii[y0 * stride + x1] - ii[y1 * stride + x0]
    };
    let mut dark = vec![false; w * h];
    let wi = w as i64; let hi = h as i64;
    for y in 0..h {
        for x in 0..w {
            let (x0, y0) = ((x as i64 - radius as i64).max(0), (y as i64 - radius as i64).max(0));
            let (x1, y1) = ((x as i64 + radius as i64).min(wi), (y as i64 + radius as i64).min(hi));
            let n = ((x1 - x0) * (y1 - y0)) as u64;
            let mean = area(x0, y0, x1, y1) / n.max(1);
            dark[y * w + x] = (luma[y * w + x] as u64) < (92 * mean / 100);
        }
    }
    BinaryMap { w, h, dark }
}



/// The regression: the receipt DM (12x36, ~7px/module) must decode from
/// the native-resolution mask's 2x NN upscale at upright orientation.
/// Skipped silently when the personal-data fixture is absent.
#[test]
fn phone_frame_upscale_decodes() {
    let Some((w, h, luma)) = load_pgm("../../packages/web/test-codes/field/viva-dm-rot0.pgm") else {
        eprintln!("skipped: field fixture not present locally"); return;
    };
    let map = bradley(w, h, &luma);
    let big = upscale2(&map);
    let locs = locate_binary(&big);
    let hit = locs.iter().find(|l| {
        l.rows == 12 && l.cols == 36
            && l.decoded.payload.as_deref() == Some(PAYLOAD.as_bytes())
    });
    assert!(hit.is_some(), "receipt DM not decoded from the 2x upscale");
}

const PAYLOAD: &str = "00385F00030004L51QZWWY0003304960";

fn upscale2(map: &BinaryMap) -> BinaryMap {
    let (uw, uh) = (map.w * 2, map.h * 2);
    let mut dark = vec![false; uw * uh];
    for y in 0..uh { for x in 0..uw {
        dark[y * uw + x] = map.dark[(y / 2) * map.w + (x / 2)];
    }}
    BinaryMap { w: uw, h: uh, dark }
}

#[test]
fn phone_frame_all_orientations() {
    let Some((w, h, luma)) = load_pgm("../../packages/web/test-codes/field/viva-dm-rot0.pgm") else {
        eprintln!("fixture missing"); return;
    };
    assert_eq!(luma.len(), w * h);
    for deg in [0u32, 90, 180, 270] {
        let (rw, rh, rl) = rotated(w, h, &luma, deg);
        let base = bradley(rw, rh, &rl);
        let dark = base.dark.iter().filter(|d| **d).count();
        eprintln!("deg {deg}: dark {dark}/{}", base.dark.len());
        // contained-fix check: 2x NEAREST-NEIGHBOR upscale of the MASK
        // itself (no luma needed) — if this hits, the ladder can do it
        {
            let (uw, uh) = (rw * 2, rh * 2);
            let mut dark2 = vec![false; uw * uh];
            for y in 0..uh { for x in 0..uw {
                dark2[y * uw + x] = base.dark[(y / 2) * rw + (x / 2)];
            }}
            let map = BinaryMap { w: uw, h: uh, dark: dark2 };
            let locs = locate_binary(&map);
            eprintln!("  f=x2 NN mask upscale: {} locations", locs.len());
            for l in &locs {
                eprintln!("    {}x{} {:?} pitch {:.2} payload {:?}", l.rows, l.cols, l.kind, l.pitch, l.decoded.payload);
            }
        }
        // hypothesis: a 2x UPSCALE (bilinear luma) lands the pitch where
        // the locator's grid estimation works — the crop-2x phone hit
        {
            let (uw, uh) = (rw * 2, rh * 2);
            let mut up = vec![0u8; uw * uh];
            for y in 0..uh {
                let sy = (y as f64 / 2.0) as usize;
                let sy0 = sy.min(rh - 1); let sy1 = (sy + 1).min(rh - 1);
                let fy = y as f64 / 2.0 - sy as f64;
                for x in 0..uw {
                    let sx = (x as f64 / 2.0) as usize;
                    let sx0 = sx.min(rw - 1); let sx1 = (sx + 1).min(rw - 1);
                    let fx = x as f64 / 2.0 - sx as f64;
                    let a = rl[sy0 * rw + sx0] as f64; let b = rl[sy0 * rw + sx1] as f64;
                    let c = rl[sy1 * rw + sx0] as f64; let d = rl[sy1 * rw + sx1] as f64;
                    up[y * uw + x] = (a * (1.0-fx) * (1.0-fy) + b * fx * (1.0-fy) + c * (1.0-fx) * fy + d * fx * fy) as u8;
                }
            }
            let map = bradley(uw, uh, &up);
            let locs = locate_binary(&map);
            eprintln!("  f=0.5 (2x upscale): {} locations", locs.len());
            for l in &locs {
                eprintln!("    {}x{} {:?} pitch {:.2} payload {:?}", l.rows, l.cols, l.kind, l.pitch, l.decoded.payload);
            }
        }
        // mirror decode_datamatrix_mask_budget's scale ladder: f=1, then box-down f=4, f=2
        for f in [1u32, 4, 2] {
            let (sw, sh) = ((rw / f as usize), (rh / f as usize));
            let mut dark2 = vec![false; sw * sh];
            for y in 0..sh { for x in 0..sw {
                let mut ink = 0;
                for dy in 0..f as usize { for dx in 0..f as usize {
                    if base.dark[(y * f as usize + dy) * rw + (x * f as usize + dx)] { ink += 1; }
                }}
                if ink * 2 >= (f * f) as usize { dark2[y * sw + x] = true; }
            }}
            let map = BinaryMap { w: sw, h: sh, dark: dark2 };
            let locs = if f >= 2 {
                let fast = barcode_dm::locator::locate_binary_fast_budget(&map, 8_000 * 7_700 / 2);
                if !fast.is_empty() { fast } else { barcode_dm::locator::locate_binary_budget(&map, 8_000 * 7_700 / 2) }
            } else {
                locate_binary(&map)
            };
            eprintln!("  f={f}: {} locations", locs.len());
            for l in &locs {
                eprintln!("    {}x{} {:?} pitch {:.2} payload {:?}", l.rows, l.cols, l.kind, l.pitch, l.decoded.payload);
            }
        }
    }
}
