//! WASM bridge for the web SDK.
//!
//! Mask layout contract with `packages/web/src/gpu/binarizer.js`:
//! 1 bit per pixel, bit (x, y) = data[y*bytesPerRow + (x>>3)] >> (x&7) & 1,
//! bytesPerRow a multiple of 4, set bit = ink. That is exactly
//! `barcode_core::BitImage`.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use barcode_code128::Code128Decoder;
use barcode_core::Point;
use barcode_codabar::CodabarDecoder;
use barcode_ean8::Ean8Decoder;
use barcode_qr::QrDecoder;
use barcode_microqr::decode_microqr;
use barcode_upce::UpceDecoder;
use barcode_code93::Code93Decoder;
use barcode_code39::{Code39Decoder, code32_text};
use barcode_core::{BitImage, LinearDecoder, RowRuns, Symbology, SymbologyDecoder};
use barcode_ean13::Ean13Decoder;
use barcode_itf::ItfDecoder;
use barcode_databar::{DataBarExpandedDecoder, DataBarLtdDecoder, DataBarOmniDecoder, DataBarStackedDecoder};

#[cfg(feature = "debug")]
mod debug;
mod masks;
mod render;
#[cfg(test)]
mod unit_tests;

pub use masks::*;
/// Milliseconds on the host clock. The still-path deadline needs WALL time:
/// sample budgets miss the fixed per-step costs (rotations, locator setup,
/// texture-scale conversions) that dominate on junk-rich big frames.
#[cfg(target_arch = "wasm32")]
fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Serialize)]
struct WasmResult {
    text: String,
    /// Exact payload bytes (binary QR byte mode etc.) — JS Uint8Array at
    /// the boundary.
    raw: Vec<u8>,
    symbology: String,
    /// x0, y0, x1, y1, x2, y2, x3, y3
    quad: [f32; 8],
}

#[wasm_bindgen]
pub struct BarcodeScanner {
    ean13: Ean13Decoder,
    code39: Code39Decoder,
    itf: ItfDecoder,
    code128: Code128Decoder,
    code93: Code93Decoder,
    codabar: CodabarDecoder,
    ean8: Ean8Decoder,
    upce: UpceDecoder,
    /// 2D tier: full chain (finders → grid → format → RS → segments).
    qr: QrDecoder,
    databar: DataBarOmniDecoder,
    databarltd: DataBarLtdDecoder,
    databarstk: DataBarStackedDecoder,
    databarexpanded: DataBarExpandedDecoder,
    /// Remaining live frames that should try inverted polarity first.
    /// Set on a live inverted read so the confirmation close-up (~46% ink,
    /// below the full-frame heuristic) stays cheap, decays on live misses,
    /// clears on normal hits, and is ignored/untouched by still scans so
    /// polarity never leaks across unrelated scans.
    inverted_ttl: u8,
}

#[wasm_bindgen]
impl BarcodeScanner {
    #[wasm_bindgen(constructor)]
    pub fn new() -> BarcodeScanner {
        BarcodeScanner {
            ean13: Ean13Decoder::new(2),
            code39: Code39Decoder::new(2),
            itf: ItfDecoder::new(2),
            code128: Code128Decoder::new(2),
            code93: Code93Decoder::new(2),
            codabar: CodabarDecoder::new(2),
            ean8: Ean8Decoder::new(2),
            upce: UpceDecoder::new(2),
            qr: QrDecoder::new(),
            databar: DataBarOmniDecoder::new(2),
            databarltd: DataBarLtdDecoder::new(2),
            databarstk: DataBarStackedDecoder::new(1),
            databarexpanded: DataBarExpandedDecoder::new(2),
            inverted_ttl: 0,
        }
    }

    /// Decode all supported symbologies from one binarized frame.
    ///
    /// Tiered: the 2D decoder runs first; the 1D tier only runs when 2D
    /// scored nothing. A QR's texture reliably goads loose 1D symbologies
    /// (Codabar especially) into false positives, so giving 1D a vote on
    /// frames that contain a 2D code is how junk results are born — the
    /// tiers are mutually exclusive per frame by design.
    pub fn decode(&mut self, width: usize, height: usize, data: &[u8]) -> Result<JsValue, JsValue> {
        self.decode_tiered(width, height, data, &Tiers::all(), false, 0)
    }

    /// Still-image decode (gallery / photo picker): same tiers, but the
    /// DM law is the `budget_ms` deadline (default 5000) and there is no
    /// confirmation to amortize.
    pub fn decode_still(
        &mut self,
        width: usize,
        height: usize,
        data: &[u8],
        budget_ms: u32,
    ) -> Result<JsValue, JsValue> {
        self.decode_tiered(width, height, data, &Tiers::all(), true, u64::from(budget_ms))
    }


