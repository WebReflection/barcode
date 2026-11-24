//! Data Matrix wasm-tier tests: the locator consumes the same 1bpp mask
//! the GPU pipeline produces (Bradley 0.85 + packed bits). Here the mask
//! is produced by the locator crate's own binarization — the same rule as
//! packages/web/src/shaders/binarize.wgsl — packed into a BitImage.

use barcode_core::BitImage;
use barcode_dm::locator::{binarize, Frame};

/// Minimal parser for the flat fixture {"w":N,"h":N,"luma":[...]}; no
/// serde dependency (crates stay dependency-free).
/// Personal-data fixture: local-only (gitignored), None when absent.
fn load_viva() -> Option<(usize, usize, Vec<u8>)> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/web/test-codes/field/viva-dm-luma.json"
    );
    let s = std::fs::read_to_string(path).ok()?;
    let dim = |key: &str| -> usize {
        let pat = format!("\"{}\":", key);
        let i = s.find(&pat).expect(key) + pat.len();
        let b = s.as_bytes();
        let mut j = i;
        while b[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut v = 0usize;
        while b[j].is_ascii_digit() {
            v = v * 10 + (b[j] - b'0') as usize;
            j += 1;
        }
        v
    };
    let (w, h) = (dim("w"), dim("h"));
    let li = s.find("\"luma\":").expect("luma") + "\"luma\":".len();
    let b = s.as_bytes();
    let mut j = li;
    while b[j] != b'[' {
        j += 1;
    }
    let mut luma = Vec::with_capacity(w * h);
    let mut v = 0usize;
    let mut in_num = false;
    while b[j] != b']' {
        match b[j] {
            b'0'..=b'9' => {
                v = v * 10 + (b[j] - b'0') as usize;
                in_num = true;
            }
            _ if in_num => {
                luma.push(v as u8);
                v = 0;
                in_num = false;
            }
            _ => {}
        }
        j += 1;
    }
    if in_num {
        luma.push(v as u8);
    }
    assert_eq!(luma.len(), w * h, "fixture luma size");
    Some((w, h, luma))
}

#[test]
fn decodes_datamatrix_from_packed_mask_tier() {
    let Some((w, h, luma)) = load_viva() else {
        eprintln!("skipped: field fixture not present locally");
        return;
    };
    let map = binarize(&Frame { w, h, luma: &luma });

    // pack into the GPU mask layout: bytesPerRow multiple of 4, set=ink
    let bpr = w.div_ceil(8).div_ceil(4) * 4;
    let mut mask = vec![0u8; bpr * h];
    for y in 0..h {
        for x in 0..w {
            if map.dark[y * w + x] {
                mask[y * bpr + (x >> 3)] |= 1 << (x & 7);
            }
        }
    }
    let img = BitImage::new(w as u32, h as u32, &mask);

    let hits = barcode_wasm::decode_datamatrix_mask(&img);
    assert_eq!(hits.len(), 1, "expected exactly one DM hit");
    assert_eq!(hits[0].symbology, barcode_core::Symbology::DataMatrix);
    if let Ok(expected) = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/web/test-codes/field/viva-dm-expected.txt"
    )) {
        let expected: Vec<u8> = expected
            .into_iter()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        assert_eq!(hits[0].text.as_bytes(), expected.as_slice());
    }
    // quad sanity: spans ~12 x 36 modules at pitch ~8 around (419, 711)
    let q: Vec<(f32, f32)> = hits[0].position.iter().map(|p| (p.x, p.y)).collect();
    let xs: Vec<f32> = q.iter().map(|p| p.0).collect();
    let ys: Vec<f32> = q.iter().map(|p| p.1).collect();
    let w_quad =
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    let h_quad =
        ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min);
    assert!((85.0..=110.0).contains(&w_quad), "quad width {w_quad}");
    assert!((270.0..=310.0).contains(&h_quad), "quad height {h_quad}");
    let cx = xs.iter().sum::<f32>() / 4.0;
    let cy = ys.iter().sum::<f32>() / 4.0;
    assert!(
        (cx - 419.0).abs() < 15.0 && (cy - 711.0).abs() < 15.0,
        "quad center ({cx},{cy})"
    );
}
