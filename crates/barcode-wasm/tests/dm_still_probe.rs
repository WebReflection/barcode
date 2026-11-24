//! Mirror the wasm still path exactly: BitImage -> decode_datamatrix_mask_budget.

use barcode_wasm::decode_datamatrix_mask_budget;

#[test]
fn still_ladder_on_crisp_mask() {
    for text in [
        "DATA MATRIX LONGER PAYLOAD 42",
        "00385F00030004L51QZWWY0003304960",
    ] {
        let enc = barcode_dm::encode::encode(text.as_bytes()).unwrap();
        let (r, c) = (enc.spec.rows, enc.spec.cols);
        // crisp 8px/module canvas: (r+8)x(c+8) modules * 8px, 1bpp like the pipeline
        let n = 8usize;
        let (w, h) = ((c + 8) * n, (r + 8) * n);
        let bpr = (w.div_ceil(8).div_ceil(4)) * 4;
        let mut data = vec![0u8; bpr * h];
        for y in 0..h {
            for x in 0..w {
                let my = y / n;
                let mx = x / n;
                let ink = my >= 4
                    && my < r + 4
                    && mx >= 4
                    && mx < c + 4
                    && enc.matrix[(my - 4) * c + (mx - 4)] != 0;
                if ink {
                    data[y * bpr + (x >> 3)] |= 1 << (x & 7);
                }
            }
        }
        let img = barcode_core::BitImage::new(w as u32, h as u32, &data);
        let hits = decode_datamatrix_mask_budget(&img, true, 5000);
        eprintln!("{text:?} {w}x{h}: {} hits", hits.len());
        assert!(!hits.is_empty(), "{text:?}: still ladder missed");
    }
}
