//! Render surface: the synthetic specimen builders behind `render()` —
//! text/bytes payloads for every supported symbology, serialized as a
//! packed 1-bit mask for the JS side.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use barcode_code39::code32_raw;
use barcode_fixtures::Canvas;
/// Smallest Micro QR tier that accepts `text`, rendered with a 4-module
/// quiet zone. Shared by the fixture arm and the round-trip test.
pub(crate) fn microqr_canvas(text: &str, module: u32) -> Result<Canvas, String> {
    if module == 0 || module > 64 {
        return Err("module must be 1..=64".into());
    }
    use barcode_microqr::{encode, Level, Version};
    const TIERS: [(Version, Level); 8] = [
        (Version::M1, Level::L),
        (Version::M2, Level::L),
        (Version::M2, Level::M),
        (Version::M3, Level::L),
        (Version::M3, Level::M),
        (Version::M4, Level::L),
        (Version::M4, Level::M),
        (Version::M4, Level::Q),
    ];
    let sym = TIERS
        .iter()
        .find_map(|&(v, l)| encode(text, v, l, None))
        .ok_or_else(|| "payload too large for Micro QR".to_string())?;
    let quiet = 4u32;
    let dim = (sym.size as u32 + 2 * quiet) * module;
    let mut c = Canvas::new(dim, dim);
    for y in 0..sym.size {
        for x in 0..sym.size {
            if sym.cells[y * sym.size + x] == 1 {
                c.fill(
                    (quiet + x as u32) * module,
                    (quiet + y as u32) * module,
                    module,
                    module,
                );
            }
        }
    }
    Ok(c)
}

