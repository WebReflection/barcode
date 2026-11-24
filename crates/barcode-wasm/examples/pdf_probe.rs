//! Stage-by-stage probe for the PDF417 tier.
//!
//! Usage: cargo run -p barcode-wasm --example pdf_probe -- <mask.bin> <w> <h> <bpr> [still]
//! The mask is the exact 1bpp frame the wasm decode receives (bit=ink).
//! `still` runs the production still path (tilt ladder included);
//! otherwise the raw upright tier runs. Set PDF_DBG2=1 for per-row
//! start/stop diagnostics from the detector.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (path, w, h, bpr) = (
        args[1].clone(),
        args[2].parse::<u32>().unwrap(),
        args[3].parse::<u32>().unwrap(),
        args[4].parse::<u32>().unwrap(),
    );
    let still = args.get(5).is_some_and(|a| a == "still");
    let data = std::fs::read(&path).unwrap();
    println!(
        "mask {w}x{h} bpr {bpr}, {} bytes (expect {}), ink pixels: {}",
        data.len(),
        bpr as usize * h as usize,
        data.iter().map(|b| b.count_ones()).sum::<u32>()
    );
    let img = barcode_core::BitImage::new(w, h, &data);
    if args.get(5).is_some_and(|a| a == "rect") {
        let hit = barcode_pdf417::read_frame_rectified(&img);
        println!(
            "rectified: {}",
            if hit.is_some() {
                "1 result"
            } else {
                "0 results"
            }
        );
        if let Some(hit) = hit {
            println!(
                "  {:?} ({} bytes)",
                barcode_core::payload_text(&hit.payload),
                hit.payload.len()
            );
        }
        return;
    }
    if still {
        let mut tiers = barcode_wasm::Tiers::default();
        tiers.pdf417 = true;
        let mut sc = barcode_wasm::BarcodeScanner::new();
        let hits = sc.decode_still_core(w as usize, h as usize, &data, tiers, 5000);
        println!("still path: {} result(s)", hits.len());
        for r in &hits {
            println!("  {:?} ({} bytes)", r.text, r.raw.len());
        }
    } else {
        let hits = barcode_wasm::decode_pdf417_mask(&img);
        println!("pdf417: {} result(s)", hits.len());
        for r in &hits {
            println!("  {:?} ({} bytes)", r.text, r.raw.len());
        }
    }
}
