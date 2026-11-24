//! Rotation sweep probe for the still AND live paths: DM / Aztec / PDF417
//! at 0/45/90/135/180/270/315°. Temporary diagnostic; not part of the
//! published API.

use barcode_core::BitImage;
use barcode_fixtures::Canvas;
use barcode_wasm::{BarcodeScanner, Tiers};

fn render(matrix: &[u8], rows: usize, cols: usize, module: u32, margin: u32) -> Canvas {
    // Square canvas big enough for the 45° rotated symbol.
    let side = ((rows.max(cols) as f64 * 1.45).ceil() as u32 + 2 * margin) * module;
    let mut c = Canvas::new(side, side);
    let ox = (side / module - cols as u32) / 2;
    let oy = (side / module - rows as u32) / 2;
    for y in 0..rows {
        for x in 0..cols {
            if matrix[y * cols + x] == 1 {
                c.fill(
                    (ox + x as u32) * module,
                    (oy + y as u32) * module,
                    module,
                    module,
                );
            }
        }
    }
    c
}

const ANGLES: [f64; 7] = [0.0, 45.0, 90.0, 135.0, 180.0, 270.0, 315.0];

fn main() {
    let module = std::env::var("ROT_MODULE")
        .map(|s| s.parse().unwrap())
        .unwrap_or(5u32);
    let margin = 4u32;
    let dm = barcode_dm::encode::encode(b"ROT-DM-TEST").unwrap();
    let dm_c = render(&dm.matrix, dm.spec.rows, dm.spec.cols, module, margin);
    let az = barcode_aztec::encode::encode(b"ROT-AZTEC-TEST").unwrap();
    let az_c = render(
        &az.1,
        az.0.rows as usize,
        az.0.cols as usize,
        module,
        margin,
    );
    let pf = barcode_pdf417::encode::encode_full(b"ROT-PDF417-TEST", 2).unwrap();
    let pf_c = render(&pf.matrix, pf.height, pf.width, module, margin);

    let cases: [(&str, &Canvas, Tiers, &str); 3] = [
        (
            "datamatrix",
            &dm_c,
            Tiers {
                datamatrix: true,
                ..Default::default()
            },
            "ROT-DM-TEST",
        ),
        (
            "aztec",
            &az_c,
            Tiers {
                aztec: true,
                ..Default::default()
            },
            "ROT-AZTEC-TEST",
        ),
        (
            "pdf417",
            &pf_c,
            Tiers {
                pdf417: true,
                ..Default::default()
            },
            "ROT-PDF417-TEST",
        ),
    ];
    for (name, canvas, tiers, want) in cases {
        let img = BitImage::new(canvas.width, canvas.height, canvas.bits());
        for (law, still) in [("still", true), ("live", false)] {
            print!("{name:12}{law:6}");
            for deg in ANGLES {
                let rot = img.rotated(deg);
                let mut sc = BarcodeScanner::new();
                let t0 = std::time::Instant::now();
                let hits = if still {
                    sc.decode_still_core(
                        canvas.width as usize,
                        canvas.height as usize,
                        &rot,
                        tiers,
                        4000,
                    )
                } else {
                    sc.decode_live_core(canvas.width as usize, canvas.height as usize, &rot, tiers)
                };
                let ok = hits.iter().any(|h| h.text == want);
                print!(
                    " {:>3.0}°:{}{:>8?}",
                    deg,
                    if ok { "ok" } else { "--" },
                    t0.elapsed()
                );
            }
            println!();
        }
    }
}