    /// Decode a filtered subset: `formats` names symbologies in the
    /// normalized lowercase form ("qrcode", "ean13", "code128", ...) —
    /// the same strings results carry. Unknown names throw: a typo must
    /// fail loudly, not silently scan nothing. Omitted tiers simply don't
    /// run; the 2D-first rule applies to whichever tiers remain.
    ///
    /// This is a *decode-tier* selection only — it never changes capture
    /// behavior (lens/zoom/torch live in the camera constraints).
    pub fn decode_filtered(
        &mut self,
        width: usize,
        height: usize,
        data: &[u8],
        formats: Vec<String>,
    ) -> Result<JsValue, JsValue> {
        let tiers = parse_formats(&formats).map_err(|e| JsValue::from_str(&e))?;
        self.decode_tiered(width, height, data, &tiers, false, 0)
    }

    /// Filtered still-image decode: `formats` as in `decode_filtered`,
    /// budget law `budget_ms` (default 5000).
    pub fn decode_filtered_still(
        &mut self,
        width: usize,
        height: usize,
        data: &[u8],
        formats: Vec<String>,
        budget_ms: u32,
    ) -> Result<JsValue, JsValue> {
        let tiers = parse_formats(&formats).map_err(|e| JsValue::from_str(&e))?;
        self.decode_tiered(width, height, data, &tiers, true, u64::from(budget_ms))
    }

