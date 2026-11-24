//! Rendered-symbol locator regression: our own encoder's output must be
//! findable by the reference locator at camera/scan-typical scales
//! (NN-upscaled masks with a 4-module quiet zone), not just via
//! decode_upright on the exact matrix. The pitch ladder used to starve
//! its sample budget at ~10px/module before reaching the true
//! (spec, pitch) candidate; try_corner now orders candidates by the
//! corner legs' own pitch estimate.

use barcode_dm::encode::encode;
use barcode_dm::locator::{locate_binary, BinaryMap};

fn to_map(m: &[u8], r: usize, c: usize) -> BinaryMap {
    let (w, h) = (c + 8, r + 8);
    let mut dark = vec![false; w * h];
    for y in 0..r {
        for x in 0..c {
            dark[(y + 4) * w + (x + 4)] = m[y * c + x] != 0;
        }
    }
    BinaryMap { w, h, dark }
}

fn upscale(map: &BinaryMap, n: usize) -> BinaryMap {
    let (w, h) = (map.w * n, map.h * n);
    let mut dark = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            dark[y * w + x] = map.dark[(y / n) * map.w + (x / n)];
        }
    }
    BinaryMap { w, h, dark }
}

#[test]
fn locates_at_scan_scales() {
    for text in [
        "DATA MATRIX LONGER PAYLOAD 42",
        "00385F00030004L51QZWWY0003304960",
        "A",
    ] {
        let enc = encode(text.as_bytes()).unwrap();
        let map = to_map(&enc.matrix, enc.spec.rows, enc.spec.cols);
        for n in [6usize, 8, 10, 12, 16] {
            let hits = locate_binary(&upscale(&map, n));
            assert!(
                hits.iter()
                    .any(|l| l.decoded.payload.as_deref() == Some(text.as_bytes())),
                "{text:?}: not located at x{n}"
            );
        }
    }
}
