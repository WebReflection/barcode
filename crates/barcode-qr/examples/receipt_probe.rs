//! Stage-by-stage probe for the receipt QR failure (2026-10-23 field bug).
//!
//! Usage: cargo run -p barcode-qr --example receipt_probe <mask.bin> <w> <h> <bpr>
//! The mask is the exact 1bpp frame the wasm decode receives (bit=ink).

use barcode_core::{BitImage, SymbologyDecoder};
use barcode_qr::QrDecoder;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (path, w, h, bpr) = (
        args[1].clone(),
        args[2].parse::<u32>().unwrap(),
        args[3].parse::<u32>().unwrap(),
        args[4].parse::<u32>().unwrap(),
    );
    let data = std::fs::read(&path).unwrap();
    println!(
        "mask {w}x{h} bpr {bpr}, {} bytes (expect {}), ink pixels: {}",
        data.len(),
        bpr as usize * h as usize,
        data.iter().map(|b| b.count_ones()).sum::<u32>()
    );
    let img = BitImage::new(w, h, &data);

    let mut dec = QrDecoder::new();
    let finders = dec.find_finders(&img);
    match &finders {
        Some(f) => {
            for x in f {
                println!(
                    "finder: ({:.1},{:.1}) module {:.2} strong {}",
                    x.x, x.y, x.module, x.strong
                );
            }
        }
        None => println!("finders: NONE — detection failed"),
    }
    let res = dec.decode(&img);
    println!("decode: {} result(s)", res.len());
    for r in &res {
        println!("  {:?} {:?} ({} bytes)", r.symbology, r.text, r.raw.len());
    }
    println!(
        "presence: {} (0=none, 1=present-marginal, 2=decoded)",
        dec.presence()
    );
}