    fn decode_tiered(
        &mut self,
        width: usize,
        height: usize,
        data: &[u8],
        tiers: &Tiers,
        still: bool,
        still_budget_ms: u64,
    ) -> Result<JsValue, JsValue> {
        let img = BitImage::new(width as u32, height as u32, data);
        let results = self.scan_tiered(&img, tiers, still, still_budget_ms);
        let out: Vec<WasmResult> = results.iter().map(|r| WasmResult::of(r.clone())).collect();
        // Union search continues; if frame budgets tighten, give the 2D
        // decoders per-frame slices.
        serde_wasm_bindgen::to_value(&out).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// QR presence signal from the LAST frame the QR tier ran on:
    /// 0 = no finder patterns in view, 1 = a QR is present but did not
    /// decode (marginal — the auto-framing policy's torch trigger),
    /// 2 = decoded. Stale unless the QR tier ran this frame.
    pub fn qr_presence(&self) -> u8 {
        self.qr.presence()
    }

    /// Tier selection over one bitmap, producing plain results — the
    /// testable core of both `decode` entry points.
    /// One sweep, then the inverted-polarity second opinion.
    ///
    /// A white-on-black code reaches the scanner as an inverted mask (the
    /// code's background IS the ink). The normal polarity is always tried
    /// first — a dense PDF417 legitimately exceeds 50% ink, so the
    /// fraction must not pick the primary polarity. On failure, a mask
    /// that is mostly ink gets the inverted sweep on the live path; a
    /// still image always gets it (a gallery photo may frame the code
    /// with white paper, keeping the fraction mid-range) with half the
    /// caller's budget.
    fn scan_tiered(
        &mut self,
        img: &BitImage,
        tiers: &Tiers,
        still: bool,
        still_budget_ms: u64,
    ) -> Vec<barcode_core::DecodeResult> {
        // Remembered polarity applies ONLY to consecutive live camera frames
        // (confirming an inverted close-up whose ink fraction dropped below 50%).
        // Stills are independent one-shot photos and never read or mutate it.
        if !still && self.inverted_ttl > 0 {
            let bits: Vec<u8> = img.data.iter().map(|b| !b).collect();
            let inv = BitImage::new(img.width, img.height, &bits);
            let out = self.scan_polarity(&inv, tiers, false, still_budget_ms / 2);
            if !out.is_empty() {
                self.inverted_ttl = 3;
                return out;
            }
            self.inverted_ttl -= 1;
            let out = self.scan_polarity(img, tiers, false, still_budget_ms / 2);
            if !out.is_empty() {
                self.inverted_ttl = 0;
            }
            return out;
        }
        // A mostly-ink mask is worth a second sweep on the live path. A
        // still image ALWAYS gets one: a gallery photo may frame a
        // white-on-black code with light margins, keeping the ink fraction
        // low (a 4% island).
        let suspicious = still || ink_fraction(img) >= 0.5;
        let first_budget = if suspicious {
            if still {
                still_budget_ms / 3
            } else {
                still_budget_ms / 2
            }
        } else {
            still_budget_ms
        };
        let started = now_ms();
        let out = self.scan_polarity(img, tiers, still, first_budget);
        if !out.is_empty() || !suspicious {
            return out;
        }
        let inv_budget = if still {
            let left = still_budget_ms.saturating_sub(now_ms().saturating_sub(started));
            if left == 0 {
                return out;
            }
            (still_budget_ms - first_budget).min(left)
        } else {
            still_budget_ms - first_budget
        };
        let bits: Vec<u8> = img.data.iter().map(|b| !b).collect();
        let inv = BitImage::new(img.width, img.height, &bits);
        let out = self.scan_polarity(&inv, tiers, still, inv_budget);
        if !still && !out.is_empty() {
            self.inverted_ttl = 3;
        }
        out
    }

    /// One polarity's sweep: 2D (primary + tilt + diagonal), then the
    /// linear tier (including UPC-A folded into EAN-13 so 2D always runs
    /// first and UPC-A gets 180°/90°/270° coverage without a duplicate scan).
    fn scan_polarity(
        &mut self,
        img: &BitImage,
        tiers: &Tiers,
        still: bool,
        still_budget_ms: u64,
    ) -> Vec<barcode_core::DecodeResult> {
        let start = now_ms();
        let mut out: Vec<barcode_core::DecodeResult> = Vec::with_capacity(4);
        let two_d = {
            // Deadline law: the still-path wall time must stay bounded by
            // the caller's budget, but the tilt ladder multiplies per-step
            // costs. Geometric split — the primary sweep takes half the
            // budget, each retry half of what remains — bounds the total
            // to the budget while every step keeps a meaningful slice.
            // The live path keeps its own fixed 500ms per step (the
            // confirmation law amortizes across frames; its tuning is
            // separate and empirically set).
            //
            // Tilt-retry rationale: the axis-aligned 2D locators miss even
            // mild in-plane roll (4° kills datamatrix/aztec/microqr; 8°
            // kills pdf417), and users do not hold phones perpendicular to
            // codes. Tilt retries stay fast-only: the exhaustive DM sweep
            // costs seconds per mask and the ladder would multiply it past
            // the frame budget — the locator's own ±6° sweep plus the
            // ladder angle covers tilt.
            let t0 = now_ms();
            // Live 2D budgets. One live frame gets 500ms from the
            // confirmation law; the 2D sweep must leave room for the
            // linear tier and the main-thread pump. Measured on junk
            // frames (2026-11-12): the exhaustive DM sweep alone spent its
            // whole 500ms, and the per-step ladder budget was multiplied
            // by every retry angle (12 in live) — one unreadable frame ran
            // seconds on a phone, and every slow no-read frame also clears
            // the confirmation streaks. The still path keeps the caller's
            // geometric split (its budget is seconds, not 500ms).
            const LIVE_DM_PRIMARY_MS: u64 = 300;
            const LIVE_LADDER_STEP_MS: u64 = 8;
            const LIVE_LADDER_DEADLINE_MS: u64 = 240;
            // Diagonal (45°-roll) pass. The locators (DM clock track,
            // PDF417 start/stop, Aztec bullseye) are axis-aligned
            // searchers, so the fine ladder above stops at ±12°; a symbol
            // rolled 45/135° needs a +45° resample first (its residual
            // 90/180° is what the locators already handle). One direction
            // covers both diagonals.
            const LIVE_DIAGONAL_MS: u64 = 140;
            const LIVE_DIAGONAL_DEADLINE_MS: u64 = 460;
            let total_dm = if still { still_budget_ms } else { LIVE_DM_PRIMARY_MS };
            let (primary, mut remaining) = if still {
                (total_dm / 2, total_dm - total_dm / 2)
            } else {
                (total_dm, 0)
            };
            let mut two_d = self.scan_2d(img, tiers, still, primary);
            let diagonal_sensitive = tiers.datamatrix || tiers.pdf417 || tiers.aztec;
            // Still: reserve the diagonal's slice before the ladder halves
            // the budget away (the last ladder steps get <1ms otherwise).
            let mut diagonal_budget = 0;
            if still && diagonal_sensitive {
                diagonal_budget = remaining / 3;
                remaining -= diagonal_budget;
            }
            // The ladder only exists for the orientation-sensitive families;
            // an all-rotation-invariant tier set never needs it. Aztec and
            // MicroQR are cheap but NOT invariant at a few degrees: their
            // bullseyes/finders still need the symbol near the module axes
            // (web tilt bank measured 8/36 fails at ±4/±8 without them in
            // the ladder; their own orientation search only covers 90°
            // multiples).
            let tilt_sensitive =
                tiers.datamatrix || tiers.pdf417 || tiers.aztec || tiers.microqr;
            if two_d.is_empty() && tilt_sensitive {
                let area = img.width as u64 * img.height as u64;
                let ladder: &[f64] = if area > 1_200_000 {
                    // Big frames: the datamatrix locator is superlinear in
                    // area (see STATUS), so bound the retry to two attempts.
                    &[4.0, -4.0]
                } else {
                    // Small masks: fine ladder, ~1° residual — every family
                    // then decodes to ~±13° roll.
                    &[2.0, -2.0, 4.0, -4.0, 6.0, -6.0, 8.0, -8.0, 10.0, -10.0, 12.0, -12.0]
                };
                // Still scans with seconds of budget are gallery photos:
                // the geometric split can starve a tilt retry below the
                // ~0.4s a real photo's DM needs, and the locator's fixed
                // costs overshoot the sample budget anyway. Give every
                // still retry a working floor (sub-second still budgets,
                // i.e. tests/embedded callers, keep the strict split).
                let still_min_step = if still && still_budget_ms >= 1200 { 2000 } else { 0 };
                'ladder: for &deg in ladder {
                    // Wall-clock check: the geometric split bounds the
                    // ladder's DM *sampling*, but each step also has fixed
                    // costs (rotation, BitImage/locator setup) the sample
                    // budget doesn't count — without this the wall still
                    // ran ~6x over on junk-rich big frames. Live shares one
                    // deadline for the whole ladder (see the budget note
                    // above): the old per-step 500ms cap was multiplied by
                    // the retry count.
                    if still {
                        if now_ms().saturating_sub(t0) >= still_budget_ms {
                            break;
                        }
                    } else if now_ms().saturating_sub(t0) >= LIVE_LADDER_DEADLINE_MS {
                        break;
                    }
                    let step = if still {
                        let step = remaining / 2;
                        remaining -= step;
                        step.max(still_min_step)
                    } else {
                        LIVE_LADDER_STEP_MS
                    };
                    if step == 0 {
                        break;
                    }
                    let data = img.rotated(deg);
                    let rimg = BitImage::new(img.width, img.height, &data);
                    // Only the orientation-SENSITIVE families retry. QR is
                    // truly direction-free; the others need the module axes
                    // within a few degrees. (The 12 x ~1.2s junk-frame cost
                    // that motivated pruning these was the DM exhaustive
                    // sweep; Aztec/MicroQR are sub-millisecond.)
                    let tilt_tiers = Tiers {
                        datamatrix: tiers.datamatrix,
                        pdf417: tiers.pdf417,
                        aztec: tiers.aztec,
                        microqr: tiers.microqr,
                        ..Default::default()
                    };
                    let mut hits = self.scan_2d(&rimg, &tilt_tiers, false, step);
                    if !hits.is_empty() {
                        // quads are in rotated coords: map back by -deg
                        let (sin, cos) = (-deg).to_radians().sin_cos();
                        let (cx, cy) = (f64::from(img.width) / 2.0, f64::from(img.height) / 2.0);
                        for r in &mut hits {
                            for p in &mut r.position {
                                let (x, y) = (f64::from(p.x) - cx, f64::from(p.y) - cy);
                                p.x = (cos * x - sin * y + cx) as f32;
                                p.y = (sin * x + cos * y + cy) as f32;
                            }
                        }
                        two_d = hits;
                        break 'ladder;
                    }
                }
            }
            // The diagonal is the last resort for BOTH modes: the fine
            // ladder above covers the common careless-wrist roll and is
            // cheaper, while a 45° symbol is rarer. Running it first on a
            // still was a budget bug: its slice overshoots (the locator's
            // fixed costs are not sample-charged), which could leave the
            // ladder no step at all — a 12MP invoice DM read at a 20s still
            // budget but never at the SDK's 5s default. After a slow
            // primary the ladder is skipped (same deadline) and the
            // diagonal still gets its slice.
            if two_d.is_empty() && diagonal_sensitive {
                let budget = if still {
                    diagonal_budget
                } else {
                    let elapsed = now_ms().saturating_sub(t0);
                    LIVE_DIAGONAL_MS.min(LIVE_DIAGONAL_DEADLINE_MS.saturating_sub(elapsed))
                };
                if budget > 0 {
                    let hits = self.scan_diagonal(img, tiers, budget);
                    if !hits.is_empty() {
                        two_d = hits;
                    }
                }
            }
            two_d
        };
        if !two_d.is_empty() {
            out.extend(two_d);
            return out;
        }
        if tiers.any_linear() {
            // Quarter-turn passes are one fast SWAR transpose + shared row
            // scans. On a still frame whose 2D sweep already spent the
            // caller's budget, skip them (0° + 180° still run); the 1D
            // pre-pass on a photo has no 2D cost, so it always gets them.
            let allow_quarter = !still || now_ms().saturating_sub(start) < still_budget_ms;
            out.extend(decode_linear(self.linear_decoders(), img, tiers, allow_quarter, still));
        }
        out
    }