/// Draw a `#`/`.` oracle matrix (1 char = 1 module) onto a Canvas with a
/// `quiet`-module margin. Used by the frozen-specimen fixture tests.
#[cfg(test)]
pub(crate) fn matrix_canvas(
    pattern: &str,
    rows: u32,
    cols: u32,
    module: u32,
    quiet: u32,
) -> Canvas {
    let mut c = Canvas::new((cols + 2 * quiet) * module, (rows + 2 * quiet) * module);
    for (y, line) in pattern.split('\n').enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if ch == '#' {
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

/// Encoder options at the wasm boundary. Sentinel values keep every
/// option optional and preserve the historical behavior: `-1` / `0`
/// means "encoder default". wasm-bindgen bridges primitives cheaply;
/// the JS `render()` options object is normalized into these.
#[derive(Clone, Copy)]
pub(crate) struct EncOptions {
    /// QR raw 2-bit EC field: M=0, L=1, H=2, Q=3; -1 = M.
    ec: i32,
    /// QR mask pattern 0..7; -1 = 2 (the historical default).
    mask: i32,
    /// QR version 1..40; 0 = smallest that fits.
    version: i32,
    /// PDF417 EC level 0..8; -1 = the encoder default.
    level: i32,
    /// PDF417 data columns 1..30; 0 = auto.
    columns: i32,
    /// Data Matrix ECC200 size in modules INCLUDING borders, e.g. a
    /// 12x36 DMRE (0/0 = the smallest square that fits).
    rows: i32,
    cols: i32,
}

impl Default for EncOptions {
    fn default() -> Self {
        Self {
            ec: -1,
            mask: -1,
            version: 0,
            level: -1,
            columns: 0,
            rows: 0,
            cols: 0,
        }
    }
}

impl EncOptions {
    fn validate(self) -> Result<Self, String> {
        if !(-1..=3).contains(&self.ec) {
            return Err("ec must be L, M, Q or H".into());
        }
        if !(-1..=7).contains(&self.mask) {
            return Err("mask must be 0..7".into());
        }
        if !(0..=40).contains(&self.version) {
            return Err("version must be 1..40".into());
        }
        if !(-1..=8).contains(&self.level) {
            return Err("level must be 0..8 (PDF417)".into());
        }
        if !(0..=30).contains(&self.columns) {
            return Err("columns must be 1..30 (PDF417)".into());
        }
        if self.rows < 0 || self.cols < 0 || (self.rows == 0) != (self.cols == 0) {
            return Err("rows and cols must both be set, or both 0".into());
        }
        Ok(self)
    }
    fn qr_ec(&self) -> u8 {
        if self.ec < 0 {
            0
        } else {
            self.ec as u8
        }
    }
    fn qr_mask(&self) -> u8 {
        if self.mask < 0 {
            2
        } else {
            self.mask as u8
        }
    }
    fn qr_version(&self) -> u32 {
        self.version.max(0) as u32
    }
    fn pdf_level(&self) -> u8 {
        if self.level < 0 {
            barcode_pdf417::encode::DEFAULT_LEVEL
        } else {
            self.level as u8
        }
    }
}

/// Render a synthetic specimen of `symbology` for the self-test harness.
/// Returns the 1bpp mask { width, height, bytesPerRow, data } in the same
/// layout the GPU binarizer produces, so the harness can round-trip it
/// through the exact video pipeline.
///
/// This is the no-options form. [`encode_fixture_opts`] is the same
/// builder with QR/PDF417/DataMatrix layout options.
#[cfg(feature = "debug")]
#[wasm_bindgen]
pub fn encode_fixture(
    symbology: &str,
    text: &str,
    module: u32,
    height: u32,
) -> Result<JsValue, JsValue> {
    encode_fixture_opts(symbology, text, module, height, -1, -1, 0, -1, 0, 0, 0)
}

/// Render a synthetic specimen with layout options. Every option keeps a
/// sane default when sent as `-1`/`0`:
///
/// * `ec`     QR EC (`M=0, L=1, H=2, Q=3`; -1 = M)
/// * `mask`   QR mask pattern 0..7 (-1 = 2)
/// * `version` QR version 1..40 (0 = smallest that fits)
/// * `level`  PDF417 EC level 0..8 (-1 = encoder default)
/// * `columns` PDF417 data columns 1..30 (0 = auto)
/// * `rows`/`cols` DataMatrix ECC200 size incl. borders (0/0 = smallest)
#[wasm_bindgen]
pub fn encode_fixture_opts(
    symbology: &str,
    text: &str,
    module: u32,
    height: u32,
    ec: i32,
    mask: i32,
    version: i32,
    level: i32,
    columns: i32,
    rows: i32,
    cols: i32,
) -> Result<JsValue, JsValue> {
    let o = EncOptions {
        ec,
        mask,
        version,
        level,
        columns,
        rows,
        cols,
    }
    .validate()
    .map_err(|e| JsValue::from_str(&e))?;
    let canvas = encode_text_canvas(symbology, text, module, height, o)
        .map_err(|e| JsValue::from_str(&e))?;
    fixture_enc(canvas)
}

/// Panic-free text specimen builder (natively testable, returns `Result<Canvas, String>`).
pub(crate) fn encode_text_canvas(
    symbology: &str,
    text: &str,
    module: u32,
    height: u32,
    o: EncOptions,
) -> Result<Canvas, String> {
    if module == 0 || module > 64 || height == 0 || height > 8192 {
        return Err("invalid render dimensions".into());
    }
    if text.is_empty() {
        return Err("empty payload".into());
    }
    let quiet = 10 * module;
    let parse_databar13 = || -> Result<u64, &'static str> {
        if text.len() != 13 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err("invalid databar payload");
        }
        text.parse::<u64>().map_err(|_| "invalid databar payload")
    };
    let canvas = match symbology {
        "ean13" => barcode_fixtures::try_encode_ean13(text, module, height, quiet)?,
        "code39" => barcode_fixtures::try_encode_code39(text, module, 3.0, height)?,
        "code32" => {
            let raw = code32_raw(text).ok_or("invalid code32 payload")?;
            barcode_fixtures::try_encode_code39(&raw, module, 3.0, height)?
        }
        "itf" => barcode_fixtures::try_encode_itf(text, module, 2.5, height)?,
        "code128" => barcode_fixtures::try_encode_code128(text, module, height)?,
        "code93" => barcode_fixtures::try_encode_code93(text, module, height)?,
        "codabar" => barcode_fixtures::try_encode_codabar(text, module, 2.5, height)?,
        "ean8" => barcode_fixtures::try_encode_ean8(text, module, height, quiet)?,
        "upce" => barcode_fixtures::try_encode_upce(text, module, height, quiet)?,
        "upca" => barcode_fixtures::try_encode_upca(text, module, height, quiet)?,
        "databar" | "databaromni" => {
            let v = parse_databar13()?;
            let runs = barcode_fixtures::try_databar_omni_runs(v)?;
            barcode_fixtures::databar_omni_canvas(&runs, module, height, quiet)
        }
        "databarltd" | "databarlimited" => {
            let v = parse_databar13()?;
            let runs = barcode_fixtures::try_databar_ltd_runs(v)?;
            barcode_fixtures::databar_ltd_canvas(&runs, module, height, quiet)
        }
        "databarstk" | "databarstacked" => {
            let v = parse_databar13()?;
            let runs = barcode_fixtures::try_databar_omni_runs(v)?;
            barcode_fixtures::databar_stacked_canvas(&runs, module, quiet, false)
        }
        "databarstackomni" => {
            let v = parse_databar13()?;
            let runs = barcode_fixtures::try_databar_omni_runs(v)?;
            barcode_fixtures::databar_stacked_canvas(&runs, module, quiet, true)
        }
        "databarexpanded" | "databarexp" | "gs1databarexpanded" | "rssexpanded" => {
            barcode_fixtures::databar_exp::encode_databarexpanded(text, module, height)?
        }
        // QR: EC M / mask 2 / smallest version by default; all three are
        // overridable. `height` is meaningless for 2D and ignored.
        "qr" | "qrcode" => barcode_fixtures::encode_qr_text_version(
            text,
            o.qr_ec(),
            o.qr_mask(),
            o.qr_version(),
            module,
        )
        .ok_or_else(|| "payload too large for QR at the requested version/EC".to_string())?,
        // Micro QR: algorithmic ISO/IEC 18004 encoder in barcode-microqr.
        "microqr" => microqr_canvas(text, module)?,
        // Datamatrix: the real ECC200 encoder.
        "datamatrix" => {
            let sized = o.rows > 0;
            let enc = (if sized {
                barcode_dm::encode::encode_as(text.as_bytes(), o.rows as usize, o.cols as usize)
            } else {
                barcode_dm::encode::encode(text.as_bytes())
            })
            .ok_or_else(|| {
                if sized {
                    "payload does not fit the requested DataMatrix size".to_string()
                } else {
                    "payload too large for DataMatrix".to_string()
                }
            })?;
            let (r, c) = (enc.spec.rows, enc.spec.cols);
            let mut canvas = Canvas::new((c as u32 + 8) * module, (r as u32 + 8) * module);
            for y in 0..r {
                for x in 0..c {
                    if enc.matrix[y * c + x] == 1 {
                        canvas.fill(
                            (4 + x as u32) * module,
                            (4 + y as u32) * module,
                            module,
                            module,
                        );
                    }
                }
            }
            canvas
        }
        // Aztec: the real encoder.
        "aztec" => {
            let (spec, matrix) = barcode_aztec::encode::encode(text.as_bytes())
                .map_err(|_| "payload too large for Aztec".to_string())?;
            let (r, c) = (spec.rows as usize, spec.cols as usize);
            let mut canvas = Canvas::new((c as u32 + 8) * module, (r as u32 + 8) * module);
            for y in 0..r {
                for x in 0..c {
                    if matrix[y * c + x] == 1 {
                        canvas.fill(
                            (4 + x as u32) * module,
                            (4 + y as u32) * module,
                            module,
                            module,
                        );
                    }
                }
            }
            canvas
        }
        // PDF417: the real encoder.
        "pdf417" => {
            let level = o.pdf_level();
            let sym = (if o.columns > 0 {
                barcode_pdf417::encode::encode_full_opts(text.as_bytes(), level, o.columns as u8)
            } else {
                barcode_pdf417::encode::encode_full(text.as_bytes(), level)
            })
            .map_err(|_| {
                "payload too large for PDF417 at the requested level/columns".to_string()
            })?;
            let mut canvas = Canvas::new(sym.width as u32 * module, sym.height as u32 * module);
            for y in 0..sym.height {
                for x in 0..sym.width {
                    if sym.matrix[y * sym.width + x] == 1 {
                        canvas.fill(x as u32 * module, y as u32 * module, module, module);
                    }
                }
            }
            canvas
        }
        _ => return Err(format!("unknown symbology: {symbology}")),
    };
    Ok(canvas)
}

