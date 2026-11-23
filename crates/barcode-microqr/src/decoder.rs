//! Micro QR decoder: single-finder detection → rolled grid sampling →
//! ISO/IEC 18004 BCH format decode → GF(2^8) Reed-Solomon correction →
//! multi-segment bitstream decode.

use barcode_core::geometry as geom;
use barcode_core::{BitImage, DecodeResult, Point, RowRuns, Symbology};
use barcode_qr::rs::{rs_correct, rs_encode, Gf};

use crate::{
    count_bits, format_word, is_function, mask_bit, mode_bits, term_bits, Spec, Version, ALNUM,
    FORMAT_COORDS, SPECS,
};

/// A located finder pattern: pixel coords of the symbol's module (0,0)
/// top-left corner and the module pitch.
#[derive(Debug, Clone, Copy)]
pub struct Finder {
    pub x0: f32,
    pub y0: f32,
    pub pitch: f32,
}

const FINDER_PAT: [[u8; 7]; 7] = [
    [1, 1, 1, 1, 1, 1, 1],
    [1, 0, 0, 0, 0, 0, 1],
    [1, 0, 1, 1, 1, 0, 1],
    [1, 0, 1, 1, 1, 0, 1],
    [1, 0, 1, 1, 1, 0, 1],
    [1, 0, 0, 0, 0, 0, 1],
    [1, 1, 1, 1, 1, 1, 1],
];

/// Sample the ink at a pixel point, returning `false` outside image bounds.
#[inline]
fn sample(image: &BitImage, x: f32, y: f32) -> bool {
    let (xi, yi) = (x.round() as i64, y.round() as i64);
    xi >= 0
        && yi >= 0
        && xi < i64::from(image.width)
        && yi < i64::from(image.height)
        && image.ink(xi as u32, yi as u32)
}

/// Verify the 7x7 finder pattern whose top-left module starts at
/// (x0, y0), allowing up to 4 mismatched modules (print noise).
fn verify_finder(image: &BitImage, x0: f32, y0: f32, pitch: f32) -> bool {
    let mut misses = 0;
    for (j, row) in FINDER_PAT.iter().enumerate() {
        for (i, &want) in row.iter().enumerate() {
            let px = x0 + (i as f32 + 0.5) * pitch;
            let py = y0 + (j as f32 + 0.5) * pitch;
            if sample(image, px, py) != (want == 1) {
                misses += 1;
                if misses > 4 {
                    return false;
                }
            }
        }
    }
    true
}

/// Locate the single Micro QR finder (7x7 1:1:3:1:1 pattern).
pub fn find_finder(image: &BitImage) -> Option<Finder> {
    let mut row = RowRuns::with_capacity(256);
    for y in 0..image.height {
        row.scan_row(image, y);
        let n = row.runs.len();
        if n < 6 {
            continue;
        }
        // In `RowRuns`, odd indices are always ink runs (index 0 is background).
        let mut w = 1usize;
        while w + 4 < n {
            let d1 = row.runs[w].1;
            let l1 = row.runs[w + 1].1;
            let d2 = row.runs[w + 2].1;
            let l2 = row.runs[w + 3].1;
            let d3 = row.runs[w + 4].1;
            let u = d2 as f32 / 3.0;
            let ok = |v: u32, t: f32| (v as f32 - t).abs() <= 0.4 * u;
            if u >= 1.0 && ok(d1, u) && ok(l1, u) && ok(l2, u) && ok(d3, u) {
                let pitch = (d1 + l1 + d2 + l2 + d3) as f32 / 7.0;
                let x = row.offsets[w];
                // Candidate row is finder row 2..=4; find the top edge.
                for top_off in 2..=4u32 {
                    let y0 = y as f32 - top_off as f32 * pitch;
                    if verify_finder(image, x as f32, y0, pitch) {
                        return Some(Finder { x0: x as f32, y0, pitch });
                    }
                }
            }
            w += 2;
        }
    }
    None
}

