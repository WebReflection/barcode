//! Debug-only WASM exports (Cargo feature `debug`): stage-by-stage QR
//! diagnostics used by the browser harnesses. Never part of the published
//! decode surface.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use barcode_core::BitImage;
use barcode_qr::QrDecoder;
/// Stage-by-stage QR diagnosis for remote debugging: reports how far the
/// pipeline gets on one frame and what each stage produced. The finders
/// are (x, y, module) triples in detection order; `stage_failed` names the
/// first stage that returned nothing.
#[cfg(feature = "debug")]
#[derive(Serialize)]
struct QrDiag {
    finders: Option<Vec<[f32; 3]>>,
    grid: Option<GridInfo>,
    format: Option<[u8; 2]>,
    format_words: Option<[u16; 2]>,
    version: Option<u32>,
    codewords: Option<usize>,
    segments: Option<String>,
    stage_failed: String,
}

#[cfg(feature = "debug")]
#[derive(Serialize)]
struct GridInfo {
    size: u32,
    tl: [f32; 2],
    step_x: [f32; 2],
    step_y: [f32; 2],
}

#[cfg(feature = "debug")]
#[wasm_bindgen]
pub fn qr_debug_stages(width: usize, height: usize, data: &[u8]) -> Result<JsValue, JsValue> {
    let img = BitImage::new(width as u32, height as u32, data);
    let mut d = QrDecoder::new();
    let finders = d.find_finders(&img);
    let Some(finders) = finders else {
        return serde_wasm_bindgen::to_value(&QrDiag {
            finders: None, grid: None, format: None, format_words: None, version: None,
            codewords: None, segments: None, stage_failed: "finders".into(),
        }).map_err(|e| JsValue::from_str(&e.to_string()));
    };
    let finders_ser: Vec<[f32; 3]> =
        finders.iter().map(|f| [f.x, f.y, f.module]).collect();

    // Same labeling enumeration as decode().
    let perms = [[0usize, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    for perm in perms {
        let [tl, tr, bl] = [finders[perm[0]], finders[perm[1]], finders[perm[2]]];
        let Some(grid) = barcode_qr::grid::Grid::from_finders(&tl, &tr, &bl) else { continue };
        let Some((ec, mask)) = barcode_qr::grid::read_format(&img, &grid) else { continue };
        let version = (grid.size - 17) / 4;
        let vword = barcode_qr::grid::read_version(&img, &grid);
        let Some(codewords) = barcode_qr::codewords::read_codewords(&img, &grid, ec, mask) else {
            return serde_wasm_bindgen::to_value(&QrDiag {
                finders: Some(finders_ser),
                grid: Some(GridInfo {
                    size: grid.size,
                    tl: [grid.tl.0, grid.tl.1],
                    step_x: [grid.step_x.0, grid.step_x.1],
                    step_y: [grid.step_y.0, grid.step_y.1],
                }),
                format: Some([match ec { barcode_qr::grid::EcLevel::M => 0, barcode_qr::grid::EcLevel::L => 1, barcode_qr::grid::EcLevel::H => 2, barcode_qr::grid::EcLevel::Q => 3 }, mask]),
                format_words: Some([grid.format_word(&img, &barcode_qr::grid::COPY1), grid.format_word(&img, &barcode_qr::grid::copy2(grid.size))]),
                version: vword.or(Some(version)),
                codewords: None,
                segments: None,
                stage_failed: "codewords".into(),
            }).map_err(|e| JsValue::from_str(&e.to_string()));
        };
        let n = codewords.len();
        let Some(payload) = barcode_qr::segments::parse_segments(version, &codewords) else {
            return serde_wasm_bindgen::to_value(&QrDiag {
                finders: Some(finders_ser),
                grid: Some(GridInfo {
                    size: grid.size,
                    tl: [grid.tl.0, grid.tl.1],
                    step_x: [grid.step_x.0, grid.step_x.1],
                    step_y: [grid.step_y.0, grid.step_y.1],
                }),
                format: Some([match ec { barcode_qr::grid::EcLevel::M => 0, barcode_qr::grid::EcLevel::L => 1, barcode_qr::grid::EcLevel::H => 2, barcode_qr::grid::EcLevel::Q => 3 }, mask]),
                format_words: Some([grid.format_word(&img, &barcode_qr::grid::COPY1), grid.format_word(&img, &barcode_qr::grid::copy2(grid.size))]),
                version: vword.or(Some(version)),
                codewords: Some(n),
                segments: None,
                stage_failed: "segments".into(),
            }).map_err(|e| JsValue::from_str(&e.to_string()));
        };
        return serde_wasm_bindgen::to_value(&QrDiag {
            finders: Some(finders_ser),
            grid: Some(GridInfo {
                size: grid.size,
                tl: [grid.tl.0, grid.tl.1],
                step_x: [grid.step_x.0, grid.step_x.1],
                step_y: [grid.step_y.0, grid.step_y.1],
            }),
            format: Some([match ec { barcode_qr::grid::EcLevel::M => 0, barcode_qr::grid::EcLevel::L => 1, barcode_qr::grid::EcLevel::H => 2, barcode_qr::grid::EcLevel::Q => 3 }, mask]),
                format_words: Some([grid.format_word(&img, &barcode_qr::grid::COPY1), grid.format_word(&img, &barcode_qr::grid::copy2(grid.size))]),
            version: vword.or(Some(version)),
            codewords: Some(n),
            segments: Some(barcode_qr::segments::payload_text(&payload)),
            stage_failed: "none".into(),
        }).map_err(|e| JsValue::from_str(&e.to_string()));
    }
    serde_wasm_bindgen::to_value(&QrDiag {
        finders: Some(finders_ser),
        grid: None, format: None, format_words: None, version: None,
        codewords: None, segments: None, stage_failed: "grid".into(),
    }).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Brute-force (EC, mask) diagnostic: reports which combinations give
/// RS-valid codewords on the detected grid, with parsed text. The format
/// read should match the first entry.
#[cfg(feature = "debug")]
#[wasm_bindgen]
pub fn qr_brute_force_format(width: usize, height: usize, data: &[u8]) -> Result<JsValue, JsValue> {
    let img = BitImage::new(width as u32, height as u32, data);
    let mut d = QrDecoder::new();
    let Some(finders) = d.find_finders(&img) else {
        return Ok(JsValue::from_str("finders"));
    };
    #[derive(serde::Serialize)]
    struct Hit { fmt: String, text: Option<String> }
    let mut results: Vec<Vec<Hit>> = Vec::new();
    for perm in barcode_qr::LABELINGS {
        let [tl, tr, bl] = [finders[perm[0]], finders[perm[1]], finders[perm[2]]];
        // Every candidate version: the finder triangle fixes the symbol's
        // axes; the version only scales the pitch (span / (size - 7)).
        // Catches version mis-snaps (blur inflates the run-width estimate).
        for size in [21u32, 25, 29, 33, 37, 41] {
            let span = (size - 7) as f32;
            let grid = barcode_qr::grid::Grid {
                size,
                tl: (tl.x, tl.y),
                step_x: ((tr.x - tl.x) / span, (tr.y - tl.y) / span),
                step_y: ((bl.x - tl.x) / span, (bl.y - tl.y) / span),
                h: None,
            };
            let hits = barcode_qr::codewords::brute_force_format(&img, &grid);
            if hits.is_empty() {
                continue;
            }
            results.push(hits
                .iter()
                .map(|((ec, mask), text)| {
                    let e = match ec {
                        barcode_qr::grid::EcLevel::L => "L",
                        barcode_qr::grid::EcLevel::M => "M",
                        barcode_qr::grid::EcLevel::Q => "Q",
                        barcode_qr::grid::EcLevel::H => "H",
                    };
                    Hit { fmt: format!("v{size} {e}/{mask}"), text: text.clone() }
                })
                .collect());
        }
    }
    serde_wasm_bindgen::to_value(&results).map_err(|e| JsValue::from_str(&e.to_string()))
}