    /// The 2D tiers over one bitmap, first non-empty tier wins. Shared by
    /// the primary sweep and the tilt-retry tier. `dm_budget_ms` is the
    /// DataMatrix tier's budget for THIS call — the caller (the ladder)
    /// splits the frame's deadline across steps so the still-path wall
    /// time stays bounded by it.
    fn scan_2d(
        &mut self,
        img: &BitImage,
        tiers: &Tiers,
        dm_exhaustive: bool,
        dm_budget_ms: u64,
    ) -> Vec<barcode_core::DecodeResult> {
        let mut out: Vec<barcode_core::DecodeResult> = Vec::with_capacity(2);
        if tiers.qr {
            out.extend(self.qr.decode(img));
            if !out.is_empty() {
                return out;
            }
        }
        if tiers.microqr {
            out.extend(decode_microqr(img));
            if !out.is_empty() {
                return out;
            }
        }
        if tiers.datamatrix {
            out.extend(decode_datamatrix_mask_budget(img, dm_exhaustive, dm_budget_ms));
            if !out.is_empty() {
                return out;
            }
        }
        if tiers.aztec {
            out.extend(decode_aztec_mask(img));
            if !out.is_empty() {
                return out;
            }
        }
        if tiers.pdf417 {
            out.extend(decode_pdf417_mask(img));
            if !out.is_empty() {
                return out;
            }
        }
        if tiers.databarstk {
            out.extend(decode_stacked(&mut self.databarstk, img));
            if !out.is_empty() {
                return out;
            }
        }
        out
    }