/// The finder's frame: the symbol's in-plane roll and its finder (0,0)
/// corner (origin), both from the two finder edges that border the quiet
/// zone / separator, plus pitch from the central 3x3 block area.
fn finder_frame(image: &BitImage, f: &Finder) -> Option<(f32, (f32, f32), f32)> {
    let p = f.pitch;
    if p <= 0.0 {
        return None;
    }
    let span = (7.0 * p).round() as i64;
    let (x0, y0) = (f.x0.round() as i64, f.y0.round() as i64);
    let (sx0, sy0) = ((x0 - 2).max(0), (y0 - 2).max(0));
    let (sx1, sy1) = (
        (x0 + span + 2).min(i64::from(image.width) - 1),
        (y0 + span + 2).min(i64::from(image.height) - 1),
    );
    let (xmin, xmax) = ((x0 + span / 5).max(0), (x0 + span * 4 / 5).min(sx1));
    let (ymin, ymax) = ((y0 + span / 5).max(0), (y0 + span * 4 / 5).min(sy1));
    if xmax <= xmin || ymax <= ymin || sx1 <= sx0 || sy1 <= sy0 {
        return None;
    }
    // Top boundary: topmost ink pixel per column, fitted as y = a·x + b.
    let (mut n, mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    for x in xmin..=xmax {
        let mut top = None;
        for y in sy0..=sy1 {
            if image.ink(x as u32, y as u32) {
                top = Some(y);
                break;
            }
        }
        if let Some(y) = top {
            let (xf, yf) = (x as f64, y as f64);
            n += 1.0;
            sx += xf;
            sy += yf;
            sxx += xf * xf;
            sxy += xf * yf;
        }
    }
    if n < 5.0 {
        return None;
    }
    let den = n * sxx - sx * sx;
    if den.abs() < 1e-9 {
        return None;
    }
    let a = (n * sxy - sx * sy) / den;
    let b = (sy - a * sx) / n;
    // Left boundary: leftmost ink pixel per row, fitted as x = c·y + d.
    let (mut m, mut uy, mut ux, mut uyy, mut uxy) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    for y in ymin..=ymax {
        let mut left = None;
        for x in sx0..=sx1 {
            if image.ink(x as u32, y as u32) {
                left = Some(x);
                break;
            }
        }
        if let Some(x) = left {
            let (xf, yf) = (x as f64, y as f64);
            m += 1.0;
            uy += yf;
            ux += xf;
            uyy += yf * yf;
            uxy += xf * yf;
        }
    }
    if m < 5.0 {
        return None;
    }
    let den2 = m * uyy - uy * uy;
    if den2.abs() < 1e-9 {
        return None;
    }
    let c = (m * uxy - uy * ux) / den2;
    let d = (ux - c * uy) / m;
    let theta = 0.5 * (a.atan() - c.atan());
    if theta.abs() > 0.35 || (a.atan() + c.atan()).abs() > 0.1 {
        return None;
    }
    let den3 = 1.0 - a * c;
    if den3.abs() < 1e-9 {
        return None;
    }
    let ox = ((c * b + d) / den3) as f32;
    let oy = (a * ox as f64 + b) as f32;
    let area = center_block_area(image, f);
    if area < 3.0 * p * p || area > 16.0 * p * p {
        return None;
    }
    let pitch = (area / 9.0).sqrt();
    if pitch < 0.4 * p || pitch > 2.5 * p {
        return None;
    }
    Some((theta as f32, (ox, oy), pitch))
}

/// Area (pixels) of the finder's central 3x3 block.
fn center_block_area(image: &BitImage, f: &Finder) -> f32 {
    let p = f.pitch;
    let (sx, sy) = (
        (f.x0 + 3.5 * p).round() as i64,
        (f.y0 + 3.5 * p).round() as i64,
    );
    if sx < 0
        || sy < 0
        || sx >= i64::from(image.width)
        || sy >= i64::from(image.height)
        || !image.ink(sx as u32, sy as u32)
    {
        return 0.0;
    }
    let r = (3.0 * p).ceil() as i64;
    let (bx0, by0) = ((sx - r).max(0), (sy - r).max(0));
    let (bx1, by1) = (
        (sx + r).min(i64::from(image.width) - 1),
        (sy + r).min(i64::from(image.height) - 1),
    );
    let (bw, bh) = ((bx1 - bx0 + 1) as usize, (by1 - by0 + 1) as usize);
    let mut seen = vec![false; bw * bh];
    let mut stack = vec![(sx, sy)];
    let mut n = 0u32;
    while let Some((x, y)) = stack.pop() {
        if x < bx0 || y < by0 || x > bx1 || y > by1 {
            continue;
        }
        let i = (y - by0) as usize * bw + (x - bx0) as usize;
        if seen[i] {
            continue;
        }
        seen[i] = true;
        if !image.ink(x as u32, y as u32) {
            continue;
        }
        n += 1;
        stack.extend([(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]);
    }
    n as f32
}

/// Sample a `size`x`size` symbol through an affine map anchored at `anchor`
/// with module pitch `pitch` and roll `theta` (radians).
fn sample_rolled(
    image: &BitImage,
    anchor: (f32, f32),
    pitch: f32,
    theta: f32,
    size: usize,
) -> (Vec<u8>, [Point; 4]) {
    let (s, c) = (f64::from(theta.sin()), f64::from(theta.cos()));
    let p = f64::from(pitch);
    let h = [
        p * c,
        -p * s,
        f64::from(anchor.0),
        p * s,
        p * c,
        f64::from(anchor.1),
        0.0,
        0.0,
        1.0,
    ];
    let cells: Vec<u8> = if pitch >= 6.0 {
        geom::sample_grid(image, &h, size as u32, size as u32)
            .bits
            .iter()
            .map(|&b| b as u8)
            .collect()
    } else {
        let mut cells = Vec::with_capacity(size * size);
        for v in 0..size {
            for u in 0..size {
                let (x, y) = geom::apply_h(&h, u as f64 + 0.5, v as f64 + 0.5);
                let (xi, yi) = (x.round() as i64, y.round() as i64);
                cells.push(
                    (xi >= 0
                        && yi >= 0
                        && xi < i64::from(image.width)
                        && yi < i64::from(image.height)
                        && image.ink(xi as u32, yi as u32)) as u8,
                );
            }
        }
        cells
    };
    let mut quad = [Point { x: 0.0, y: 0.0 }; 4];
    let n = size as f64;
    for (k, (u, v)) in [(0.0, 0.0), (n, 0.0), (n, n), (0.0, n)].iter().enumerate() {
        let (x, y) = geom::apply_h(&h, *u, *v);
        quad[k] = Point { x: x as f32, y: y as f32 };
    }
    (cells, quad)
}

/// BCH(15, 5) format decode (min Hamming distance 7 → unique up to 3 bit errors).
fn decode_format(cells: &[u8], size: usize, version: Version) -> Option<(&'static Spec, u8)> {
    let mut raw = 0u16;
    for &(x, y) in &FORMAT_COORDS {
        raw = (raw << 1) | u16::from(cells[y * size + x] & 1);
    }
    let mut best_dist = u32::MAX;
    let mut best = (0u8, 0u8);
    for sym in 0..8u8 {
        for mask in 0..4u8 {
            let d = (raw ^ format_word(sym, mask)).count_ones();
            if d < best_dist {
                best_dist = d;
                best = (sym, mask);
            }
        }
    }
    if best_dist > 3 {
        return None;
    }
    let spec = &SPECS[best.0 as usize];
    if spec.version != version {
        return None;
    }
    Some((spec, best.1))
}

/// Parse ISO/IEC 18004 Micro QR data segments and verify terminator + padding.
fn parse_bits(data_cws: &[u8], spec: &Spec) -> Option<Vec<u8>> {
    // Expand data codewords back into exact `spec.data_bits` bits.
    let full_bytes = spec.dc - usize::from(spec.half_last);
    let mut bits = Vec::with_capacity(spec.data_bits);
    for &byte in &data_cws[..full_bytes] {
        for i in (0..8).rev() {
            bits.push((byte >> i) & 1);
        }
    }
    if spec.half_last {
        let last = data_cws[full_bytes];
        // The 4 low bits of M1/M3's half-codeword are structurally always 0.
        if last & 0x0F != 0 {
            return None;
        }
        for i in (4..8).rev() {
            bits.push((last >> i) & 1);
        }
    }

    let total = bits.len();
    let mut pos = 0usize;
    let take = |pos: &mut usize, n: usize| -> Option<u32> {
        if *pos + n > total {
            return None;
        }
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | u32::from(bits[*pos]);
            *pos += 1;
        }
        Some(v)
    };

    let mb = mode_bits(spec.version);
    let tb = term_bits(spec.version);
    let mut out = Vec::new();

    while pos < total {
        let rem = total - pos;
        if rem < tb {
            // Truncated terminator at end of capacity: remaining bits must be 0.
            if bits[pos..total].iter().any(|&b| b != 0) {
                return None;
            }
            pos = total;
            break;
        }
        // Full terminator: `tb` consecutive zero bits (equivalently mode=0, count=0).
        if bits[pos..pos + tb].iter().all(|&b| b == 0) {
            pos += tb;
            break;
        }
        let mode = if mb == 0 { 0u8 } else { take(&mut pos, mb)? as u8 };
        let cb = count_bits(spec.version, mode)?;
        let count = take(&mut pos, cb)? as usize;
        if count == 0 {
            return None;
        }
        match mode {
            0 => {
                // Numeric
                let mut left = count;
                while left >= 3 {
                    let v = take(&mut pos, 10)?;
                    if v >= 1000 {
                        return None;
                    }
                    out.push(b'0' + (v / 100) as u8);
                    out.push(b'0' + ((v / 10) % 10) as u8);
                    out.push(b'0' + (v % 10) as u8);
                    left -= 3;
                }
                if left == 2 {
                    let v = take(&mut pos, 7)?;
                    if v >= 100 {
                        return None;
                    }
                    out.push(b'0' + (v / 10) as u8);
                    out.push(b'0' + (v % 10) as u8);
                } else if left == 1 {
                    let v = take(&mut pos, 4)?;
                    if v >= 10 {
                        return None;
                    }
                    out.push(b'0' + v as u8);
                }
            }
            1 => {
                // Alphanumeric
                let mut left = count;
                while left >= 2 {
                    let v = take(&mut pos, 11)?;
                    if v >= 45 * 45 {
                        return None;
                    }
                    out.push(ALNUM[(v / 45) as usize]);
                    out.push(ALNUM[(v % 45) as usize]);
                    left -= 2;
                }
                if left == 1 {
                    let v = take(&mut pos, 6)?;
                    if v >= 45 {
                        return None;
                    }
                    out.push(ALNUM[v as usize]);
                }
            }
            2 => {
                // Byte
                for _ in 0..count {
                    out.push(take(&mut pos, 8)? as u8);
                }
            }
            _ => return None,
        }
    }

    if out.is_empty() {
        return None;
    }

    // Verify ISO/IEC 18004 padding after terminator:
    // 1) zero bit-padding up to the next 8-bit boundary (or end of capacity)
    let full_bits = full_bytes * 8;
    if pos < full_bits {
        let pad_zero_end = (pos.div_ceil(8) * 8).min(full_bits);
        if bits[pos..pad_zero_end].iter().any(|&b| b != 0) {
            return None;
        }
        pos = pad_zero_end;
        // 2) alternating 0xEC / 0x11 pad codewords
        let mut pad_idx = 0usize;
        while pos < full_bits {
            let cw = take(&mut pos, 8)? as u8;
            let want = if pad_idx % 2 == 0 { 0xEC } else { 0x11 };
            if cw != want {
                return None;
            }
            pad_idx += 1;
        }
    }
    // 3) trailing 4-bit codeword padding (M1/M3) must be all zeros
    if bits[pos..total].iter().any(|&b| b != 0) {
        return None;
    }

    Some(out)
}

/// Decode a sampled module grid (row-major 0/1) of the given size (`11, 13, 15, 17`);
/// returns the decoded payload bytes.
pub fn decode_grid(cells: &[u8], size: usize) -> Option<Vec<u8>> {
    let version = match size {
        11 => Version::M1,
        13 => Version::M2,
        15 => Version::M3,
        17 => Version::M4,
        _ => return None,
    };
    if cells.len() != size * size {
        return None;
    }

    // Cheap structural reject: verify timing patterns and finder separator.
    let max_timing_err = if version == Version::M1 { 1 } else { 2 };
    let mut timing_err = 0usize;
    for k in 8..size {
        let want = u8::from(k % 2 == 0);
        if cells[k] != want {
            timing_err += 1;
        }
        if cells[k * size] != want {
            timing_err += 1;
        }
        if timing_err > max_timing_err {
            return None;
        }
    }
    let max_sep_err = if version == Version::M1 { 2 } else { 3 };
    let mut sep_err = 0usize;
    for k in 0..=7 {
        if cells[7 * size + k] != 0 {
            sep_err += 1;
        }
        if k < 7 && cells[k * size + 7] != 0 {
            sep_err += 1;
        }
        if sep_err > max_sep_err {
            return None;
        }
    }

    // 1. Decode 15-bit BCH format word (spec + mask).
    let (spec, mask) = decode_format(cells, size, version)?;

    // 2. Read zigzag stream and unmask.
    let total_bits = spec.data_bits + spec.ec * 8;
    let mut stream = Vec::with_capacity(total_bits);
    let mut up = true;
    let mut col = size - 1;
    while col >= 1 {
        for step in 0..size - 1 {
            let y = if up { size - 1 - step } else { 1 + step };
            for x in [col, col - 1] {
                if is_function(x, y) {
                    continue;
                }
                let bit = (cells[y * size + x] & 1) ^ u8::from(mask_bit(mask, x, y));
                stream.push(bit);
            }
        }
        up = !up;
        if col < 2 {
            break;
        }
        col -= 2;
    }
    if stream.len() != total_bits {
        return None;
    }

    // 3. Pack data + EC codewords (M1/M3's 4-bit codeword in the upper nibble).
    let full_bytes = spec.dc - usize::from(spec.half_last);
    let mut cws = Vec::with_capacity(spec.dc + spec.ec);
    for chunk in stream[..full_bytes * 8].chunks_exact(8) {
        cws.push(chunk.iter().fold(0u8, |acc, &b| (acc << 1) | b));
    }
    if spec.half_last {
        let nib = stream[full_bytes * 8..spec.data_bits]
            .iter()
            .fold(0u8, |acc, &b| (acc << 1) | b);
        cws.push(nib << 4);
    }
    for chunk in stream[spec.data_bits..].chunks_exact(8) {
        cws.push(chunk.iter().fold(0u8, |acc, &b| (acc << 1) | b));
    }

    // 4. Reed-Solomon verification / error correction via `barcode_qr::rs`.
    let gf = Gf::new();
    if spec.ec == 2 {
        // M1 is error detection only (ISO/IEC 18004 Table 1): require 0 errors.
        if rs_encode(&gf, &cws[..spec.dc], 2) != cws[spec.dc..] {
            return None;
        }
    } else {
        rs_correct(&gf, &mut cws, spec.ec)?;
    }

    // 5. Decode data segments and strictly verify terminator + padding.
    parse_bits(&cws[..spec.dc], spec)
}

/// Try all 4 cardinal orientations around a located finder frame `(origin, pitch, theta)`.
fn try_orientations(
    image: &BitImage,
    origin: (f32, f32),
    pitch: f32,
    theta: f32,
) -> Option<(Vec<u8>, [Point; 4])> {
    let (s, c) = (theta.sin(), theta.cos());
    let span = 7.0 * pitch;
    // Finder local corners (0,0), (7,0), (7,7), (0,7) for 0°/90°/180°/270° CW.
    let orientations = [
        (origin, theta),
        ((origin.0 + span * c, origin.1 + span * s), theta + std::f32::consts::FRAC_PI_2),
        (
            (origin.0 + span * (c - s), origin.1 + span * (s + c)),
            theta + std::f32::consts::PI,
        ),
        (
            (origin.0 - span * s, origin.1 + span * c),
            theta + 3.0 * std::f32::consts::FRAC_PI_2,
        ),
    ];
    for (anchor, angle) in orientations {
        for size in [11usize, 13, 15, 17] {
            let (cells, quad) = sample_rolled(image, anchor, pitch, angle, size);
            if let Some(payload) = decode_grid(&cells, size) {
                return Some((payload, quad));
            }
        }
    }
    None
}

/// Decode Micro QR symbols from a bilevel image. Returns 0 or 1 result.
pub fn decode(image: &BitImage) -> Vec<DecodeResult> {
    let mut out = Vec::new();
    let Some(finder) = find_finder(image) else {
        return out;
    };
    if let Some((seed, origin, pitch)) = finder_frame(image, &finder) {
        for off in [0.0f32, -0.5, 0.5, -1.0, 1.0, -1.5, 1.5, -2.0, 2.0] {
            let theta = seed + f32::to_radians(off);
            if let Some((payload, quad)) = try_orientations(image, origin, pitch, theta) {
                let text = barcode_core::payload_text(&payload);
                out.push(DecodeResult {
                    symbology: Symbology::QRCodeMicro,
                    raw: payload,
                    text,
                    position: quad,
                });
                return out;
            }
        }
        return out;
    }
    // Fallback axis-aligned sampler when the finder's central block is smeared.
    if let Some((payload, quad)) = try_orientations(image, (finder.x0, finder.y0), finder.pitch, 0.0) {
        let text = barcode_core::payload_text(&payload);
        out.push(DecodeResult {
            symbology: Symbology::QRCodeMicro,
            raw: payload,
            text,
            position: quad,
        });
    }
    out
}