/// Shared `Enc` serialization for the fixture builders.
pub(crate) fn fixture_enc(canvas: Canvas) -> Result<JsValue, JsValue> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Enc {
        width: u32,
        height: u32,
        bytes_per_row: usize,
        data: Vec<u8>,
    }
    let enc = Enc {
        width: canvas.width,
        height: canvas.height,
        bytes_per_row: canvas.bytes_per_row,
        data: canvas.bits().to_vec(),
    };
    serde_wasm_bindgen::to_value(&enc).map_err(|_| JsValue::from_str("serialize failed"))
}

/// Binary specimen canvas builder shared by `encode_fixture_hex`,
/// `encode_fixture_raw`, and `encode_fixture_raw_opts`. Returns `Result<Canvas, String>`
/// so it is panic-free and natively testable.
pub(crate) fn fixture_bytes_canvas(
    symbology: &str,
    data: &[u8],
    module: u32,
    o: EncOptions,
) -> Result<Canvas, String> {
    if module == 0 || module > 64 {
        return Err("module must be 1..=64".into());
    }
    if data.is_empty() {
        return Err("empty payload".into());
    }
    Ok(match symbology {
        "qr" | "qrcode" => {
            if data.len() > 2953 {
                return Err("payload too large for QR at the requested version/EC".into());
            }
            let ec = o.qr_ec();
            let mask = o.qr_mask();
            let forced = o.qr_version();
            let mut found = None;
            let versions = if (1..=40).contains(&forced) {
                forced..=forced
            } else {
                1..=40
            };
            for version in versions {
                let cap = barcode_fixtures::qr_data_codewords(version, ec) as usize;
                let count_bits = if version <= 9 { 8 } else { 16 };
                let needed = (4 + count_bits + 8 * data.len() + 4).div_ceil(8);
                if needed <= cap {
                    found = Some((version, cap, count_bits));
                    break;
                }
            }
            let Some((version, cap, count_bits)) = found else {
                return Err("payload too large for QR at the requested version/EC".into());
            };
            let mut bits: Vec<bool> = vec![false, true, false, false]; // mode 4
            for i in (0..count_bits).rev() {
                bits.push((data.len() >> i) & 1 == 1);
            }
            for &b in data {
                for i in (0..8).rev() {
                    bits.push((b >> i) & 1 == 1);
                }
            }
            while bits.len() % 8 != 0 {
                bits.push(false);
            }
            let mut stream: Vec<u8> = bits
                .chunks(8)
                .map(|c| c.iter().fold(0u8, |a, &b| (a << 1) | b as u8))
                .collect();
            let mut pad = true;
            while stream.len() < cap {
                stream.push(if pad { 0xEC } else { 0x11 });
                pad = !pad;
            }
            barcode_fixtures::encode_qr(version, ec, mask, &stream, module)
        }
        "datamatrix" => {
            let sized = o.rows > 0;
            let enc = (if sized {
                barcode_dm::encode::encode_as(data, o.rows as usize, o.cols as usize)
            } else {
                barcode_dm::encode::encode(data)
            })
            .ok_or_else(|| {
                if sized {
                    "payload does not fit the requested DataMatrix size".to_string()
                } else {
                    "payload too large for DataMatrix".to_string()
                }
            })?;
            let (r, c) = (enc.spec.rows, enc.spec.cols);
            let mut canvas = Canvas::new((c as u32 + 8) * module, (r as u32 + 8) * module);
            for y in 0..r {
                for x in 0..c {
                    if enc.matrix[y * c + x] == 1 {
                        canvas.fill(
                            (4 + x as u32) * module,
                            (4 + y as u32) * module,
                            module,
                            module,
                        );
                    }
                }
            }
            canvas
        }
        "aztec" => {
            let (spec, matrix) = barcode_aztec::encode::encode(data)
                .map_err(|_| "payload too large for Aztec".to_string())?;
            let (r, c) = (spec.rows as usize, spec.cols as usize);
            let mut canvas = Canvas::new((c as u32 + 8) * module, (r as u32 + 8) * module);
            for y in 0..r {
                for x in 0..c {
                    if matrix[y * c + x] == 1 {
                        canvas.fill(
                            (4 + x as u32) * module,
                            (4 + y as u32) * module,
                            module,
                            module,
                        );
                    }
                }
            }
            canvas
        }
        "pdf417" => {
            let level = o.pdf_level();
            let sym = (if o.columns > 0 {
                barcode_pdf417::encode::encode_full_opts(data, level, o.columns as u8)
            } else {
                barcode_pdf417::encode::encode_full(data, level)
            })
            .map_err(|_| {
                "payload too large for PDF417 at the requested level/columns".to_string()
            })?;
            let mut canvas = Canvas::new(sym.width as u32 * module, sym.height as u32 * module);
            for y in 0..sym.height {
                for x in 0..sym.width {
                    if sym.matrix[y * sym.width + x] == 1 {
                        canvas.fill(x as u32 * module, y as u32 * module, module, module);
                    }
                }
            }
            canvas
        }
        "microqr" => {
            use barcode_microqr::{encode_bytes, Level, Mode, Version};
            const TIERS: [(Version, Level); 8] = [
                (Version::M1, Level::L),
                (Version::M2, Level::L),
                (Version::M2, Level::M),
                (Version::M3, Level::L),
                (Version::M3, Level::M),
                (Version::M4, Level::L),
                (Version::M4, Level::M),
                (Version::M4, Level::Q),
            ];
            let sym = TIERS
                .iter()
                .find_map(|&(v, l)| encode_bytes(data, v, l, Some(Mode::Byte), None))
                .ok_or_else(|| "payload too large for Micro QR".to_string())?;
            let quiet = 4u32;
            let dim = (sym.size as u32 + 2 * quiet) * module;
            let mut c = Canvas::new(dim, dim);
            for y in 0..sym.size {
                for x in 0..sym.size {
                    if sym.cells[y * sym.size + x] == 1 {
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
        _ => {
            return Err(format!("binary render not supported for {symbology}"));
        }
    })
}

/// Render a synthetic binary specimen (hex payload) for the self-test
/// harness — same mask layout as `encode_fixture`.
#[cfg(feature = "debug")]
#[wasm_bindgen]
pub fn encode_fixture_hex(
    symbology: &str,
    hex: &str,
    module: u32,
    _height: u32,
) -> Result<JsValue, JsValue> {
    if hex.len() % 2 != 0 || hex.bytes().any(|b| !b.is_ascii_hexdigit()) {
        return Err(JsValue::from_str("bad hex payload"));
    }
    let data: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
        .collect::<Result<_, _>>()
        .map_err(|_| JsValue::from_str("bad hex payload"))?;
    let canvas = fixture_bytes_canvas(symbology, &data, module, EncOptions::default())
        .map_err(|e| JsValue::from_str(&e))?;
    fixture_enc(canvas)
}

/// Render an exact byte payload — the encode half of `Code.raw`. Same
/// mask layout as `encode_fixture`; symbologies without binary encoders
/// error here (the caller falls back to the text path).
#[cfg(feature = "debug")]
#[wasm_bindgen]
pub fn encode_fixture_raw(
    symbology: &str,
    raw: &[u8],
    module: u32,
    _height: u32,
) -> Result<JsValue, JsValue> {
    let canvas = fixture_bytes_canvas(symbology, raw, module, EncOptions::default())
        .map_err(|e| JsValue::from_str(&e))?;
    fixture_enc(canvas)
}

/// [`encode_fixture_raw`] with the same layout options as
/// [`encode_fixture_opts`] (binary payload: QR byte mode, DM/Aztec/
/// PDF417 take the exact bytes).
#[wasm_bindgen]
pub fn encode_fixture_raw_opts(
    symbology: &str,
    raw: &[u8],
    module: u32,
    _height: u32,
    ec: i32,
    mask: i32,
    version: i32,
    level: i32,
    columns: i32,
    rows: i32,
    cols: i32,
) -> Result<JsValue, JsValue> {
    let o = EncOptions {
        ec,
        mask,
        version,
        level,
        columns,
        rows,
        cols,
    }
    .validate()
    .map_err(|e| JsValue::from_str(&e))?;
    let canvas =
        fixture_bytes_canvas(symbology, raw, module, o).map_err(|e| JsValue::from_str(&e))?;
    fixture_enc(canvas)
}