    /// The diagonal-sensitive tiers on the image resampled by ±45°.
    /// Covers symbols rolled 45/135/225/315° (the residual 90/180° is
    /// each locator's own direction search). Both signs are needed: a
    /// binary image resampled by 45° and then landing on 0/180° at the
    /// locator can lose its bullseye (measured: Aztec at "315" only
    /// reads through the -45° attempt), while 90/270° is stable.
    /// Hit quads are in the rotated frame and are mapped back.
    fn scan_diagonal(
        &mut self,
        img: &BitImage,
        tiers: &Tiers,
        dm_budget_ms: u64,
    ) -> Vec<barcode_core::DecodeResult> {
        // A camera mask has already been through one resample (canvas draw
        // + binarizer); a second nearest-neighbour 45° pass erodes Aztec's
        // bullseye (web tilt bank failed 45/135/315 on 4px modules while
        // the native binary fixture passed). 2x2 supersampling keeps the
        // module edges instead of point-sampling them.
        let diag_tiers = Tiers {
            datamatrix: tiers.datamatrix,
            pdf417: tiers.pdf417,
            aztec: tiers.aztec,
            ..Default::default()
        };
        let (cx, cy) = (f64::from(img.width) / 2.0, f64::from(img.height) / 2.0);
        // The caller's slice is for the whole diagonal; split it.
        let per_attempt = (dm_budget_ms / 2).max(1);
        for deg in [45.0f64, -45.0] {
            let data = rotated_supersampled(img, deg);
            let rimg = BitImage::new(img.width, img.height, &data);
            let mut hits = self.scan_2d(&rimg, &diag_tiers, false, per_attempt);
            if hits.is_empty() {
                continue;
            }
            let (sin, cos) = (-deg).to_radians().sin_cos();
            for r in &mut hits {
                for p in &mut r.position {
                    let (x, y) = (f64::from(p.x) - cx, f64::from(p.y) - cy);
                    p.x = (cos * x - sin * y + cx) as f32;
                    p.y = (sin * x + cos * y + cy) as f32;
                }
            }
            return hits;
        }
        Vec::new()
    }
}

/// Fraction of the mask that is ink. A normal print on paper is mostly
/// background; an inverted (white-on-black) symbol is mostly ink.
fn ink_fraction(img: &BitImage) -> f64 {
    let ones: u64 = img.data.iter().map(|b| u64::from(b.count_ones())).sum();
    ones as f64 / (f64::from(img.width) * f64::from(img.height))
}

/// Rotate a packed bitmap by `deg` with 2x2 supersampling: a destination
/// pixel is ink when at least half of the four source samples are ink.
/// [`BitImage::rotated`] is nearest-neighbour; that is fine for the fine
/// tilt ladder (one resample), but the diagonal pass rotates a mask the
/// camera already resampled once, and point sampling there can drop a
/// whole thin module (Aztec).
fn rotated_supersampled(img: &BitImage, deg: f64) -> Vec<u8> {
    let bpr = img.bytes_per_row;
    let mut data = vec![0u8; bpr * img.height as usize];
    let (cx, cy) = (f64::from(img.width) / 2.0, f64::from(img.height) / 2.0);
    let (sin, cos) = deg.to_radians().sin_cos();
    const OFF: [(f64, f64); 4] = [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)];
    let (fw, fh) = (f64::from(img.width), f64::from(img.height));
    for dy in 0..img.height as usize {
        for dx in 0..img.width as usize {
            let (x, y) = (dx as f64 - cx, dy as f64 - cy);
            let mut ink = 0u8;
            for (ox, oy) in OFF {
                // same inverse mapping as BitImage::rotated
                let sx = cos * (x + ox) + sin * (y + oy) + cx;
                let sy = -sin * (x + ox) + cos * (y + oy) + cy;
                if sx >= 0.0 && sy >= 0.0 && sx < fw && sy < fh && img.ink(sx as u32, sy as u32) {
                    ink += 1;
                }
            }
            if ink >= 2 {
                data[dy * bpr + (dx >> 3)] |= 1 << (dx & 7);
            }
        }
    }
    data
}

/// The canonical symbology names, in a stable order. Results carry exactly
/// these lowercase names; the `formats` API accepts the same forms with
/// "qr" accepted as an alias for "qrcode".
pub const SYMBOLOGIES: [&str; 19] = [
    "qrcode", "microqr", "upca", "datamatrix", "aztec", "pdf417", "ean13", "ean8", "upce", "code39",
    "code32", "code93", "code128", "itf", "codabar", "databar", "databarltd", "databarstk",
    "databarexpanded",
];

/// Which decoder tiers to run for one frame.
#[derive(Default, Clone, Copy)]
pub struct Tiers {
    pub qr: bool,
    pub microqr: bool,
    pub upca: bool,
    pub datamatrix: bool,
    pub aztec: bool,
    pub pdf417: bool,
    pub ean13: bool,
    pub code39: bool,
    /// Code 32 is a Code 39 payload interpretation: the carrier decoder is
    /// the same one, only the result mapping differs.
    pub code32: bool,
    pub itf: bool,
    pub code128: bool,
    pub code93: bool,
    pub codabar: bool,
    pub ean8: bool,
    pub upce: bool,
    pub databar: bool,
    pub databarltd: bool,
    pub databarstk: bool,
    pub databarexpanded: bool,
}

