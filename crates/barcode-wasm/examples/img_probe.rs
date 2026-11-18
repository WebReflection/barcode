//! Decode a binary PGM (P5) still image through the full native tier set.
//!
//! Usage: cargo run -p barcode-wasm --example img_probe -- file.pgm [threshold]
//!
//! Diagnostic only (real field photos): the web worker's still path
//! downscales to 1056 px, so this is the native-resolution opinion.

use barcode_wasm::{BarcodeScanner, Tiers};

fn read_pgm(path: &str) -> (usize, usize, Vec<u8>) {
    let bytes = std::fs::read(path).expect("read pgm");
    // P5 <w> <h> <maxval> <data>, whitespace/comments between header tokens
    let mut i = 0usize;
    let token = |i: &mut usize| -> String {
        loop {
            while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
                *i += 1;
            }
            if *i < bytes.len() && bytes[*i] == b'#' {
                while *i < bytes.len() && bytes[*i] != b'\n' {
                    *i += 1;
                }
                continue;
            }
            break;
        }
        let start = *i;
        while *i < bytes.len() && !bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
        String::from_utf8_lossy(&bytes[start..*i]).to_string()
    };
    assert_eq!(token(&mut i), "P5", "not a P5 pgm");
    let w: usize = token(&mut i).parse().unwrap();
    let h: usize = token(&mut i).parse().unwrap();
    let _max: u32 = token(&mut i).parse().unwrap();
    i += 1; // the single whitespace before the raster
    let rest = bytes[i..].to_vec();
    assert!(rest.len() >= w * h, "pgm data short: {} < {}", rest.len(), w * h);
    (w, h, rest)
}

fn pack(w: usize, h: usize, gray: &[u8], threshold: u8) -> Vec<u8> {
    let bytes_per_row = w.div_ceil(8).div_ceil(4) * 4;
    let mut data = vec![0u8; bytes_per_row * h];
    for y in 0..h {
        for x in 0..w {
            if gray[y * w + x] < threshold {
                data[y * bytes_per_row + (x >> 3)] |= 1 << (x & 7);
            }
        }
    }
    data
}

fn rotate90(w: usize, h: usize, gray: &[u8], turns: usize) -> (usize, usize, Vec<u8>) {
    // clockwise quarter turns
    let mut cur = (w, h, gray.to_vec());
    for _ in 0..turns {
        let (cw, ch, c) = cur;
        let (nw, nh) = (ch, cw);
        let mut out = vec![0u8; nw * nh];
        for y in 0..ch {
            for x in 0..cw {
                out[x * nw + (ch - 1 - y)] = c[y * cw + x];
            }
        }
        cur = (nw, nh, out);
    }
    cur
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: img_probe file.pgm [threshold]");
    let threshold: u8 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(110);
    let (w, h, gray) = read_pgm(path);
    let mut scanner = BarcodeScanner::new();
    for turns in 0..4 {
        let (rw, rh, rot) = rotate90(w, h, &gray, turns);
        let packed = pack(rw, rh, &rot, threshold);
        let hits = scanner.decode_still_core(rw, rh, &packed, Tiers::all(), 5000);
        println!(
            "rot{} {rw}x{rh} thr{} -> {:?}",
            turns * 90,
            threshold,
            hits.iter().map(|r| (format!("{:?}", r.symbology), r.text.clone())).collect::<Vec<_>>()
        );
    }
}
