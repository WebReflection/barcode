//! Mask-level 2D sweeps: DataMatrix / PDF417 / Aztec detection over one
//! packed bitmap, plus the packed-bit 2x upscale and 90-degree rotation
//! those locators share.

use barcode_core::BitImage;
/// Spread the 8 bits of one byte to every other bit of a u16: each source
/// pixel becomes a 2-pixel run. Half of the DataMatrix 2× upscale leg.
#[inline]
pub(crate) fn spread_byte2x(b: u8) -> u16 {
    let mut x = b as u16;
    x = (x | (x << 4)) & 0x0f0f;
    x = (x | (x << 2)) & 0x3333;
    (x | (x << 1)) & 0x5555
}

/// Nearest-neighbour 2× upscale of a packed bitmap: every source bit is a
/// 2×2 destination block, every source row is written twice.
#[inline]
pub(crate) fn upscale2_packed(img: &BitImage) -> (usize, usize, Vec<u8>) {
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
pub(crate) fn upscale2_packed_simd(img: &BitImage) -> (usize, usize, Vec<u8>) {
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
pub(crate) fn upscale2_packed_scalar(img: &BitImage) -> (usize, usize, Vec<u8>) {
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
pub fn decode_datamatrix_mask_ex(
    img: &BitImage,
    exhaustive: bool,
) -> Vec<barcode_core::DecodeResult> {
    decode_datamatrix_mask_budget(img, exhaustive, 5000)
}

/// Camera-tier entry: the live budget (500ms — two consecutive frames
/// confirm in <1s). Expiry = no-match; the stream retries fresh frames.
pub fn decode_datamatrix_mask_stream(
    img: &BitImage,
    exhaustive: bool,
) -> Vec<barcode_core::DecodeResult> {
    decode_datamatrix_mask_budget(img, exhaustive, 500)
}

/// `budget_ms` bounds the whole DM search for this frame via the
/// calibrated sample allowance (36M charged ~ 4.7s on rig frames).
pub fn decode_datamatrix_mask_budget(
    img: &BitImage,
    exhaustive: bool,
    budget_ms: u64,
) -> Vec<barcode_core::DecodeResult> {
    const SAMPLES_PER_MS: u64 = 7_700;
    let total = budget_ms * SAMPLES_PER_MS;
    // Big masks: the exhaustive pitch sweep on a junk-rich frame costs
    // seconds (a 1080p still measured 40s) — the downscaled copies decode
    // them in tens of ms, and the 2x upscale last resort covers small
    // symbols on big frames. Still images keep the small-mask native pass.
    let big = img.width as u64 * img.height as u64 > 1_200_000;
    // The native reference sweep is the only leg that can see a small code
    // on a big still: the box downgrades crush a ~100px symbol and the 2x
    // upscale lands the grid wrong. Its sample counter charges failed
    // pitch/angle attempts, not wall time (a real 18x18 GS1 DataMatrix
    // needed ~30M counted but decoded in 35ms), so a big still's native
    // pass gets a working floor; the caller's wall deadline bounds it and
    // the corner list is capped for junk-rich frames.
    let native = if big && exhaustive {
        total.max(6_000 * SAMPLES_PER_MS)
    } else {
        total
    };
    let base = if big && !exhaustive {
        Vec::new()
    } else {
        decode_dm_at(img, 1.0, exhaustive, native)
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
            // Fast locator only: the upscaled mask is the biggest one in
            // the ladder and the reference sweep's fixed per-corner costs
            // are not sample-charged (measured 15s on a 1080p still).
            let hits = decode_dm_at(&big, 0.5, false, total / 2);
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
pub(crate) fn decode_dm_at(
    img: &BitImage,
    f: f32,
    exhaustive: bool,
    budget: u64,
) -> Vec<barcode_core::DecodeResult> {
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
                let mut position: barcode_core::Quad = [barcode_core::Point { x: 0.0, y: 0.0 }; 4];
                for (i, (x, y)) in loc.quad().into_iter().enumerate() {
                    position[i] = barcode_core::Point {
                        x: x as f32 * f,
                        y: y as f32 * f,
                    };
                }
                Some(barcode_core::DecodeResult {
                    symbology: barcode_core::Symbology::DataMatrix,
                    rows: loc.rows as u16,
                    cols: loc.cols as u16,
                    text,
                    raw: payload.to_vec(),
                    position,
                })
            })
            .collect::<Vec<_>>()
    };
    let hits = verified(locs);
    if hits.is_empty() && f < 2.0 && exhaustive {
        // Budgeted reference fallback. The unbudgeted `locate_binary`
        // (6x SAMPLE_BUDGET) was the 1.2s live footgun; an explicit
        // allowance keeps the still path exhaustive and the live path
        // bounded. Small masks (fixture canvases) are cheap here — the
        // sample budget never exceeds their pixel count. Big masks cap
        // the corner list instead: the fixed per-corner costs are not
        // charged to the sample budget, and 1080p junk measured 40s.
        const NATIVE_CORNERS: usize = 12;
        let big = w as u64 * h as u64 > 1_200_000;
        return verified(barcode_dm::locator::locate_binary_budget_corners(
            &map,
            budget,
            if big { NATIVE_CORNERS } else { usize::MAX },
        ));
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
    let decode_packed =
        |packed: &[u8], pw: usize, ph: usize| -> Option<barcode_pdf417::detect::FrameDecode> {
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
    hit.map(|hit| {
        let mut position: barcode_core::Quad = [barcode_core::Point { x: 0.0, y: 0.0 }; 4];
        for i in 0..4 {
            position[i] = barcode_core::Point {
                x: hit.quad[2 * i] as f32,
                y: hit.quad[2 * i + 1] as f32,
            };
        }
        barcode_core::DecodeResult {
            symbology: barcode_core::Symbology::PDF417,
            rows: 0,
            cols: 0,
            text: barcode_core::payload_text(&hit.payload),
            raw: hit.payload.clone(),
            position,
        }
    })
    .into_iter()
    .collect()
}

/// Quarter-turn of the packed mask; the dimensions swap. Uses the SWAR 8×8
/// transpose (`rot90_cw`), with CCW derived via SIMD `rot180` of the CW mask.
pub(crate) fn rot90_packed(img: &BitImage, clockwise: bool) -> Vec<u8> {
    let cw = img.rot90_cw();
    if clockwise {
        cw
    } else {
        BitImage::new(img.height, img.width, &cw).rot180()
    }
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
                rows: 0,
                cols: 0,
                text,
                raw: payload,
                position,
            }
        })
        .collect()
}