impl Tiers {
    pub fn all() -> Tiers {
        Tiers { qr: true, microqr: true, upca: true, datamatrix: true, aztec: true, pdf417: true, ean13: true, code39: true, code32: true, itf: true, code128: true, code93: true, codabar: true, ean8: true, upce: true, databar: true, databarltd: true, databarstk: true, databarexpanded: true }
    }

    fn any_linear(&self) -> bool {
        self.upca || self.ean13 || self.code39 || self.code32 || self.itf || self.code128 || self.code93 || self.codabar || self.ean8 || self.upce || self.databar || self.databarltd || self.databarexpanded
    }

}

impl BarcodeScanner {
    /// Plain-results still decode — the natively testable half of
    /// [`BarcodeScanner::decode_still`] (which only adds the JsValue
    /// envelope). `tiers` selects the tier set; `budget_ms` is the shared
    /// still deadline (geometrically split across the tilt ladder).
    pub fn decode_still_core(
        &mut self,
        width: usize,
        height: usize,
        data: &[u8],
        tiers: Tiers,
        budget_ms: u32,
    ) -> Vec<barcode_core::DecodeResult> {
        let img = BitImage::new(width as u32, height as u32, data);
        self.scan_tiered(&img, &tiers, true, u64::from(budget_ms))
    }

    /// Plain-results LIVE decode — the natively testable half of
    /// [`BarcodeScanner::decode`]. Same tiers, but the camera budget
    /// laws (300ms DM primary, 240ms ladder, diagonal last resort).
    pub fn decode_live_core(
        &mut self,
        width: usize,
        height: usize,
        data: &[u8],
        tiers: Tiers,
    ) -> Vec<barcode_core::DecodeResult> {
        let img = BitImage::new(width as u32, height as u32, data);
        self.scan_tiered(&img, &tiers, false, 0)
    }
}

fn parse_formats(formats: &[String]) -> Result<Tiers, String> {
    let mut t = Tiers::default();
    for name in formats {
        let norm: String = name
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        match norm.as_str() {
            "qr" | "qrcode" => t.qr = true,
            "microqr" | "microqrcode" => t.microqr = true,
            "upca" | "upc-a" | "upc" => t.upca = true,
            "dm" | "datamatrix" => t.datamatrix = true,
            "aztec" => t.aztec = true,
            "pdf417" => t.pdf417 = true,
            "ean13" => t.ean13 = true,
            "ean8" => t.ean8 = true,
            "upce" => t.upce = true,
            "code39" => t.code39 = true,
            "code32" | "pharmacode32" | "italianpharmacode" => t.code32 = true,
            "code93" => t.code93 = true,
            "code128" => t.code128 = true,
            "itf" => t.itf = true,
            "codabar" => t.codabar = true,
            "databar" | "databaromni" | "gs1databar" => t.databar = true,
            "databarltd" | "databarlimited" | "gs1databarlimited" => t.databarltd = true,
            "databarstk" | "databarstacked" | "databarstackomni" | "gs1databarstacked" => t.databarstk = true,
            "databarexpanded" | "databarexp" | "gs1databarexpanded" | "rssexpanded" => {
                t.databarexpanded = true
            }
            other => {
                return Err(format!(
                    "unknown symbology {other:?} — supported: {}",
                    SYMBOLOGIES.join(", ")
                ));
            }
        }
    }
    Ok(t)
}


/// The canonical symbology list for capability reporting.
#[wasm_bindgen]
pub fn supported_symbologies() -> Vec<String> {
    SYMBOLOGIES.iter().map(|s| s.to_string()).collect()
}

/// Re-label carrier symbologies (`EAN13` -> `UPCA` on leading `'0'`,
/// `Code39` -> `Code32` on valid 6-char AIC payloads) and drop carrier hits
/// that weren't requested when only the derived symbology was asked for.
fn apply_linear_labels(
    out: Vec<barcode_core::DecodeResult>,
    tiers: &Tiers,
) -> Vec<barcode_core::DecodeResult> {
    out.into_iter()
        .filter_map(|mut r| match r.symbology {
            Symbology::EAN13 => {
                if tiers.upca && r.text.starts_with('0') {
                    r.symbology = Symbology::UPCA;
                    r.text.remove(0);
                    r.raw = r.text.as_bytes().to_vec();
                    Some(r)
                } else if tiers.ean13 {
                    Some(r)
                } else {
                    None
                }
            }
            Symbology::Code39 if tiers.code32 => {
                let Some(text) = code32_text(&r.text) else {
                    return if tiers.code39 { Some(r) } else { None };
                };
                r.symbology = Symbology::Code32;
                r.raw = text.as_bytes().to_vec();
                r.text = text;
                Some(r)
            }
            _ => Some(r),
        })
        .collect()
}

