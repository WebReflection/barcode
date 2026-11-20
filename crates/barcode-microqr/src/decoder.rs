//! Micro QR decoder: single-finder detection → grid sampling →
//! frame-matching decode.
//!
//! Design note: instead of parsing the format word, the decoder tries
//! every (frame, mask class) candidate of the detected version and lets
//! GF(2) consistency plus an exact re-encode comparison arbitrate. A
//! symbol decodes iff some frame's affine model `cells = G·f ⊕ K`
//! explains every data cell AND the rebuilt matrix matches the sampled
//! grid exactly — the ECC embedded in G provides the redundancy.

use barcode_core::geometry as geom;
use barcode_core::{BitImage, DecodeResult, Point, Symbology};

use crate::frames::{self, Frame, FRAMES};

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

/// Sample the ink at a pixel point (single pixel; canvas/quantized
/// inputs are phase-correct by construction of the finder pitch).
fn sample(image: &BitImage, x: f32, y: f32) -> bool {
    image.ink(x.round() as u32, y.round() as u32)
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

/// Locate the single Micro QR finder (top-left of the symbol).
pub fn find_finder(image: &BitImage) -> Option<Finder> {
    for y in 0..image.height {
        let runs: Vec<(bool, u32)> = image.row_runs(y).collect();
        if runs.len() < 5 {
            continue;
        }
        // prefix sums: left edge of run i
        let mut edges = vec![0u32; runs.len() + 1];
        for (i, (_, r)) in runs.iter().enumerate() {
            edges[i + 1] = edges[i] + r;
        }
        for w in 0..=runs.len() - 5 {
            let (c1, d1) = runs[w];
            let (c2, l1) = runs[w + 1];
            let (c3, d2) = runs[w + 2];
            let (c4, l2) = runs[w + 3];
            let (c5, d3) = runs[w + 4];
            let matched = c1 && !c2 && c3 && !c4 && c5;
            let u = d2 as f32 / 3.0;
            let ok = |v: u32, t: f32| (v as f32 - t).abs() <= 0.4 * u;
            if !(matched && u >= 1.0 && ok(d1, u) && ok(l1, u) && ok(l2, u) && ok(d3, u))
            {
                continue;
            }
            let pitch = (d1 + l1 + d2 + l2 + d3) as f32 / 7.0;
            let x = edges[w];
            // the candidate row is finder row 2..=4; find the top edge
            for top_off in 2..=4u32 {
                let y0 = y as f32 - top_off as f32 * pitch;
                if verify_finder(image, x as f32, y0, pitch) {
                    return Some(Finder { x0: x as f32, y0, pitch });
                }
            }
        }
    }
    None
}

/// Sample the module grid of a symbol whose module (0,0) starts at
/// (fx, fy) with the given pitch; row-major 0/1 cells.
fn sample_grid(image: &BitImage, fx: f32, fy: f32, pitch: f32, size: usize) -> Vec<u8> {
    let mut cells = vec![0u8; size * size];
    for j in 0..size {
        for i in 0..size {
            let px = fx + (i as f32 + 0.5) * pitch;
            let py = fy + (j as f32 + 0.5) * pitch;
            cells[j * size + i] = sample(image, px, py) as u8;
        }
    }
    cells
}

/// The finder's frame: the symbol's in-plane roll and its module (0,0)
/// corner (origin), both from the two finder edges that border the quiet
/// zone.
///
/// A rolled symbol defeats an axis-aligned sampler: at pitch 4 an 11-module
/// symbol drifts a whole module across its far side by 4° (the phone tilt
/// bank missed MicroQR at ±4°). The finder's outer top and left edges are
/// straight lines against the quiet zone and are long (7 modules), so a
/// least-squares fit gives the roll to ~0.5° at the smallest supported
/// pitch; their intersection is the symbol's own corner. Only the roll is
/// searched around this seed (the decode match is the arbiter).
fn finder_frame(image: &BitImage, f: &Finder) -> Option<(f32, (f32, f32), f32)> {
    let p = f.pitch;
    if p <= 0.0 {
        return None;
    }
    let span = (7.0 * p).round() as i64;
    let (x0, y0) = (f.x0.round() as i64, f.y0.round() as i64);
    // Scan domain: the finder's box with a couple of pixels of slack. The
    // *middle* 60% restriction applies to the positions along the edge
    // (away from the rounded corners), never to the scan depth.
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
    // Perpendicular edges: their slopes must disagree by ~0° after
    // mirroring (top slope = tan θ, left dx/dy = -tan θ). A large
    // disagreement means one fit latched onto data/noise.
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
    // Pitch from the finder's central 3x3 block area (~9p²). The run
    // lengths that `find_finder` measures along a horizontal chord are
    // quantised by ~7 pixel boundaries and read up to 4% long on a rolled
    // symbol — enough to drift a module across the far side. Three
    // modules' worth of area averages that error down to ~0.3%.
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

/// Area (pixels) of the finder's central 3x3 block: an isolated dark
/// component one light ring away from the finder ring.
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

/// Sample a `size`x`size` symbol through an affine map that follows the
/// symbol's own module axes (`theta` = in-plane roll), anchored on the
/// finder's centre block. Uses the shared 3x3-majority sampler, so a
/// one-pixel edge wobble does not flip a module. Returns the cells plus
/// the symbol quad for the result position.
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
        // Field-scale modules: the shared 3x3-majority sampler absorbs a
        // one-pixel print/camera wobble.
        geom::sample_grid(image, &h, size as u32, size as u32)
            .bits
            .iter()
            .map(|&b| b as u8)
            .collect()
    } else {
        // At a 4px module a 3x3 window covers 75% of the cell, so any
        // drift of a few tenths of a pixel flips the vote (measured: one
        // module at p+0.01). The centre pixel is the faithful sample
        // there, exactly as the old axis-aligned sampler did.
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

/// Solve G·f = r over GF(2). G is N rows (cells) x C columns; None if
/// inconsistent.
fn solve_gf2(g: &[u8], n: usize, c: usize, r: &[u8]) -> Option<Vec<u8>> {
    // augmented rows: c coefficient bits + 1 rhs bit, bit-packed
    let words = (c + 1).div_ceil(64);
    let mut aug: Vec<(Vec<u64>, bool)> = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = vec![0u64; words];
        for j in 0..c {
            if g[i * c + j] == 1 {
                row[j / 64] |= 1 << (j % 64);
            }
        }
        aug.push((row, r[i] == 1));
    }
    let mut pivots = vec![usize::MAX; c];
    let mut used = vec![false; n];
    let mut rank = 0usize;
    for col in 0..c {
        let Some(pi) = (0..n).find(|&ri| !used[ri] && (aug[ri].0[col / 64] >> (col % 64)) & 1 == 1)
        else {
            continue;
        };
        aug.swap(rank, pi);
        used[rank] = true;
        let prow = aug[rank].0.clone();
        for ri in 0..n {
            if ri != rank && (aug[ri].0[col / 64] >> (col % 64)) & 1 == 1 {
                for w in 0..words {
                    aug[ri].0[w] ^= prow[w];
                }
                aug[ri].1 ^= aug[rank].1;
            }
        }
        pivots[col] = rank;
        rank += 1;
    }
    for (row, rbit) in &aug {
        let coeff_zero = (0..c).all(|j| (row[j / 64] >> (j % 64)) & 1 == 0);
        if coeff_zero && *rbit {
            return None;
        }
    }
    let mut f = vec![0u8; c];
    for (col, &pr) in pivots.iter().enumerate() {
        if pr != usize::MAX {
            f[col] = aug[pr].1 as u8;
        }
    }
    Some(f)
}

/// Rebuild the full module matrix for a frame/field/class.
fn build_matrix(frame: &Frame, version: u8, f: &[u8], k: &[u8]) -> Vec<u8> {
    let (size, tmpl) = frames::tmpl(version);
    let mut m = tmpl.to_vec();
    for &(x, y, val) in frame.fill {
        m[y as usize * size + x as usize] = val;
    }
    for (i, &(x, y)) in frame.data.iter().enumerate() {
        let mut bit = 0u32;
        for j in 0..frame.cols {
            if f[j] == 1 {
                bit ^= frame.g[i * frame.cols + j] as u32;
            }
        }
        m[y as usize * size + x as usize] = (bit ^ k[i] as u32) as u8;
    }
    m
}

/// Decode a sampled module grid (row-major 0/1) of the given size;
/// returns the payload bytes.
fn decode_grid(cells: &[u8], size: usize) -> Option<Vec<u8>> {
    let version = match size {
        11 => 1,
        13 => 2,
        15 => 3,
        17 => 4,
        _ => return None,
    };
    let levels: &[u8] = match version {
        1 => &[1],
        2 => &[1, 2],
        3 => &[1, 2],
        _ => &[1, 2, 3],
    };
    for (key, frame) in FRAMES.iter() {
        let (v, lv, _mode, _len) = (key[0], key[1], key[2], key[3]);
        if v != version || !levels.contains(&lv) {
            continue;
        }
        let n = frame.data.len();
        // Cheap reject: the fixed modules (finder, format/timing) are
        // listed in `fill` for every frame. A wrong version hypothesis or
        // a mis-rolled sample dies here for ~70 comparisons instead of
        // paying for the GF(2) solve on every frame — the roll search
        // makes that path hot.
        if frame
            .fill
            .iter()
            .any(|&(x, y, val)| cells[y as usize * size + x as usize] != val)
        {
            continue;
        }
        for k in frame.ks {
            let mut r = vec![0u8; n];
            for (i, &(x, y)) in frame.data.iter().enumerate() {
                r[i] = cells[y as usize * size + x as usize] ^ k[i];
            }
            let Some(f) = solve_gf2(frame.g, n, frame.cols, &r) else {
                continue;
            };
            let Some(payload) = fields_to_payload(frame, &f) else {
                continue;
            };
            if build_matrix(frame, version, &f, k) == cells {
                return Some(payload);
            }
        }
    }
    None
}

/// Decode the field values out of a solved f vector; payload bytes if
/// every field value is in range.
fn fields_to_payload(frame: &Frame, f: &[u8]) -> Option<Vec<u8>> {
    let mode = frame.mode;
    let len = frame.len as usize;
    let mode = match mode {
        0 => crate::Mode::Numeric,
        1 => crate::Mode::Alnum,
        _ => crate::Mode::Byte,
    };
    let fw = crate::fields(len, mode);
    const ALNUM: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
    let mut out = Vec::with_capacity(len);
    let mut idx = 0usize;
    for &(n, width) in &fw {
        let mut v: u32 = 0;
        for e in 0..width {
            let bit = if frame.msb_first { f[idx + width - 1 - e] } else { f[idx + e] };
            if bit == 1 {
                v |= 1 << e;
            }
        }
        idx += width;
        match mode {
            crate::Mode::Numeric => {
                if v >= 10u32.pow(n as u32) {
                    return None;
                }
                let mut ds = [0u8; 3];
                let mut vv = v;
                for k in (0..n).rev() {
                    ds[k] = b'0' + (vv % 10) as u8;
                    vv /= 10;
                }
                out.extend_from_slice(&ds[..n]);
            }
            crate::Mode::Alnum => {
                if n == 2 {
                    if v >= 45 * 45 {
                        return None;
                    }
                    out.push(ALNUM[(v / 45) as usize]);
                    out.push(ALNUM[(v % 45) as usize]);
                } else {
                    if v >= 45 {
                        return None;
                    }
                    out.push(ALNUM[v as usize]);
                }
            }
            crate::Mode::Byte => out.push(v as u8),
        }
    }
    Some(out)
}

/// Decode Micro QR symbols from a bilevel image. Returns all decodes
/// (normally 0 or 1).
pub fn decode(image: &BitImage) -> Vec<DecodeResult> {
    let mut out = Vec::new();
    let Some(finder) = find_finder(image) else {
        return out;
    };
    // Users hold phones, not tripods. The centre block pins the grid
    // exactly and seeds the roll; refine that seed by ±2° and let the
    // exact frame match arbitrate. The old axis-aligned sampler tolerated
    // only ~2° at the smallest pitch — beyond that the far side of an
    // 11-module symbol drifts a whole module (the phone tilt bank missed
    // MicroQR at ±4°, where a desktop GPU happened to get away with it).
    if let Some((seed, origin, pitch)) = finder_frame(image, &finder) {
        for off in [0.0f32, -0.5, 0.5, -1.0, 1.0, -1.5, 1.5, -2.0, 2.0] {
            let theta = seed + f32::to_radians(off);
            for size in [11usize, 13, 15, 17] {
                let (cells, quad) = sample_rolled(image, origin, pitch, theta, size);
                let Some(payload) = decode_grid(&cells, size) else {
                    continue;
                };
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
    // No isolated centre block (the finder's white ring is broken, e.g. a
    // heavy smear): keep the axis-aligned sampler as a last resort.
    for size in [11usize, 13, 15, 17] {
        let cells = sample_grid(image, finder.x0, finder.y0, finder.pitch, size);
        let Some(payload) = decode_grid(&cells, size) else {
            continue;
        };
        let text = barcode_core::payload_text(&payload);
        let p = finder.pitch;
        let e = finder.x0 + size as f32 * p;
        let s = finder.y0 + size as f32 * p;
        out.push(DecodeResult {
            symbology: Symbology::QRCodeMicro,
            raw: payload,
            text,
            position: [
                Point { x: finder.x0, y: finder.y0 },
                Point { x: e, y: finder.y0 },
                Point { x: e, y: s },
                Point { x: finder.x0, y: s },
            ],
        });
        break;
    }
    out
}

/// Debug access (examples/tests): expose the solver.
pub fn solve_gf2_pub(g: &[u8], n: usize, c: usize, r: &[u8]) -> Option<Vec<u8>> {
    solve_gf2(g, n, c, r)
}
