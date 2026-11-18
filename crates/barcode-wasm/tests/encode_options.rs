//! Layout options for `render()`: explicit QR EC/mask/version, PDF417
//! EC level/columns and a Data Matrix of an explicit ECC200 size must
//! produce a valid, decodable symbol — and the options must actually
//! change the matrix/size (not be silently ignored).

use barcode_core::BitImage;
use barcode_fixtures::Canvas;
use barcode_wasm::{decode_datamatrix_mask_budget, decode_pdf417_mask, BarcodeScanner, Tiers};

/// Render a module matrix with a quiet zone (2D families).
fn render(matrix: &[u8], rows: usize, cols: usize, module: u32, quiet: u32) -> Canvas {
    let mut c = Canvas::new(
        (cols as u32 + 2 * quiet) * module,
        (rows as u32 + 2 * quiet) * module,
    );
    for y in 0..rows {
        for x in 0..cols {
            if matrix[y * cols + x] == 1 {
                c.fill(
                    (quiet + x as u32) * module,
                    (quiet + y as u32) * module,
                    module,
                    module,
                );
            }
        }
    }
    c
}

fn qr_texts(c: &Canvas) -> Vec<String> {
    let mut sc = BarcodeScanner::new();
    sc.decode_still_core(
        c.width as usize,
        c.height as usize,
        c.bits(),
        Tiers {
            qr: true,
            ..Default::default()
        },
        4000,
    )
    .into_iter()
    .map(|h| h.text)
    .collect()
}

#[test]
fn qr_ec_mask_and_version_are_honoured() {
    let text = "OPTIONS-QR-H";
    // Every EC level × a spread of mask patterns must still read.
    for (ec, label) in [(0u8, "M"), (1, "L"), (2, "H"), (3, "Q")] {
        for mask in [0u8, 2, 5, 7] {
            let c = barcode_fixtures::encode_qr_text_version(text, ec, mask, 0, 5)
                .unwrap_or_else(|| panic!("{label} mask {mask}: payload should fit"));
            assert!(
                qr_texts(&c).iter().any(|t| t == text),
                "{label} mask {mask}: did not decode"
            );
        }
    }

    // The options must change the artwork, not be ignored. (The decoded
    // value is identical, as it should be.)
    let m = barcode_fixtures::encode_qr_text_version(text, 0, 2, 5, 5).unwrap();
    let h = barcode_fixtures::encode_qr_text_version(text, 2, 5, 5, 5).unwrap();
    assert_ne!(m.bits(), h.bits(), "EC/mask options changed nothing");

    // Forced version: v5 = 37 modules, v10 = 57; both read.
    let v5 = barcode_fixtures::encode_qr_text_version(text, 2, 5, 5, 4).unwrap();
    let v10 = barcode_fixtures::encode_qr_text_version(text, 2, 5, 10, 4).unwrap();
    assert_eq!(v5.width, (17 + 4 * 5 + 12) * 4);
    assert_eq!(v10.width, (17 + 4 * 10 + 12) * 4);
    assert!(qr_texts(&v5).iter().any(|t| t == text));
    assert!(qr_texts(&v10).iter().any(|t| t == text));
}

#[test]
fn pdf417_level_and_columns_are_honoured() {
    let text = "OPTIONS-PDF417";
    for level in 0..=8u8 {
        let sym = barcode_pdf417::encode::encode_full(text.as_bytes(), level)
            .unwrap_or_else(|_| panic!("level {level}: payload should fit"));
        let c = render(&sym.matrix, sym.height, sym.width, 3, 8);
        let img = BitImage::new(c.width, c.height, c.bits());
        assert!(
            decode_pdf417_mask(&img).iter().any(|h| h.text == text),
            "level {level}: did not decode"
        );
    }
    for cols in [2u8, 3, 6] {
        let sym = barcode_pdf417::encode::encode_full_opts(b"HI", 2, cols).expect("fits");
        assert_eq!(sym.cols, cols as usize, "columns override not applied");
        let c = render(&sym.matrix, sym.height, sym.width, 3, 8);
        let img = BitImage::new(c.width, c.height, c.bits());
        assert!(
            decode_pdf417_mask(&img).iter().any(|h| h.text == "HI"),
            "columns {cols}: did not decode"
        );
    }
}

#[test]
fn datamatrix_size_is_honoured() {
    for (rows, cols) in [(12usize, 36usize), (16, 48), (20, 20), (24, 24)] {
        let enc = barcode_dm::encode::encode_as(b"OPTIONS-DM", rows, cols)
            .unwrap_or_else(|| panic!("{rows}x{cols}: payload should fit"));
        assert_eq!((enc.spec.rows, enc.spec.cols), (rows, cols));
        let c = render(&enc.matrix, rows, cols, 4, 4);
        let img = BitImage::new(c.width, c.height, c.bits());
        assert!(
            decode_datamatrix_mask_budget(&img, true, 5000)
                .iter()
                .any(|h| h.text == "OPTIONS-DM"),
            "{rows}x{cols}: did not decode"
        );
    }
}