/// Scan `img` center-out, extracting word-level [`RowRuns`] ONCE per
/// scanline and fanning the slice out to all active 1D decoders.
///
/// - When `stride == 1` (stills and small crops `< 64px`): every single row
///   is scanned (100% lossless).
/// - When `stride == 2` (tall live camera frames): scans every 2nd row
///   center-out, and whenever any decoder records a new vote on row `y`,
///   immediately probes `y - 1` and `y + 1` so even a 2-row-tall tilted
///   symbol confirms on the spot while junk/aiming frames pay half the rows.
fn scan_linear_orientation(
    active: &mut [&mut dyn LinearDecoder],
    img: &BitImage,
    reverse: bool,
    stride: u32,
    row: &mut RowRuns,
    rev: &mut RowRuns,
) -> Vec<barcode_core::DecodeResult> {
    for d in active.iter_mut() {
        d.reset();
    }
    let mut out = Vec::new();
    let n = active.len();
    if n == 0 || img.height == 0 {
        return out;
    }
    let mut done = [false; 12];
    let mut remaining = n;
    let h = img.height;
    let stride = stride.max(1);
    let mid = h / 2;

    let mut run_row = |y: u32, check_vote: bool| -> (bool, bool) {
        row.scan_row(img, y);
        // Shortest valid 1D symbol is ITF (10 runs); skip sparse rows.
        if row.runs.len() < 10 {
            return (false, false);
        }
        let feed: &RowRuns = if reverse {
            row.reverse_into(rev);
            rev
        } else {
            row
        };
        let mut voted = false;
        for (i, d) in active.iter_mut().enumerate() {
            if done[i] {
                continue;
            }
            let before = if check_vote { d.vote_count() } else { 0 };
            if let Some(hit) = d.scan_line(feed, y) {
                out.push(hit);
                done[i] = true;
                remaining -= 1;
            } else if check_vote && d.vote_count() > before {
                voted = true;
            }
        }
        (voted, remaining == 0)
    };

    // Track the outermost odd neighbor already probed on each side of `mid`
    // so adjacent strided hits can never double-vote on the same odd row:
    // - side 0 moves downward: `y + 1` may already equal `lo_edge`, new edge is `y - 1`
    // - side 1 moves upward:   `y - 1` may already equal `hi_edge`, new edge is `y + 1`
    let mut lo_edge = u32::MAX;
    let mut hi_edge = u32::MAX;

    let mut step = 0u32;
    'outer: loop {
        let off = step * stride;
        let ys = if step == 0 {
            [Some(mid), None]
        } else {
            [
                mid.checked_sub(off),
                (mid + off < h).then_some(mid + off),
            ]
        };
        if ys[0].is_none() && ys[1].is_none() {
            break;
        }
        for (side, oy) in ys.into_iter().enumerate() {
            let Some(y) = oy else { continue };
            let (voted, all_done) = run_row(y, stride > 1);
            if all_done {
                break 'outer;
            }
            if stride > 1 && voted {
                if let Some(nb) = y.checked_sub(1) {
                    if side == 0 || nb != hi_edge {
                        if side == 0 {
                            lo_edge = nb;
                        }
                        if run_row(nb, false).1 {
                            break 'outer;
                        }
                    }
                }
                if y + 1 < h {
                    let nb = y + 1;
                    if step == 0 || side == 1 || nb != lo_edge {
                        if step == 0 || side == 1 {
                            hi_edge = nb;
                        }
                        if run_row(nb, false).1 {
                            break 'outer;
                        }
                    }
                }
            }
        }
        step += 1;
    }
    for (i, d) in active.iter_mut().enumerate() {
        if !done[i] {
            if let Some(hit) = d.finish_votes() {
                out.push(hit);
            }
        }
    }
    out
}

