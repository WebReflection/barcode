//! Stage-by-stage probe for the DataMatrix tier.
//!
//! Usage: cargo run -p barcode-wasm --example dm_probe -- <mask.bin> <w> <h> <bpr> [live]
//! The mask is the exact 1bpp frame the wasm decode receives (bit=ink).
//! Defaults to the exhaustive still search; `live` uses the live sample
//! budget. Set DM_DBG=1 for the locator's per-corner diagnostics.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (path, w, h, bpr) = (
        args[1].clone(),
        args[2].parse::<u32>().unwrap(),
        args[3].parse::<u32>().unwrap(),
        args[4].parse::<u32>().unwrap(),
    );
    let live = args.get(5).is_some_and(|a| a == "live");
    let budget_ms = args.get(6).map(|a| a.parse::<u64>().unwrap()).unwrap_or(30);
    let data = std::fs::read(&path).unwrap();
    println!(
        "mask {w}x{h} bpr {bpr}, {} bytes (expect {}), ink pixels: {}",
        data.len(),
        bpr as usize * h as usize,
        data.iter().map(|b| b.count_ones()).sum::<u32>()
    );
    let img = barcode_core::BitImage::new(w, h, &data);
    let t0 = std::time::Instant::now();
    let hits = if live {
        barcode_wasm::decode_datamatrix_mask_budget(&img, false, budget_ms)
    } else {
        barcode_wasm::decode_datamatrix_mask(&img)
    };
    println!("datamatrix: {} result(s) in {:?}", hits.len(), t0.elapsed());
    for r in &hits {
        println!("  {:?} ({} bytes)", r.text, r.raw.len());
    }
}
