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
use barcode_ean13::UpcaDecoder;
use barcode_upce::UpceDecoder;
use barcode_code93::Code93Decoder;
use barcode_code39::{Code39Decoder, code32_raw, code32_text};
use barcode_core::{BitImage, Symbology, SymbologyDecoder};
use barcode_ean13::Ean13Decoder;
use barcode_itf::ItfDecoder;
use barcode_databar::{DataBarLtdDecoder, DataBarOmniDecoder, DataBarStackedDecoder};
use barcode_fixtures::Canvas;

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
    upca: UpcaDecoder,
    databar: DataBarOmniDecoder,
    databarltd: DataBarLtdDecoder,
    databarstk: DataBarStackedDecoder,
    /// Polarity of the last successful read. Camera frames keep their
    /// polarity, and the confirmation re-check crops to the quad — that
    /// close-up of an inverted symbol is ~46% ink, below the full-frame
    /// heuristic. Remembering the polarity makes the close-up cheap.
    inverted: bool,
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
            upca: UpcaDecoder::new(2),
            databar: DataBarOmniDecoder::new(2),
            databarltd: DataBarLtdDecoder::new(2),
            databarstk: DataBarStackedDecoder::new(1),
            inverted: false,
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
        let bits: Vec<u8> = img.data.iter().map(|b| !b).collect();
        let inv = BitImage::new(img.width, img.height, &bits);
        // Remembered polarity first: a camera frame keeps its polarity, and
        // the confirmation close-up crops to the quad — an inverted bar
        // strip measures ~46% ink, below the full-frame heuristic.
        if self.inverted {
            let out = self.scan_polarity(&inv, tiers, still, still_budget_ms / 2);
            if !out.is_empty() {
                return out;
            }
            let out = self.scan_polarity(img, tiers, still, still_budget_ms / 2);
            if !out.is_empty() {
                self.inverted = false;
            }
            return out;
        }
        // A mostly-ink mask is worth a second sweep on the live path. A
        // still image ALWAYS gets one: a gallery photo may frame a
        // white-on-black code with light margins, keeping the ink fraction
        // low (a 4% island), and the remembered polarity starts false.
        // The budget is split up-front so the pair still respects the
        // caller's deadline (a dark scene pays the same total for two
        // opinions).
        let suspicious = still || ink_fraction(img) >= 0.5;
        // The inverted second opinion is the common failure mode for a
        // still (a white-on-black code on a light page), and its DataMatrix
        // ladder alone can need ~2s, so the still primary gets a third and
        // the retry two thirds. Live keeps the even split: a frame that
        // already looks dark should not starve the primary polarity.
        let first_budget = if suspicious {
            if still {
                still_budget_ms / 3
            } else {
                still_budget_ms / 2
            }
        } else {
            still_budget_ms
        };
        let out = self.scan_polarity(img, tiers, still, first_budget);
        if !out.is_empty() || !suspicious {
            return out;
        }
        let out = self.scan_polarity(&inv, tiers, still, still_budget_ms - first_budget);
        if !out.is_empty() {
            self.inverted = true;
        }
        out
    }

    /// One polarity's sweep: UPC-A, then 2D (primary + tilt + diagonal),
    /// then the linear tier.
    fn scan_polarity(
        &mut self,
        img: &BitImage,
        tiers: &Tiers,
        still: bool,
        still_budget_ms: u64,
    ) -> Vec<barcode_core::DecodeResult> {
        let start = now_ms();
        let mut out: Vec<barcode_core::DecodeResult> = Vec::with_capacity(4);
        // UPC-A is a subset of EAN-13 (lead 0): report it first when enabled.
        if tiers.upca {
            out.extend(self.upca.decode(img));
            if !out.is_empty() {
                return out;
            }
        }
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
            let ladder_dbg = std::env::var("DM_LADDER_DBG").is_ok();
            let mut two_d = self.scan_2d(img, tiers, still, primary);
            if ladder_dbg {
                eprintln!("ladder primary budget {primary} @{}ms -> {}", now_ms().saturating_sub(t0), two_d.len());
            }
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
                    if ladder_dbg {
                        eprintln!("ladder step {deg} budget {step} @{}ms -> {}", now_ms().saturating_sub(t0), hits.len());
                    }
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
                    if ladder_dbg {
                        eprintln!("ladder diagonal budget {budget} @{}ms -> {}", now_ms().saturating_sub(t0), hits.len());
                    }
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
            // Quarter-turn passes are two extra full-mask copies. On a
            // still frame whose 2D sweep already spent the caller's
            // budget, skip them (upright + mirrored still run); the 1D
            // pre-pass on a photo has no 2D cost, so it always gets them.
            let allow_quarter = !still || now_ms().saturating_sub(start) < still_budget_ms;
            out.extend(decode_linear(self.linear_decoders(), img, tiers, allow_quarter, still));
            out = apply_code32(out, tiers);
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

/// The canonical symbology names, in a stable order. Results carry these
/// (title-cased via the debug format); the `formats` API accepts the
/// lowercase forms with "qr" accepted as an alias for "qrcode".
pub const SYMBOLOGIES: [&str; 18] = [
    "qrcode", "microqr", "upca", "datamatrix", "aztec", "pdf417", "ean13", "ean8", "upce", "code39",
    "code32", "code93", "code128", "itf", "codabar", "databar", "databarltd", "databarstk",
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
}

impl Tiers {
    pub fn all() -> Tiers {
        Tiers { qr: true, microqr: true, upca: true, datamatrix: true, aztec: true, pdf417: true, ean13: true, code39: true, code32: true, itf: true, code128: true, code93: true, codabar: true, ean8: true, upce: true, databar: true, databarltd: true, databarstk: true }
    }

    fn any_linear(&self) -> bool {
        self.ean13 || self.code39 || self.code32 || self.itf || self.code128 || self.code93 || self.codabar || self.ean8 || self.upce || self.databar || self.databarltd
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

/// Re-label valid Code 32 payloads. Code 32 has no separate carrier: the
/// six-char Code 39 text *is* the base-32 encoding of the printed AIC
/// value. `code32_text` is strict (alphabet + check digit), so an ordinary
/// Code 39 symbol passes through unchanged. When only `code32` was
/// requested, non-Code-32 Code 39 hits are dropped.
fn apply_code32(out: Vec<barcode_core::DecodeResult>, tiers: &Tiers) -> Vec<barcode_core::DecodeResult> {
    if !tiers.code32 {
        return out;
    }
    out.into_iter()
        .filter_map(|mut r| {
            if r.symbology != Symbology::Code39 {
                return Some(r);
            }
            let Some(text) = code32_text(&r.text) else {
                return if tiers.code39 { Some(r) } else { None };
            };
            r.symbology = Symbology::Code32;
            r.raw = text.as_bytes().to_vec();
            r.text = text;
            Some(r)
        })
        .collect()
}

/// The 1D tier: all linear decoders over one bitmap, plus an upside-down
/// pass. A 180°-rotated linear barcode's run sequence is the reverse of
/// the true one, so the decoders see nothing; feeding them a horizontally
/// mirrored bitmap reads it as a normal barcode and yields the TRUE text
/// (check digit validated). The hit's x coordinates are mirrored back
/// into the original frame space. The mirror pass is only paid when the
/// upright pass found nothing.
fn decode_linear(
    decoders: (&mut Ean13Decoder, &mut Code39Decoder, &mut ItfDecoder, &mut Code128Decoder, &mut Code93Decoder, &mut CodabarDecoder, &mut Ean8Decoder, &mut UpceDecoder, &mut DataBarOmniDecoder, &mut DataBarLtdDecoder),
    img: &BitImage,
    tiers: &Tiers,
    allow_quarter: bool,
    still: bool,
) -> Vec<barcode_core::DecodeResult> {
    let (ean13, code39, itf, code128, code93, codabar, ean8, upce, databar, databarltd) = decoders;
    let mut out: Vec<barcode_core::DecodeResult> = Vec::new();
    let mut active: Vec<&mut dyn barcode_core::SymbologyDecoder> = Vec::new();
    if tiers.ean13 { active.push(ean13); }
    if tiers.code39 || tiers.code32 { active.push(code39); }
    if tiers.itf { active.push(itf); }
    if tiers.code128 { active.push(code128); }
    if tiers.code93 { active.push(code93); }
    if tiers.codabar { active.push(codabar); }
    if tiers.ean8 { active.push(ean8); }
    if tiers.upce { active.push(upce); }
    if tiers.databar { active.push(databar); }
    if tiers.databarltd { active.push(databarltd); }
    for d in active.iter_mut().map(|d| &mut **d as &mut dyn barcode_core::SymbologyDecoder) {
        out.extend(d.decode(img));
    }
    if !out.is_empty() {
        return out;
    }
    let mirrored = img.mirrored_x();
    let flipped = BitImage::new(img.width, img.height, &mirrored);
    let w = img.width as f32 - 1.0;
    for d in active.iter_mut().map(|d| &mut **d as &mut dyn barcode_core::SymbologyDecoder) {
        for mut r in d.decode(&flipped) {
            let [p0, p1, p2, p3] = r.position;
            r.position = [
                Point { x: w - p1.x, y: p1.y },
                Point { x: w - p0.x, y: p0.y },
                Point { x: w - p3.x, y: p3.y },
                Point { x: w - p2.x, y: p2.y },
            ];
            out.push(r);
        }
    }
    if !out.is_empty() || !allow_quarter {
        return out;
    }
    // Quarter-turn passes: a linear barcode printed sideways (loyalty
    // cards, receipts photographed in portrait) is invisible to a
    // row-scanner. Field photo: a loyalty-card Code 128 read 0 hits
    // upright, decoded at 90° at native resolution. Two packed copies +
    // decodes, only after both upright passes found nothing.
    let (ow, oh) = (img.width as f32 - 1.0, img.height as f32 - 1.0);
    // Both quarter turns, each also MIRRORED: a receipt printed sideways
    // and photographed upside down needs the 90°+180° combination. The
    // upright passes above cover 0° and 180°; without the mirrored quarter
    // turns those two quadrants are unreachable (field photo: the Esselunga
    // receipt barcode only decodes on the reversed 270° runs).
    for clockwise in [true, false] {
        let rotated = rot90_packed(img, clockwise);
        let rimg = BitImage::new(img.height, img.width, &rotated);
        let rw = rimg.width as f32 - 1.0;
        // The mirrored quarter variants are still-frame insurance: the
        // common sideways print reads on the plain quarter, and live
        // frames must not pay four full-mask decodes when a 2D tier is
        // also scheduled.
        let variants: &[bool] = if still { &[false, true] } else { &[false] };
        for &mirrored in variants {
            let mpacked = if mirrored { rimg.mirrored_x() } else { Vec::new() };
            let timg = if mirrored {
                BitImage::new(rimg.width, rimg.height, &mpacked)
            } else {
                rimg
            };
            for d in active.iter_mut().map(|d| &mut **d as &mut dyn barcode_core::SymbologyDecoder) {
                for mut r in d.decode(&timg) {
                    if mirrored {
                        // Mirrored hit space -> rotated-frame space.
                        let [p0, p1, p2, p3] = r.position;
                        r.position = [
                            Point { x: rw - p1.x, y: p1.y },
                            Point { x: rw - p0.x, y: p0.y },
                            Point { x: rw - p3.x, y: p3.y },
                            Point { x: rw - p2.x, y: p2.y },
                        ];
                    }
                    for p in &mut r.position {
                        // Rotated-frame space -> original frame space.
                        let (rx, ry) = (p.x, p.y);
                        if clockwise {
                            p.x = ry;
                            p.y = oh - rx;
                        } else {
                            p.x = ow - ry;
                            p.y = rx;
                        }
                    }
                    out.push(r);
                }
            }
            if !out.is_empty() {
                return out;
            }
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
    ) -> (&mut Ean13Decoder, &mut Code39Decoder, &mut ItfDecoder, &mut Code128Decoder, &mut Code93Decoder, &mut CodabarDecoder, &mut Ean8Decoder, &mut UpceDecoder, &mut DataBarOmniDecoder, &mut DataBarLtdDecoder) {
        (&mut self.ean13, &mut self.code39, &mut self.itf, &mut self.code128, &mut self.code93, &mut self.codabar, &mut self.ean8, &mut self.upce, &mut self.databar, &mut self.databarltd)
    }
}

/// Spread the 8 bits of one byte to every other bit of a u16: each source
/// pixel becomes a 2-pixel run. Half of the DataMatrix 2× upscale leg.
#[inline]
fn spread_byte2x(b: u8) -> u16 {
    let mut x = b as u16;
    x = (x | (x << 4)) & 0x0f0f;
    x = (x | (x << 2)) & 0x3333;
    (x | (x << 1)) & 0x5555
}

/// Nearest-neighbour 2× upscale of a packed bitmap: every source bit is a
/// 2×2 destination block, every source row is written twice.
#[inline]
fn upscale2_packed(img: &BitImage) -> (usize, usize, Vec<u8>) {
    #[cfg(target_arch = "wasm32")]
    {
        // The SDK's browser floor is WebGPU-capable engines, all of which
        // ship wasm SIMD; the opt-in is local to this function.
        upscale2_packed_simd(img)
    }
    #[cfg(not(target_arch = "wasm32"))]
    upscale2_packed_scalar(img)
}

#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
fn upscale2_packed_simd(img: &BitImage) -> (usize, usize, Vec<u8>) {
    use core::arch::wasm32::*;
    let w = img.width as usize;
    if w % 8 != 0 {
        return upscale2_packed_scalar(img);
    }
    let h = img.height as usize;
    let (uw, uh) = (w * 2, h * 2);
    let bpr = uw.div_ceil(8).div_ceil(4) * 4;
    let mut data = vec![0u8; bpr * uh];
    let wb = w / 8;
    for y in 0..h {
        let src = &img.data[y * img.bytes_per_row..y * img.bytes_per_row + wb];
        let (r0, r1) = (2 * y * bpr, (2 * y + 1) * bpr);
        let mut xb = 0usize;
        while xb + 8 <= wb {
            let mut lanes = [0u16; 8];
            for (k, lane) in lanes.iter_mut().enumerate() {
                *lane = src[xb + k] as u16;
            }
            // SAFETY: lanes is 16 bytes; the destination block is 16
            // bytes and fits: xb*2 + 16 <= 2*wb = uw/8 <= bpr.
            let x = unsafe { core::ptr::read_unaligned(lanes.as_ptr() as *const v128) };
            let x = v128_and(v128_or(i16x8_shl(x, 4), x), i16x8_splat(0x0f0f));
            let x = v128_and(v128_or(i16x8_shl(x, 2), x), i16x8_splat(0x3333));
            let x = v128_and(v128_or(i16x8_shl(x, 1), x), i16x8_splat(0x5555));
            let off = xb * 2;
            unsafe {
                core::ptr::write_unaligned(data.as_mut_ptr().add(r0 + off) as *mut v128, x);
                core::ptr::write_unaligned(data.as_mut_ptr().add(r1 + off) as *mut v128, x);
            }
            xb += 8;
        }
        for i in xb..wb {
            let v = spread_byte2x(src[i]).to_le_bytes();
            let off = i * 2;
            data[r0 + off] = v[0];
            data[r0 + off + 1] = v[1];
            data[r1 + off] = v[0];
            data[r1 + off + 1] = v[1];
        }
    }
    (uw, uh, data)
}

/// Portable arm (and the non-whole-byte fallback for the SIMD arm).
fn upscale2_packed_scalar(img: &BitImage) -> (usize, usize, Vec<u8>) {
    let w = img.width as usize;
    let h = img.height as usize;
    let (uw, uh) = (w * 2, h * 2);
    let bpr = uw.div_ceil(8).div_ceil(4) * 4;
    let mut data = vec![0u8; bpr * uh];
    if w % 8 == 0 {
        let wb = w / 8;
        for y in 0..h {
            let src = &img.data[y * img.bytes_per_row..y * img.bytes_per_row + wb];
            let (r0, r1) = (2 * y * bpr, (2 * y + 1) * bpr);
            for i in 0..wb {
                let v = spread_byte2x(src[i]).to_le_bytes();
                let off = i * 2;
                data[r0 + off] = v[0];
                data[r0 + off + 1] = v[1];
                data[r1 + off] = v[0];
                data[r1 + off + 1] = v[1];
            }
        }
        return (uw, uh, data);
    }
    // Widths that are not whole bytes keep the direct contract mapping.
    for y in 0..uh {
        let sy = y / 2;
        for x in 0..uw {
            if img.ink((x / 2) as u32, sy as u32) {
                data[y * bpr + (x >> 3)] |= 1 << (x & 7);
            }
        }
    }
    (uw, uh, data)
}

/// The Data Matrix tier: L-finder location + decode over the same 1bpp
/// mask the other tiers consume (the GPU binarizer's Bradley rule matches
/// the locator's own binarization, docs/STATUS.md). Public for native
/// tests; `scan_tiered` is the production caller.
pub fn decode_datamatrix_mask(img: &BitImage) -> Vec<barcode_core::DecodeResult> {
    decode_datamatrix_mask_ex(img, true)
}

/// `exhaustive` permits the reference sweep fallback after the fast
/// locator misses (upright masks only — tilt retries stay fast-only so
/// the ladder cannot multiply seconds per frame).
pub fn decode_datamatrix_mask_ex(img: &BitImage, exhaustive: bool) -> Vec<barcode_core::DecodeResult> {
    decode_datamatrix_mask_budget(img, exhaustive, 5000)
}

/// Camera-tier entry: the live budget (500ms — two consecutive frames
/// confirm in <1s). Expiry = no-match; the stream retries fresh frames.
pub fn decode_datamatrix_mask_stream(img: &BitImage, exhaustive: bool) -> Vec<barcode_core::DecodeResult> {
    decode_datamatrix_mask_budget(img, exhaustive, 500)
}

/// `budget_ms` bounds the whole DM search for this frame via the
/// calibrated sample allowance (36M charged ~ 4.7s on rig frames).
pub fn decode_datamatrix_mask_budget(img: &BitImage, exhaustive: bool, budget_ms: u64) -> Vec<barcode_core::DecodeResult> {
    const SAMPLES_PER_MS: u64 = 7_700;
    let total = budget_ms * SAMPLES_PER_MS;
    // Big masks: the exhaustive pitch sweep on a junk-rich frame costs
    // seconds — the downscaled copies decode them in tens of ms.
    let base = if img.width as u64 * img.height as u64 > 1_200_000 {
        Vec::new()
    } else {
        // `exhaustive` = the reference sweep may back up the fast locator
        // on this mask (still images). Live frames pass false: the fast
        // leg-derived locator only, then the downscaled legs below.
        decode_dm_at(img, 1.0, exhaustive, total)
    };
    if !base.is_empty() {
        return base;
    }
    // High-resolution masks fragment the finder legs: the run-chain slack
    // is absolute (±2px) and a camera-keystoned leg at 20px+/module drifts
    // past it, shattering the L into 1px row-bars. The SAME locate
    // succeeds on a box-downscaled copy — so retry downscaled, quads
    // scaled back. 4x first: its sweep is ~4x cheaper and every pitch
    // reachable at 2x is decodable at 4x (RS absorbs the sub-pitch error);
    // 2x catches codes whose on-screen pitch is too small for 4x.
    for &f in &[4u32, 2] {
        let (w, h) = (img.width, img.height);
        let (sw, sh) = (w / f, h / f);
        let bpr = (sw.div_ceil(8).div_ceil(4) * 4) as usize;
        let mut data = vec![0u8; bpr * sh as usize];
        for y in 0..sh as usize {
            for x in 0..sw as usize {
                let mut ink = 0;
                for dy in 0..f as usize {
                    for dx in 0..f as usize {
                        if img.ink((x * f as usize + dx) as u32, (y * f as usize + dy) as u32) {
                            ink += 1;
                        }
                    }
                }
                if ink * 2 >= (f * f) as usize {
                    data[y * bpr + (x >> 3)] |= 1 << (x & 7);
                }
            }
        }
        let small = BitImage::new(sw, sh, &data);
        let hits = decode_dm_at(&small, f as f32, exhaustive, total / 4);
        if !hits.is_empty() {
            return hits;
        }
    }
    // Small symbol on a big frame: at native resolution the finder legs
    // fragment (pitch too small for the corner pairing) and the box
    // downgrades land the grid wrong (e.g. 12x26 instead of 12x36), so
    // RS never verifies. A 2x nearest-neighbor upscaled copy puts the
    // pitch where the grid estimation works (fixture: viva-dm-rot0 —
    // a 12x36 DMRE at ~7px/module on a 1080px mask decodes there at
    // 0/90/270 degrees). Last resort: only when every other leg came
    // up empty, and bounded in memory like the rest of the ladder.
    //
    // Live frames skip it: building a 2112² mask cost 4.6s on the phone
    // for a junk frame (the sample budget bounds the locator, not the
    // buffer build). The still path (budget >= 1M samples) keeps it.
    if total >= 1_000_000 && img.width as u64 * img.height as u64 >= 500_000 {
        let (uw, uh) = ((img.width * 2) as usize, (img.height * 2) as usize);
        if uw * uh <= 9_000_000 {
            let (_, _, data) = upscale2_packed(img);
            let big = BitImage::new(uw as u32, uh as u32, &data);
            let hits = decode_dm_at(&big, 0.5, exhaustive, total / 2);
            if !hits.is_empty() {
                return hits;
            }
        }
    }
    Vec::new()
}

/// DM decode over `img`, with result quads mapped back through an
/// `f`x resample (f=1 = plain; f may be fractional — 0.5 maps quads of
/// a 2x-upscaled mask back to native coordinates).
/// `exhaustive` enables the reference sweep fallback after the fast
/// locator misses — only affordable at 4x downscale.
fn decode_dm_at(img: &BitImage, f: f32, exhaustive: bool, budget: u64) -> Vec<barcode_core::DecodeResult> {
    let w = img.width as usize;
    let h = img.height as usize;
    let mut dark = vec![false; w * h];
    for y in 0..h as u32 {
        for x in 0..w as u32 {
            dark[y as usize * w + x as usize] = img.ink(x, y);
        }
    }
    let map = barcode_dm::locator::BinaryMap { w, h, dark };
    // Downscaled masks: the fast leg-derived locator first (~ms); the
    // exhaustive sweep only where it is cheap (4x — its sweep is ~4x
    // smaller than 2x's). Native-resolution masks keep the reference
    // locator only. This bounds the worst case per frame so the frame
    // budget survives geometry that defeats corner formation.
    let locs = if f >= 2.0 {
        let fast = barcode_dm::locator::locate_binary_fast_budget(&map, budget / 2);
        if !fast.is_empty() {
            fast
        } else if exhaustive {
            barcode_dm::locator::locate_binary_budget(&map, budget / 2)
        } else {
            Vec::new()
        }
    } else {
        // Native resolution. The reference locator (`locate_binary`) is
        // unbudgeted and measured 1.2s on a phone for a junk frame (the
        // live path ran it on EVERY frame). The leg-derived fast locator
        // is ~ms and the downscaled legs below already cover the
        // high-resolution/fragmented-leg cases. Fast first everywhere;
        // the reference sweep stays the still-image fallback (exhaustive)
        // and only runs when the fast candidates verify nothing — a fast
        // false positive must not shadow it.
        barcode_dm::locator::locate_binary_fast_budget(&map, budget)
    };
    let verified = |locs: Vec<barcode_dm::locator::Location>| {
        locs.into_iter()
            // Corners whose RS verification failed carry no payload — they are
            // locator false positives (text/graphics), not symbols. (They only
            // surfaced once collect_bars scanned the whole non-square frame.)
            .filter_map(|loc| {
                let payload = loc.decoded.payload.as_ref()?;
                let text = barcode_core::payload_text(payload);
                let mut position: barcode_core::Quad =
                    [barcode_core::Point { x: 0.0, y: 0.0 }; 4];
                for (i, (x, y)) in loc.quad().into_iter().enumerate() {
                    position[i] = barcode_core::Point { x: x as f32 * f, y: y as f32 * f };
                }
                Some(barcode_core::DecodeResult {
                    symbology: barcode_core::Symbology::DataMatrix,
                    text,
                    raw: payload.to_vec(),
                    position,
                })
            })
            .collect::<Vec<_>>()
    };
    let hits = verified(locs);
    if hits.is_empty() && f < 2.0 {
        // Budgeted reference fallback. The unbudgeted `locate_binary`
        // (6x SAMPLE_BUDGET) was the 1.2s live footgun; an explicit
        // allowance keeps the still path exhaustive and the live path
        // bounded. Small masks (fixture canvases) are cheap here — the
        // sample budget never exceeds their pixel count.
        return verified(barcode_dm::locator::locate_binary_budget(&map, budget));
    }
    hits
}

/// The PDF417 tier: scan-line start/stop detection over the same 1bpp
/// mask the other tiers consume. Public for native tests; `scan_tiered`
/// is the production caller.
pub fn decode_pdf417_mask(img: &BitImage) -> Vec<barcode_core::DecodeResult> {
    let w = img.width as usize;
    let h = img.height as usize;
    // Try one packed orientation: scanline first (cheap), then the
    // rectified quad detector (shear/crease).
    let decode_packed = |packed: &[u8], pw: usize, ph: usize| -> Option<barcode_pdf417::detect::FrameDecode> {
        let pimg = BitImage::new(pw as u32, ph as u32, packed);
        let mut dark = vec![0u8; pw * ph];
        for y in 0..ph {
            for x in 0..pw {
                dark[y * pw + x] = pimg.ink(x as u32, y as u32) as u8;
            }
        }
        barcode_pdf417::read_frame(&dark, pw, ph)
            .or_else(|| barcode_pdf417::read_frame_rectified(&pimg))
    };
    // Boarding passes are printed landscape and scanned portrait: the
    // symbol arrives upside down (field report, 2026-11-12), and a code
    // can be presented in any cardinal rotation. Each orientation is one
    // packed linear copy + a quad remap; a residual roll is the caller's
    // tilt-ladder business.
    let map180 = |mut hit: barcode_pdf417::detect::FrameDecode| {
        let q = hit.quad;
        let mut mapped = [0f64; 8];
        for i in 0..4 {
            // TL,TR,BR,BL after 180° = BR,BL,TL,TR of the original.
            let j = (i + 2) % 4;
            mapped[i * 2] = (w as f64 - 1.0) - q[j * 2];
            mapped[i * 2 + 1] = (h as f64 - 1.0) - q[j * 2 + 1];
        }
        hit.quad = mapped;
        hit
    };
    // dest dims are (h, w) for both quarter turns.
    let map90 = |mut hit: barcode_pdf417::detect::FrameDecode| {
        let q = hit.quad;
        for i in 0..4 {
            let (x, y) = (q[i * 2], q[i * 2 + 1]);
            hit.quad[i * 2] = y; // src x = dest Y
            hit.quad[i * 2 + 1] = h as f64 - 1.0 - x; // src y = h-1-dest X
        }
        hit
    };
    let map270 = |mut hit: barcode_pdf417::detect::FrameDecode| {
        let q = hit.quad;
        for i in 0..4 {
            let (x, y) = (q[i * 2], q[i * 2 + 1]);
            hit.quad[i * 2] = w as f64 - 1.0 - y; // src x = w-1-dest Y
            hit.quad[i * 2 + 1] = x; // src y = dest X
        }
        hit
    };
    let rev = img.rot180();
    let quarter = |clockwise: bool| rot90_packed(img, clockwise);
    let hit = decode_packed(img.data, w, h)
        .or_else(|| decode_packed(&rev, w, h).map(map180))
        .or_else(|| {
            let packed = quarter(true);
            decode_packed(&packed, h, w).map(map90)
        })
        .or_else(|| {
            let packed = quarter(false);
            decode_packed(&packed, h, w).map(map270)
        });
    hit
        .map(|hit| {
            let mut position: barcode_core::Quad =
                [barcode_core::Point { x: 0.0, y: 0.0 }; 4];
            for i in 0..4 {
                position[i] = barcode_core::Point {
                    x: hit.quad[2 * i] as f32,
                    y: hit.quad[2 * i + 1] as f32,
                };
            }
            barcode_core::DecodeResult {
                symbology: barcode_core::Symbology::PDF417,
                text: barcode_core::payload_text(&hit.payload),
                raw: hit.payload.clone(),
                position,
            }
        })
        .into_iter()
        .collect()
}

/// Quarter-turn of the packed mask; the dimensions swap. The PDF417
/// scanline/quad detectors are upright-only, so a cardinal rotation is
/// undone here before detection and the result quad mapped back.
fn rot90_packed(img: &BitImage, clockwise: bool) -> Vec<u8> {
    let w = img.width as usize;
    let h = img.height as usize;
    let (dw, dh) = (h, w);
    let dbpr = dw.div_ceil(8).div_ceil(4) * 4;
    let mut out = vec![0u8; dbpr * dh];
    for y in 0..h {
        for x in 0..w {
            if !img.ink(x as u32, y as u32) {
                continue;
            }
            let (dx, dy) = if clockwise { (h - 1 - y, x) } else { (y, w - 1 - x) };
            out[dy * dbpr + (dx >> 3)] |= 1 << (dx & 7);
        }
    }
    out
}

/// The Aztec tier: bullseye location + corner-mark orientation + per-size
/// extraction over the same 1bpp mask the other tiers consume. The mode
/// message's check nibbles arbitrate size and core class, so wrong
/// guesses fail fast. Public for native tests; `scan_tiered` is the
/// production caller.
pub fn decode_aztec_mask(img: &BitImage) -> Vec<barcode_core::DecodeResult> {
    let w = img.width as usize;
    let h = img.height as usize;
    let mut dark = vec![0u8; w * h];
    for y in 0..h as u32 {
        for x in 0..w as u32 {
            dark[y as usize * w + x as usize] = img.ink(x, y) as u8;
        }
    }
    barcode_aztec::read_frame(&dark, w, h)
        .into_iter()
        .map(|hit| {
            let payload = barcode_aztec::text::decode_text(&hit.stream);
            let text = barcode_core::payload_text(&payload);
            let mut position: barcode_core::Quad = [barcode_core::Point { x: 0.0, y: 0.0 }; 4];
            for i in 0..4 {
                position[i] = barcode_core::Point {
                    x: hit.quad[2 * i],
                    y: hit.quad[2 * i + 1],
                };
            }
            barcode_core::DecodeResult {
                symbology: barcode_core::Symbology::Aztec,
                text,
                raw: payload,
                position,
            }
        })
        .collect()
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

/// Stage-by-stage QR diagnosis for remote debugging: reports how far the
/// pipeline gets on one frame and what each stage produced. The finders
/// are (x, y, module) triples in detection order; `stage_failed` names the
/// first stage that returned nothing.
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

#[derive(Serialize)]
struct GridInfo {
    size: u32,
    tl: [f32; 2],
    step_x: [f32; 2],
    step_y: [f32; 2],
}

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

/// Smallest Micro QR tier that accepts `text`, rendered with a 4-module
/// quiet zone. Shared by the fixture arm and the round-trip test.
fn microqr_canvas(text: &str, module: u32) -> Result<Canvas, JsValue> {
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
        .ok_or_else(|| JsValue::from_str("payload too large for Micro QR"))?;
    let quiet = 4u32;
    let dim = (sym.size as u32 + 2 * quiet) * module;
    let mut c = Canvas::new(dim, dim);
    for y in 0..sym.size {
        for x in 0..sym.size {
            if sym.cells[y * sym.size + x] == 1 {
                c.fill((quiet + x as u32) * module, (quiet + y as u32) * module, module, module);
            }
        }
    }
    Ok(c)
}

/// Draw a `#`/`.` oracle matrix (1 char = 1 module) onto a Canvas with a
/// `quiet`-module margin. Used by the frozen-specimen fixture tests.
#[cfg(test)]
fn matrix_canvas(pattern: &str, rows: u32, cols: u32, module: u32, quiet: u32) -> Canvas {
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
struct EncOptions {
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
        Self { ec: -1, mask: -1, version: 0, level: -1, columns: 0, rows: 0, cols: 0 }
    }
}

impl EncOptions {
    fn validate(self) -> Result<Self, JsValue> {
        let bad = |m: &str| JsValue::from_str(m);
        if !(-1..=3).contains(&self.ec) {
            return Err(bad("ec must be L, M, Q or H"));
        }
        if !(-1..=7).contains(&self.mask) {
            return Err(bad("mask must be 0..7"));
        }
        if !(0..=40).contains(&self.version) {
            return Err(bad("version must be 1..40"));
        }
        if !(-1..=8).contains(&self.level) {
            return Err(bad("level must be 0..8 (PDF417)"));
        }
        if !(0..=30).contains(&self.columns) {
            return Err(bad("columns must be 1..30 (PDF417)"));
        }
        if self.rows < 0 || self.cols < 0 || (self.rows == 0) != (self.cols == 0) {
            return Err(bad("rows and cols must both be set, or both 0"));
        }
        Ok(self)
    }
    fn qr_ec(&self) -> u8 {
        if self.ec < 0 { 0 } else { self.ec as u8 }
    }
    fn qr_mask(&self) -> u8 {
        if self.mask < 0 { 2 } else { self.mask as u8 }
    }
    fn qr_version(&self) -> u32 {
        self.version.max(0) as u32
    }
    fn pdf_level(&self) -> u8 {
        if self.level < 0 { barcode_pdf417::encode::DEFAULT_LEVEL } else { self.level as u8 }
    }
}

/// Render a synthetic specimen of `symbology` for the self-test harness.
/// Returns the 1bpp mask { width, height, bytesPerRow, data } in the same
/// layout the GPU binarizer produces, so the harness can round-trip it
/// through the exact video pipeline.
///
/// This is the no-options form. [`encode_fixture_opts`] is the same
/// builder with QR/PDF417/DataMatrix layout options.
#[wasm_bindgen]
pub fn encode_fixture(symbology: &str, text: &str, module: u32, height: u32) -> Result<JsValue, JsValue> {
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
    let o = EncOptions { ec, mask, version, level, columns, rows, cols }.validate()?;
    encode_fixture_impl(symbology, text, module, height, o)
}

fn encode_fixture_impl(
    symbology: &str,
    text: &str,
    module: u32,
    height: u32,
    o: EncOptions,
) -> Result<JsValue, JsValue> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Enc {
        width: u32,
        height: u32,
        bytes_per_row: usize,
        data: Vec<u8>,
    }
    let quiet = 10 * module;
    let canvas = match symbology {
        "ean13" => barcode_fixtures::encode_ean13(text, module, height, quiet),
        "code39" => barcode_fixtures::encode_code39(text, module, 3.0, height),
        "code32" => {
            let raw = code32_raw(text)
                .ok_or_else(|| JsValue::from_str("code32 needs A + 9 digits with a valid check digit"))?;
            barcode_fixtures::encode_code39(&raw, module, 3.0, height)
        }
        "itf" => barcode_fixtures::encode_itf(text, module, 2.5, height),
        "code128" => barcode_fixtures::encode_code128(text, module, height),
        "code93" => barcode_fixtures::encode_code93(text, module, height),
        "codabar" => barcode_fixtures::encode_codabar(text, module, 2.5, height),
        "ean8" => barcode_fixtures::encode_ean8(text, module, height, 10 * module),
        "upce" => barcode_fixtures::encode_upce(text, module, height, 10 * module),
        "upca" => barcode_fixtures::encode_upca(text, module, height, 10 * module),
        "databar" | "databaromni" => {
            if text.len() != 13 || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(JsValue::from_str("databar needs exactly 13 digits"));
            }
            let v: u64 = text.parse().map_err(|_| JsValue::from_str("bad databar value"))?;
            let runs = barcode_fixtures::databar_omni_runs(v);
            barcode_fixtures::databar_omni_canvas(&runs, module, height, 10 * module)
        }
        "databarltd" | "databarlimited" => {
            if text.len() != 13 || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(JsValue::from_str("databarltd needs exactly 13 digits"));
            }
            let v: u64 = text.parse().map_err(|_| JsValue::from_str("bad databar value"))?;
            let runs = barcode_fixtures::databar_ltd_runs(v);
            barcode_fixtures::databar_ltd_canvas(&runs, module, height, 10 * module)
        }
        "databarstk" | "databarstacked" => {
            let v: u64 = text.parse().map_err(|_| JsValue::from_str("bad databar value"))?;
            let runs = barcode_fixtures::databar_omni_runs(v);
            barcode_fixtures::databar_stacked_canvas(&runs, module, 10 * module, false)
        }
        "databarstackomni" => {
            let v: u64 = text.parse().map_err(|_| JsValue::from_str("bad databar value"))?;
            let runs = barcode_fixtures::databar_omni_runs(v);
            barcode_fixtures::databar_stacked_canvas(&runs, module, 10 * module, true)
        }
        // QR: EC M / mask 2 / smallest version by default; all three are
        // overridable. `height` is meaningless for 2D and ignored.
        "qr" | "qrcode" => barcode_fixtures::encode_qr_text_version(
            text, o.qr_ec(), o.qr_mask(), o.qr_version(), module,
        )
        .ok_or_else(|| JsValue::from_str("payload too large for QR at the requested version/EC"))?,
        // Micro QR: full encoder in barcode-microqr — pick the smallest
        // version/level tier that accepts the payload (auto mode, class 0).
        "microqr" => microqr_canvas(text, module)?,
        // Datamatrix: the real ECC200 encoder (ASCII + upper shift mode,
        // smallest symbol that fits). Aztec / PDF417: encoder half not
        // written yet — render a frozen oracle specimen
        // (barcode_fixtures::oracle_matrices).
        "datamatrix" => {
            let sized = o.rows > 0;
            let enc = (if sized {
                barcode_dm::encode::encode_as(text.as_bytes(), o.rows as usize, o.cols as usize)
            } else {
                barcode_dm::encode::encode(text.as_bytes())
            })
            .ok_or_else(|| {
                JsValue::from_str(if sized {
                    "payload does not fit the requested DataMatrix size"
                } else {
                    "payload too large for DataMatrix"
                })
            })?;
            let (r, c) = (enc.spec.rows, enc.spec.cols);
            let mut canvas = Canvas::new((c as u32 + 8) * module, (r as u32 + 8) * module);
            for y in 0..r {
                for x in 0..c {
                    if enc.matrix[y * c + x] == 1 {
                        canvas.fill((4 + x as u32) * module, (4 + y as u32) * module, module, module);
                    }
                }
            }
            canvas
        }
        // Aztec: the real encoder (mode machine + window-relative stuff
        // model + RS over raw windows; byte-identical to zint for clean
        // payloads). `height` is meaningless for 2D and ignored.
        "aztec" => {
            let (spec, matrix) = barcode_aztec::encode::encode(text.as_bytes())
                .map_err(|_| JsValue::from_str("payload too large for Aztec"))?;
            let (r, c) = (spec.rows as usize, spec.cols as usize);
            let mut canvas = Canvas::new((c as u32 + 8) * module, (r as u32 + 8) * module);
            for y in 0..r {
                for x in 0..c {
                    if matrix[y * c + x] == 1 {
                        canvas.fill((4 + x as u32) * module, (4 + y as u32) * module, module, module);
                    }
                }
            }
            canvas
        }
        // PDF417: the real encoder (TC/BC/NC compaction, GF(929) ECC,
        // zint-fitted geometry). `height` is meaningless for 2D.
        "pdf417" => {
            let level = o.pdf_level();
            let sym = (if o.columns > 0 {
                barcode_pdf417::encode::encode_full_opts(text.as_bytes(), level, o.columns as u8)
            } else {
                barcode_pdf417::encode::encode_full(text.as_bytes(), level)
            })
            .map_err(|_| JsValue::from_str("payload too large for PDF417 at the requested level/columns"))?;
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
        _ => return Err(JsValue::from_str("unknown symbology")),
    };
    let enc = Enc {
        width: canvas.width,
        height: canvas.height,
        bytes_per_row: canvas.bytes_per_row,
        data: canvas.bits().to_vec(),
    };
    serde_wasm_bindgen::to_value(&enc).map_err(|_| JsValue::from_str("serialize failed"))
}

/// Shared `Enc` serialization for the fixture builders.
fn fixture_enc(canvas: Canvas) -> Result<JsValue, JsValue> {
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

/// Binary specimen canvas builder shared by `encode_fixture_hex` and
/// `encode_fixture_raw`. QR frames the bytes as a byte-mode segment
/// (smallest fitting version, EC M); MicroQR, DataMatrix, Aztec and
/// PDF417 take the bytes through their real encoders.
fn fixture_bytes_canvas(
    symbology: &str,
    data: &[u8],
    module: u32,
    o: EncOptions,
) -> Result<Canvas, JsValue> {
    Ok(match symbology {
        "qr" | "qrcode" => {
            // Byte mode (4-bit mode + count + bytes). The count indicator
            // is 8 bits for v1-9 and 16 bits for v10-40. `o.version`
            // forces a version; otherwise the smallest that fits.
            let ec = o.qr_ec();
            let mask = o.qr_mask();
            let forced = o.qr_version();
            let mut found = None;
            let versions = if (1..=40).contains(&forced) { forced..=forced } else { 1..=40 };
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
                return Err(JsValue::from_str("payload too large for QR at the requested version/EC"));
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
                barcode_dm::encode::encode_as(&data, o.rows as usize, o.cols as usize)
            } else {
                barcode_dm::encode::encode(&data)
            })
            .ok_or_else(|| {
                JsValue::from_str(if sized {
                    "payload does not fit the requested DataMatrix size"
                } else {
                    "payload too large for DataMatrix"
                })
            })?;
            let (r, c) = (enc.spec.rows, enc.spec.cols);
            let mut canvas = Canvas::new((c as u32 + 8) * module, (r as u32 + 8) * module);
            for y in 0..r {
                for x in 0..c {
                    if enc.matrix[y * c + x] == 1 {
                        canvas.fill((4 + x as u32) * module, (4 + y as u32) * module, module, module);
                    }
                }
            }
            canvas
        }
        "aztec" => {
            let (spec, matrix) = barcode_aztec::encode::encode(&data)
                .map_err(|_| JsValue::from_str("payload too large for Aztec"))?;
            let (r, c) = (spec.rows as usize, spec.cols as usize);
            let mut canvas = Canvas::new((c as u32 + 8) * module, (r as u32 + 8) * module);
            for y in 0..r {
                for x in 0..c {
                    if matrix[y * c + x] == 1 {
                        canvas.fill((4 + x as u32) * module, (4 + y as u32) * module, module, module);
                    }
                }
            }
            canvas
        }
        "pdf417" => {
            let level = o.pdf_level();
            let sym = (if o.columns > 0 {
                barcode_pdf417::encode::encode_full_opts(&data, level, o.columns as u8)
            } else {
                barcode_pdf417::encode::encode_full(&data, level)
            })
            .map_err(|_| JsValue::from_str("payload too large for PDF417 at the requested level/columns"))?;
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
                .ok_or_else(|| JsValue::from_str("payload too large for Micro QR"))?;
            let quiet = 4u32;
            let dim = (sym.size as u32 + 2 * quiet) * module;
            let mut c = Canvas::new(dim, dim);
            for y in 0..sym.size {
                for x in 0..sym.size {
                    if sym.cells[y * sym.size + x] == 1 {
                        c.fill((quiet + x as u32) * module, (quiet + y as u32) * module, module, module);
                    }
                }
            }
            c
        }
        _ => {
            return Err(JsValue::from_str(&format!(
                "binary render not supported for {symbology}"
            )))
        }
    })
}

/// Render a synthetic binary specimen (hex payload) for the self-test
/// harness — same mask layout as `encode_fixture`.
#[wasm_bindgen]
pub fn encode_fixture_hex(symbology: &str, hex: &str, module: u32, _height: u32) -> Result<JsValue, JsValue> {
    if hex.len() % 2 != 0 || hex.bytes().any(|b| !b.is_ascii_hexdigit()) {
        return Err(JsValue::from_str("bad hex payload"));
    }
    let data: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
        .collect::<Result<_, _>>()
        .map_err(|_| JsValue::from_str("bad hex payload"))?;
    fixture_enc(fixture_bytes_canvas(symbology, &data, module, EncOptions::default())?)
}

/// Render an exact byte payload — the encode half of `Code.raw`. Same
/// mask layout as `encode_fixture`; symbologies without binary encoders
/// error here (the caller falls back to the text path).
#[wasm_bindgen]
pub fn encode_fixture_raw(symbology: &str, raw: &[u8], module: u32, _height: u32) -> Result<JsValue, JsValue> {
    fixture_enc(fixture_bytes_canvas(symbology, raw, module, EncOptions::default())?)
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
    let o = EncOptions { ec, mask, version, level, columns, rows, cols }.validate()?;
    fixture_enc(fixture_bytes_canvas(symbology, raw, module, o)?)
}

#[cfg(test)]
mod upside_down_tests {
    use super::*;

    /// The render-side bytes builder round-trips through the tiers for
    /// every binary-capable symbology.
    #[test]
    fn fixture_bytes_canvas_round_trips() {
        let data: Vec<u8> = vec![0x00, 0x01, 0x7F, 0x80, 0xFF, 0xFE, b'#'];
        for sym in ["qrcode", "microqr", "datamatrix", "aztec", "pdf417"] {
            let canvas =
                fixture_bytes_canvas(sym, &data, 4, EncOptions::default()).unwrap_or_else(|e| panic!("{sym}: {e:?}"));
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert_eq!(hits.len(), 1, "{sym}: expected exactly one hit, got {:?}", hits.iter().map(|h| (&h.text, h.raw.len())).collect::<Vec<_>>());
            assert_eq!(hits[0].raw, data, "{sym}: raw mismatch");
        }
    }

    /// Binary payloads through the 2D tiers: `raw` must carry the exact
    /// bytes while `text` is the lossy display form.
    #[test]
    fn binary_payloads_survive_the_2d_tiers() {
        let data: Vec<u8> = vec![0x00, 0xFF, 0x7F, 0x80, 0xFE];

        let enc = barcode_dm::encode::encode(&data).unwrap();
        let (r, c) = (enc.spec.rows, enc.spec.cols);
        let mut canvas = Canvas::new((c as u32 + 8) * 4, (r as u32 + 8) * 4);
        for y in 0..r {
            for x in 0..c {
                if enc.matrix[y * c + x] == 1 {
                    canvas.fill((4 + x as u32) * 4, (4 + y as u32) * 4, 4, 4);
                }
            }
        }
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let hits = decode_dm_at(&img, 4.0, false, u64::MAX);
        assert_eq!(hits.len(), 1, "dm");
        assert_eq!(hits[0].raw, data, "dm raw");

        let sym = barcode_pdf417::encode::encode_full(&data, barcode_pdf417::encode::DEFAULT_LEVEL)
            .unwrap();
        let m = 4;
        let mut canvas = Canvas::new((sym.width as u32 + 8) * m, (sym.height as u32 + 8) * m);
        for y in 0..sym.height {
            for x in 0..sym.width {
                if sym.matrix[y * sym.width + x] == 1 {
                    canvas.fill((4 + x as u32) * m, (4 + y as u32) * m, m, m);
                }
            }
        }
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let hits = decode_pdf417_mask(&img);
        assert_eq!(hits.len(), 1, "pdf417");
        assert_eq!(hits[0].raw, data, "pdf417 raw");

        let (spec, matrix) = barcode_aztec::encode::encode(&data).unwrap();
        let (r, c) = (spec.rows as usize, spec.cols as usize);
        let mut canvas = Canvas::new((c as u32 + 8) * 4, (r as u32 + 8) * 4);
        for y in 0..r {
            for x in 0..c {
                if matrix[y * c + x] == 1 {
                    canvas.fill((4 + x as u32) * 4, (4 + y as u32) * 4, 4, 4);
                }
            }
        }
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let hits = decode_aztec_mask(&img);
        assert_eq!(hits.len(), 1, "aztec");
        assert_eq!(hits[0].raw, data, "aztec raw");
    }

    #[test]
    fn code128_fixture_decodes_short_payloads() {
        // ISO/IEC 15417 permits one data codeword ("A") and two ("AB").
        // The old gate required three and rejected those symbols even
        // though the check digit validated.
        for text in ["A", "AB", "1", "12"] {
            let canvas = barcode_fixtures::encode_code128(text, 4, 80);
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert!(
                hits.iter().any(|r| r.text == text),
                "short {text:?}: got {:?}",
                hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn code128_fixture_round_trips_every_control_byte() {
        // Not one GS1 payload: every C0 control plus DEL must survive the
        // fixture encoder -> scanner path, so the encoder cannot be
        // special-cased to a single string. Chunked to stay inside the
        // decoder's existing length heuristics (see the tracked issue for
        // the separate short-symbol and >30-codeword limits).
        let controls: Vec<char> = (0u8..=31).chain([127]).map(char::from).collect();
        for chunk in controls.chunks(8) {
            let mid: String = chunk.iter().collect();
            let text = format!("A{mid}Z");
            let canvas = barcode_fixtures::encode_code128(&text, 4, 80);
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert!(
                hits.iter().any(|r| r.text == text),
                "control-byte round-trip {text:?}: got {:?}",
                hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn databar_stacked_tier_upright_and_upside_down() {
        let v = 4006381333931u64;
        for &omni in &[false, true] {
            let runs = barcode_fixtures::databar_omni_runs(v);
            let canvas = barcode_fixtures::databar_stacked_canvas(&runs, 4, 2, omni);
            let want = "(01)40063813339314";

            // Upright, through the full tiered scan.
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
            assert!(
                hits.iter()
                    .any(|r| r.symbology == barcode_core::Symbology::DataBarStacked
                        && r.text == want),
                "upright tier miss (omni={omni}): {hits:?}"
            );

            // Upside down: only the 180° pass may read it.
            let rot_bits = rot180(&canvas);
            let rot = BitImage::new(canvas.width, canvas.height, &rot_bits);
            let mut sc2 = BarcodeScanner::new();
            let hits2 = sc2.scan_tiered(&rot, &Tiers::all(), false, 0);
            assert!(
                hits2.iter().any(|r| r.text == want),
                "rot180 tier miss (omni={omni}): {hits2:?}"
            );
        }
    }

    #[test]
    fn databar_ltd_tier_upright_and_upside_down() {
        let v = 1_234_567_890_123u64;
        let runs = barcode_fixtures::databar_ltd_runs(v);
        let canvas = barcode_fixtures::databar_ltd_canvas(&runs, 4, 40, 2);
        let body = format!("{v:013}");
        let sum: usize = body
            .bytes()
            .enumerate()
            .map(|(i, b)| (b - b'0') as usize * if i % 2 == 0 { 3 } else { 1 })
            .sum();
        let want = format!("(01){body}{}", (10 - sum % 10) % 10);

        // Upright, through the full tiered scan.
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut sc = BarcodeScanner::new();
        let hits = sc.scan_tiered(&img, &Tiers::all(), false, 0);
        assert!(
            hits.iter()
                .any(|r| r.symbology == barcode_core::Symbology::DataBarLimited && r.text == want),
            "upright tier miss: {hits:?}"
        );

        // Upside down: the mirrored linear pass must read it.
        let rot_bits = rot180(&canvas);
        let rot = BitImage::new(canvas.width, canvas.height, &rot_bits);
        let mut sc2 = BarcodeScanner::new();
        let hits2 = sc2.scan_tiered(&rot, &Tiers::all(), false, 0);
        assert!(
            hits2.iter().any(|r| r.text == want),
            "rot180 tier miss: {hits2:?}"
        );

        // The formats filter keeps the tier addressable.
        let mut sc3 = BarcodeScanner::new();
        let only = parse_formats(&["databarltd".into()]).unwrap();
        assert!(sc3.scan_tiered(&img, &only, false, 0).iter().any(|r| r.text == want));
        let off = parse_formats(&["ean13".into()]).unwrap();
        assert!(sc3.scan_tiered(&img, &off, false, 0).is_empty());
    }

    fn rot180(canvas: &barcode_fixtures::Canvas) -> Vec<u8> {
        // A 180° rotation of the packed bitmap: what the camera sees when a
        // barcode is presented upside down.
        let w = canvas.width as usize;
        let h = canvas.height as usize;
        let bpr = canvas.bytes_per_row as usize;
        let src = canvas.bits();
        let mut out = vec![0u8; bpr * h];
        for y in 0..h {
            for x in 0..w {
                let v = (src[(h - 1 - y) * bpr + ((w - 1 - x) >> 3)] >> ((w - 1 - x) & 7)) & 1;
                if v == 1 {
                    out[y * bpr + (x >> 3)] |= 1 << (x & 7);
                }
            }
        }
        out
    }

    #[test]
    fn decodes_linear_barcodes_presented_upside_down() {
        // (digits, encoder, expected symbology via the decoder that hits)
        let cases: Vec<(String, barcode_fixtures::Canvas)> = vec![
            (
                "4006381333931".into(),
                barcode_fixtures::encode_ean13("4006381333931", 4, 80, 4),
            ),
            (
                "CODE 39",
                barcode_fixtures::encode_code39("CODE 39", 4, 2.0, 80),
            )
            .into_text(),
            (
                "12345670".into(),
                barcode_fixtures::encode_ean8("12345670", 4, 80, 4),
            ),
            (
                "1234567895".into(),
                barcode_fixtures::encode_itf("1234567895", 4, 2.0, 80),
            )
            .into_text(),
            (
                "9501101531000".into(),
                barcode_fixtures::encode_ean13("9501101531000", 4, 80, 4),
            ),
            (
                "Code 128!".into(),
                barcode_fixtures::encode_code128("Code 128!", 4, 80),
            ),
            (
                // GS1-128: FNC1 decodes back to GS (0x1D), so the payload
                // must survive a re-encode after a scan.
                "0112345678901231\u{1d}10ABC".into(),
                barcode_fixtures::encode_code128("0112345678901231\u{1d}10ABC", 4, 80),
            ),
            (
                "CODE 93".into(),
                barcode_fixtures::encode_code93("CODE 93", 4, 80),
            ),
            (
                // Codabar results carry the body only — sentinels are structure.
                "40156".into(),
                barcode_fixtures::encode_codabar("40156", 4, 2.5, 80),
            ),
        ];
        for (want, canvas) in cases {
            let bits = rot180(&canvas);
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut sc = BarcodeScanner::new();
            // The QR tier must stay silent on linear input...
            assert!(
                sc.qr.decode(&img).is_empty(),
                "2D tier fired on a linear canvas"
            );
            let hits = decode_linear(sc.linear_decoders(), &img, &Tiers::all(), true, true);
            assert!(
                hits.iter().any(|r| r.text == want),
                "upside-down decode of {want:?}: got {:?}",
                hits.iter().map(|r| (&r.symbology, &r.text)).collect::<Vec<_>>(),
            );
            // The hit must sit inside the frame (x coordinates mirrored back).
            for r in hits.iter().filter(|r| r.text == want) {
                let xs: Vec<f32> = r.position.iter().map(|p| p.x).collect();
                assert!(xs.iter().cloned().fold(f32::MAX, f32::min) >= 0.0);
                assert!(
                    xs.iter().cloned().fold(f32::MIN, f32::max) <= img.width as f32,
                    "x out of frame: {xs:?}"
                );
            }
        }
    }

    #[test]
    fn decodes_italian_pharmacode_code32() {
        // A real box: the six Code 39 symbols VYTH1V carry the printed AIC
        // value A938261563. `code32` must relabel it; plain `code39` must
        // keep the carrier payload.
        let canvas = barcode_fixtures::encode_code39("VYTH1V", 4, 3.0, 80);
        let img = BitImage::new(canvas.width, canvas.height, canvas.bits());
        let mut sc = BarcodeScanner::new();

        let all = sc.scan_tiered(&img, &Tiers::all(), false, 0);
        let hit = all.first().expect("code32 specimen did not decode");
        assert_eq!(hit.symbology, barcode_core::Symbology::Code32, "{all:?}");
        assert_eq!(hit.text, "A938261563");

        let only32 = parse_formats(&["code32".into()]).unwrap();
        assert!(sc.scan_tiered(&img, &only32, false, 0).iter().any(|r| r.text == "A938261563"));

        let only39 = parse_formats(&["code39".into()]).unwrap();
        assert!(sc.scan_tiered(&img, &only39, false, 0).iter().any(|r| r.text == "VYTH1V"));
    }

    trait IntoText {
        fn into_text(self) -> (String, barcode_fixtures::Canvas);
    }
    impl IntoText for (&'static str, barcode_fixtures::Canvas) {
        fn into_text(self) -> (String, barcode_fixtures::Canvas) {
            (self.0.into(), self.1)
        }
    }
}

#[cfg(test)]
mod formats_tests {
    use super::*;

    #[test]
    fn formats_filter_selects_decode_tiers() {
        let qr_canvas = {
            let data: Vec<u8> = (0..19u8).collect(); // v1-L capacity
            barcode_fixtures::encode_qr(1, 1, 2, &data, 4)
        };
        let ean_canvas = barcode_fixtures::encode_ean13("4006381333931", 4, 80, 4);

        let qr_img = BitImage::new(qr_canvas.width, qr_canvas.height, qr_canvas.bits());
        let ean_img = BitImage::new(ean_canvas.width, ean_canvas.height, ean_canvas.bits());
        let mut sc = BarcodeScanner::new();

        let count = |r: &Vec<barcode_core::DecodeResult>| r.len();

        // QR canvas, everything: decodes.
        assert_eq!(count(&sc.scan_tiered(&qr_img, &Tiers::all(), false, 0)), 1);

        // QR canvas, 1D-only formats: the QR tier is off — nothing.
        let one_d = parse_formats(&["ean13".into(), "code128".into()]).unwrap();
        assert_eq!(count(&sc.scan_tiered(&qr_img, &one_d, false, 0)), 0, "1D filter must silence the QR tier");

        // QR canvas, qrcode-only: decodes.
        let qr_only = parse_formats(&["qr".into()]).unwrap();
        assert_eq!(count(&sc.scan_tiered(&qr_img, &qr_only, false, 0)), 1, "qrcode filter must keep the QR tier");

        // EAN-13 canvas, qrcode-only: nothing (and no junk 1D votes).
        assert_eq!(count(&sc.scan_tiered(&ean_img, &qr_only, false, 0)), 0, "qrcode filter must not fire on 1D input");

        // EAN-13 canvas, ean13-only: decodes.
        let ean_only = parse_formats(&["ean13".into()]).unwrap();
        let hits = sc.scan_tiered(&ean_img, &ean_only, false, 0);
        assert_eq!(count(&hits), 1, "ean13 filter must decode EAN-13");
        assert_eq!(hits[0].text, "4006381333931");

        // Unknown names throw.
        assert!(parse_formats(&["qrcodes".into()]).is_err());
        // Alias + case/spacing normalization.
        assert!(parse_formats(&["  QR ".into()]).unwrap().qr);
        assert!(parse_formats(&["datamatrix".into()]).unwrap().datamatrix);
    }

    /// Embedded zint 12x36 canonical matrix for b"DETECTOR-TEST-123"
    /// (independent of the field fixture; col 0 + row 11 are the finder L).
    const TEST_MATRIX: [&str; 12] = [
        "#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.",
        "#.#.#.##....##..#######.####.#.#.#.#",
        "##.#...###.######.#.####...##..#.##.",
        "##.##..##.#.###.#########.##...#.###",
        "#.#..#...#..##....##..##.#..#.#.##..",
        "#.##...#.###.#...####..#..#.########",
        "##.######.#..##...#...#.....#.#...#.",
        "###.#.##......#.######....#...#..#.#",
        "##..#.#...#.##.##.##.####....##.##..",
        "#.........##..#####.#.#...#.###...##",
        "###..##.#..#####..###.####...#.#....",
        "####################################",
    ];

    #[test]
    fn datamatrix_tier_runs_2d_first_and_respects_formats() {
        // draw the 12x36 symbol upright (L bottom-left) at pitch 8 on a
        // white canvas, packed as the GPU mask
        let (w, h) = (420usize, 420usize);
        let pitch = 8usize;
        let rows = TEST_MATRIX.len();
        let cols = TEST_MATRIX[0].len();
        let bpr = w.div_ceil(8).div_ceil(4) * 4;
        let mut mask = vec![0u8; bpr * h];
        for r in 0..rows {
            for c in 0..cols {
                if TEST_MATRIX[r].as_bytes()[c] != b'#' {
                    continue;
                }
                // upright: L corner at bottom-left; module (r,c) center at
                // (x0 + (c+0.5)*pitch, y0 + (r+0.5)*pitch) with the L
                // corner at (x0, y0 + rows*pitch)
                let cx = 40 + (c * pitch) + pitch / 2;
                let cy = 40 + (r * pitch) + pitch / 2;
                for dy in 0..pitch {
                    for dx in 0..pitch {
                        let x = cx - pitch / 2 + dx;
                        let y = cy - pitch / 2 + dy;
                        mask[y * bpr + (x >> 3)] |= 1 << (x & 7);
                    }
                }
            }
        }
        let img = BitImage::new(w as u32, h as u32, &mask);
        let mut sc = BarcodeScanner::new();
        let count = |r: &Vec<barcode_core::DecodeResult>| r.len();

        // datamatrix-only filter: decodes.
        let dm_only = parse_formats(&["datamatrix".into()]).unwrap();
        let hits = sc.scan_tiered(&img, &dm_only, false, 0);
        assert_eq!(count(&hits), 1, "datamatrix filter must decode DM");
        assert_eq!(hits[0].symbology, barcode_core::Symbology::DataMatrix);
        assert_eq!(hits[0].text, "DETECTOR-TEST-123");

        // 1D-only filter: the DM tier is off - nothing.
        let one_d = parse_formats(&["ean13".into()]).unwrap();
        assert_eq!(count(&sc.scan_tiered(&img, &one_d, false, 0)), 0, "1D filter must silence the DM tier");

        // all tiers: DM hits 2D-first.
        assert_eq!(count(&sc.scan_tiered(&img, &Tiers::all(), false, 0)), 1);
    }

    /// Embedded zint 'B' vers-2 compact oracle (19x19, docs/STATUS.md;
    /// same matrix as barcode-aztec's golden test).
    const AZTEC_B: [&str; 19] = [
        "..#.#.##.##..##..#.",
        ".#.#..##..###..#.#.",
        "##.#..#.#.#.#..###.",
        "..#..####.##.####.#",
        "..####.#......#...#",
        "#...############..#",
        "#....#.......#.###.",
        "######.#####.#.###.",
        "###..#.#...#.#.##.#",
        "#...##.#.#.#.#.##..",
        "##..##.#...#.###.#.",
        "#.#.##.#####.#..#.#",
        "...#.#.......####..",
        ".##..##########....",
        "##.....#...#....#..",
        ".#...#..##..####...",
        "#.######..#.###.#.#",
        "#.##...#####..#...#",
        "..####..#.#.##..##.",
    ];

    fn draw_aztec(rows: &[&str], pitch: usize, quiet: usize) -> (Vec<u8>, u32, u32) {
        let size = rows.len();
        let dim = (size + 2 * quiet) * pitch;
        let bpr = dim.div_ceil(8).div_ceil(4) * 4;
        let mut mask = vec![0u8; bpr * dim];
        for r in 0..size {
            for c in 0..size {
                if rows[r].as_bytes()[c] != b'#' {
                    continue;
                }
                for dy in 0..pitch {
                    for dx in 0..pitch {
                        let x = (c + quiet) * pitch + dx;
                        let y = (r + quiet) * pitch + dy;
                        mask[y * bpr + (x >> 3)] |= 1 << (x & 7);
                    }
                }
            }
        }
        (mask, dim as u32, dim as u32)
    }

    #[test]
    fn aztec_tier_decodes_and_respects_formats() {
        let (mask, w, h) = draw_aztec(&AZTEC_B, 8, 4);
        let img = BitImage::new(w, h, &mask);
        let mut sc = BarcodeScanner::new();
        let count = |r: &Vec<barcode_core::DecodeResult>| r.len();

        let az_only = parse_formats(&["aztec".into()]).unwrap();
        let hits = sc.scan_tiered(&img, &az_only, false, 0);
        assert_eq!(count(&hits), 1, "aztec filter must decode Aztec");
        assert_eq!(hits[0].symbology, barcode_core::Symbology::Aztec);
        assert_eq!(hits[0].text, "B");
        // quad: TL corner module center ≈ half a module into the quiet zone.
        assert!((hits[0].position[0].x - 36.0).abs() < 1.0, "{:?}", hits[0].position);

        // 1D-only filter: the Aztec tier is off - nothing.
        let one_d = parse_formats(&["ean13".into()]).unwrap();
        assert_eq!(count(&sc.scan_tiered(&img, &one_d, false, 0)), 0, "1D filter must silence the Aztec tier");

        // All tiers: still exactly one hit (QR/DM tiers find nothing here).
        assert_eq!(count(&sc.scan_tiered(&img, &Tiers::all(), false, 0)), 1);

        // 90°-rotated symbol still decodes.
        let size = AZTEC_B.len();
        let mut rot = vec![vec![0u8; size]; size];
        for r in 0..size {
            for c in 0..size {
                rot[c][size - 1 - r] = (AZTEC_B[r].as_bytes()[c] == b'#') as u8;
            }
        }
        let rot_rows: Vec<String> = rot.iter().map(|r| r.iter().map(|&b| if b == 1 { '#' } else { '.' }).collect()).collect();
        let rot_refs: Vec<&str> = rot_rows.iter().map(|s| s.as_str()).collect();
        let (rmask, rw, rh) = draw_aztec(&rot_refs, 8, 4);
        let rimg = BitImage::new(rw, rh, &rmask);
        let rhits = sc.scan_tiered(&rimg, &az_only, false, 0);
        assert_eq!(count(&rhits), 1, "rotated aztec must decode");
        assert_eq!(rhits[0].text, "B");
    }
}

#[cfg(test)]
mod oracle_fixture_tests {
    use super::*;

    /// Every banked oracle specimen must decode through the production
    /// mask decoders. Validates the bank's polarity AND keeps the fixture
    /// bank (used by show.html / e2e / harden) in sync with the decoders.
    fn img_of(canvas: &Canvas) -> BitImage<'static> {
        // leak for the test's lifetime; the canvases are small
        let bits: &'static [u8] = Box::leak(canvas.bits().to_vec().into_boxed_slice());
        BitImage::new(canvas.width, canvas.height, bits)
    }

    #[test]
    fn oracle_specimens_round_trip() {
        let mut failures = Vec::new();
        for (sym, text, m) in barcode_fixtures::oracle_matrices::bank() {
            let canvas = matrix_canvas(m.pattern, m.rows, m.cols, 4, 4);
            let mut hits = match *sym {
                "datamatrix" => decode_datamatrix_mask(&img_of(&canvas)),
                "aztec" => decode_aztec_mask(&img_of(&canvas)),
                "pdf417" => decode_pdf417_mask(&img_of(&canvas)),
                other => panic!("uncovered symbology {other} in oracle bank"),
            };
            if hits.is_empty() && *sym == "datamatrix" {
                // experiment: does the tight (no quiet) render decode?
                let tight = matrix_canvas(m.pattern, m.rows, m.cols, 4, 0);
                hits = decode_datamatrix_mask(&img_of(&tight));
                if !hits.is_empty() {
                    failures.push(format!("{sym} {text:?}: DECODES ONLY WITHOUT QUIET"));
                    continue;
                }
            }
            let ok = hits.len() == 1 && hits[0].text == *text;
            if !ok {
                failures.push(format!(
                    "{sym} {text:?} ({}x{}): {:?}",
                    m.rows,
                    m.cols,
                    hits.iter().map(|h| &h.text).collect::<Vec<_>>(),
                ));
            }
        }
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    /// The Micro QR fixture arm's tier pick must produce a decodable symbol.
    #[test]
    fn microqr_fixture_round_trip() {
        for text in ["12345", "HELLO", "ABC-DEF"] {
            let canvas = microqr_canvas(text, 4).unwrap_or_else(|e| panic!("{text}: {e:?}"));
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let hits = decode_microqr(&img);
            assert_eq!(hits.len(), 1, "{text:?}: {} hits", hits.len());
            assert_eq!(hits[0].text, text, "{text:?} payload");
        }
    }
}

#[cfg(test)]
mod frame_dump_tests {
    use super::*;

    /// Native diagnosis of real camera frames: DUMP=<file.bits>
    /// [FORMATS=csv] cargo test decode_dump_frame -- --nocapture
    /// Dump layout: u32 w, h, bytes_per_row, flags LE, then mask bytes.
    #[test]
    fn decode_dump_frame() {
        let Ok(path) = std::env::var("DUMP") else { return };
        let formats: Vec<String> = std::env::var("FORMATS")
            .map(|s| s.split(',').map(str::to_string).collect())
            .unwrap_or_default();
        let raw = std::fs::read(&path).expect("read dump");
        let w = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        let h = u32::from_le_bytes(raw[4..8].try_into().unwrap());
        let bpr = u32::from_le_bytes(raw[8..12].try_into().unwrap());
        let bits = &raw[16..];
        assert_eq!(bits.len(), bpr as usize * h as usize, "dump size mismatch");
        let img = BitImage::new(w, h, bits);
        println!("frame {w}x{h} bpr {bpr}");
        let tiers = if formats.is_empty() { Tiers::all() } else { parse_formats(&formats).unwrap() };
        let t0 = std::time::Instant::now();
        let hits = BarcodeScanner::new().scan_tiered(&img, &tiers, false, 0);
        println!("{:?}: {} hits in {:?}", formats, hits.len(), t0.elapsed());
        for hit in &hits {
            println!("  {:?}: {:?}", hit.symbology, hit.text);
        }
        assert!(!hits.is_empty(), "no decode");
    }
}