/// The 1D tier: shared word-level scanline extraction across all active
/// linear decoders, covering all four cardinal orientations:
/// - 0°: forward rows of `img`
/// - 180°: in-place reversed runs of `img` (no `mirrored_x()` bitmap copy)
/// - 90° CW: one SWAR 8×8 transpose (`rot90_cw`), forward rows
/// - 270° (90° CCW): in-place reversed runs of the same `rot90_cw` mask
///   (no second transpose or redundant mirrored-quarter passes)
fn decode_linear(
    decoders: (&mut Ean13Decoder, &mut Code39Decoder, &mut ItfDecoder, &mut Code128Decoder, &mut Code93Decoder, &mut CodabarDecoder, &mut Ean8Decoder, &mut UpceDecoder, &mut DataBarOmniDecoder, &mut DataBarLtdDecoder, &mut DataBarExpandedDecoder),
    img: &BitImage,
    tiers: &Tiers,
    allow_quarter: bool,
    still: bool,
) -> Vec<barcode_core::DecodeResult> {
    let (ean13, code39, itf, code128, code93, codabar, ean8, upce, databar, databarltd, databarexpanded) = decoders;
    let mut active: Vec<&mut dyn LinearDecoder> = Vec::with_capacity(11);
    if tiers.ean13 || tiers.upca { active.push(ean13); }
    if tiers.code39 || tiers.code32 { active.push(code39); }
    if tiers.itf { active.push(itf); }
    if tiers.code128 { active.push(code128); }
    if tiers.code93 { active.push(code93); }
    if tiers.codabar { active.push(codabar); }
    if tiers.ean8 { active.push(ean8); }
    if tiers.upce { active.push(upce); }
    if tiers.databar { active.push(databar); }
    if tiers.databarltd { active.push(databarltd); }
    if tiers.databarexpanded { active.push(databarexpanded); }
    if active.is_empty() {
        return Vec::new();
    }

    let mut row = RowRuns::with_capacity(256);
    let mut rev = RowRuns::with_capacity(256);
    // Stills and small ROI crops keep stride = 1 (every row); tall live
    // camera frames use stride = 2 with immediate `y ± 1` corroboration
    // whenever any decoder records a vote.
    let stride_h = if !still && img.height >= 64 { 2 } else { 1 };

    // 0° (upright)
    let out = apply_linear_labels(
        scan_linear_orientation(&mut active, img, false, stride_h, &mut row, &mut rev),
        tiers,
    );
    if !out.is_empty() {
        return out;
    }

    // 180° (upside-down via in-place run reversal)
    let w = img.width as f32 - 1.0;
    let mut out = apply_linear_labels(
        scan_linear_orientation(&mut active, img, true, stride_h, &mut row, &mut rev),
        tiers,
    );
    if !out.is_empty() {
        for r in &mut out {
            let [p0, p1, p2, p3] = r.position;
            r.position = [
                Point { x: w - p1.x, y: p1.y },
                Point { x: w - p0.x, y: p0.y },
                Point { x: w - p3.x, y: p3.y },
                Point { x: w - p2.x, y: p2.y },
            ];
        }
        return out;
    }

    if !allow_quarter {
        return out;
    }

    // 90° CW and 270° (90° CCW) from ONE SWAR 8×8 transpose:
    // - forward rows of `rot90_cw` read columns bottom-to-top (90° CW)
    // - reversed rows of `rot90_cw` read columns top-to-bottom (270° CCW)
    let oh = img.height as f32 - 1.0;
    let rotated = img.rot90_cw();
    let rimg = BitImage::new(img.height, img.width, &rotated);
    let stride_v = if !still && rimg.height >= 64 { 2 } else { 1 };

    let mut out = apply_linear_labels(
        scan_linear_orientation(&mut active, &rimg, false, stride_v, &mut row, &mut rev),
        tiers,
    );
    if !out.is_empty() {
        for r in &mut out {
            for p in &mut r.position {
                let (rx, ry) = (p.x, p.y);
                p.x = ry;
                p.y = oh - rx;
            }
        }
        return out;
    }

    let mut out = apply_linear_labels(
        scan_linear_orientation(&mut active, &rimg, true, stride_v, &mut row, &mut rev),
        tiers,
    );
    if !out.is_empty() {
        for r in &mut out {
            // Reversed `rot90_cw` (mx, ry) maps to original (x = ry, y = mx).
            let [p0, p1, p2, p3] = r.position;
            r.position = [
                Point { x: p1.y, y: p1.x },
                Point { x: p0.y, y: p0.x },
                Point { x: p3.y, y: p3.x },
                Point { x: p2.y, y: p2.x },
            ];
        }
    }
    out
}

/// The DataBar Stacked tier: upright first, then a 180° rotation pass (an
/// upside-down stacked symbol swaps its bands AND reverses its runs — the
/// rotation restores an upright symbol; the hit's quad maps back through
/// the same rotation). The stitch + decode_omni tables reject anything
/// malformed, so a pass that finds nothing is just wasted work.
fn decode_stacked(
    decoder: &mut DataBarStackedDecoder,
    img: &BitImage,
) -> Vec<barcode_core::DecodeResult> {
    let mut out = decoder.decode(img);
    if !out.is_empty() {
        return out;
    }
    let rotated = img.rot180();
    let flipped = BitImage::new(img.width, img.height, &rotated);
    let (w, h) = (img.width as f32 - 1.0, img.height as f32 - 1.0);
    for mut r in decoder.decode(&flipped) {
        let [p0, p1, p2, p3] = r.position;
        let t = |p: Point| Point { x: w - p.x, y: h - p.y };
        r.position = [t(p2), t(p3), t(p0), t(p1)];
        out.push(r);
    }
    out
}

impl BarcodeScanner {
    fn linear_decoders(
        &mut self,
    ) -> (&mut Ean13Decoder, &mut Code39Decoder, &mut ItfDecoder, &mut Code128Decoder, &mut Code93Decoder, &mut CodabarDecoder, &mut Ean8Decoder, &mut UpceDecoder, &mut DataBarOmniDecoder, &mut DataBarLtdDecoder, &mut DataBarExpandedDecoder) {
        (&mut self.ean13, &mut self.code39, &mut self.itf, &mut self.code128, &mut self.code93, &mut self.codabar, &mut self.ean8, &mut self.upce, &mut self.databar, &mut self.databarltd, &mut self.databarexpanded)
    }
}


impl WasmResult {
    fn of(r: barcode_core::DecodeResult) -> Self {
        // Canonical lowercase names — the same vocabulary as `formats`,
        // `supported_symbologies()` and the live scheduler (`checked`).
        let symbology = r.symbology.name().to_string();
        let mut quad = [0f32; 8];
        for (i, p) in r.position.iter().enumerate() {
            quad[i * 2] = p.x;
            quad[i * 2 + 1] = p.y;
        }
        Self { text: r.text, raw: r.raw, symbology, quad }
    }
}
