//! Planar homography utilities: fitting a projective map from point
//! correspondences (DLT with Hartley normalization, solved by Gaussian
//! elimination on the normal equations — no linear-algebra dependency).
//!
//! Used by the perspective decode path: the three finder patterns yield
//! ~15 module-space ↔ image-space correspondences (their centers and the
//! measured outer-ring edge midpoints), and the fitted H becomes the
//! grid's module → pixel sampler.

/// Apply a 3x3 projective transform (row-major, h8 = 1) to a point.
pub fn apply_h(h: &[f64; 9], x: f64, y: f64) -> (f64, f64) {
    let w = h[6] * x + h[7] * y + h[8];
    (
        (h[0] * x + h[1] * y + h[2]) / w,
        (h[3] * x + h[4] * y + h[5]) / w,
    )
}

/// Fit a homography mapping (x, y) -> (x', y') from >= 4 correspondences,
/// least squares over all of them. Returns None for degenerate input
/// (collinear points, singular system).
pub fn fit_homography(pairs: &[(f64, f64, f64, f64)]) -> Option<[f64; 9]> {
    if pairs.len() < 4 {
        return None;
    }

    // Hartley normalization: translate/scale both point sets to roughly
    // [-1, 1] centred on their centroid. Without it the normal equations
    // are badly conditioned (pixels ~1e3 vs modules ~1e1).
    let norm = |pts: &[(f64, f64)]| -> ([f64; 9], Vec<(f64, f64)>) {
        let n = pts.len() as f64;
        let cx = pts.iter().map(|p| p.0).sum::<f64>() / n;
        let cy = pts.iter().map(|p| p.1).sum::<f64>() / n;
        let avg_dist = pts.iter().map(|p| (p.0 - cx).hypot(p.1 - cy)).sum::<f64>() / n;
        let s = if avg_dist > 1e-9 {
            (2.0f64).sqrt() / avg_dist
        } else {
            1.0
        };
        let t = [s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0];
        let mapped = pts
            .iter()
            .map(|p| (s * (p.0 - cx), s * (p.1 - cy)))
            .collect();
        (t, mapped)
    };

    let src: Vec<(f64, f64)> = pairs.iter().map(|p| (p.0, p.1)).collect();
    let dst: Vec<(f64, f64)> = pairs.iter().map(|p| (p.2, p.3)).collect();
    let (t_src, src_n) = norm(&src);
    let (t_dst, dst_n) = norm(&dst);

    // Build the 2n x 9 DLT system over the normalized points.
    let rows = 2 * src_n.len();
    let mut a = vec![0f64; rows * 9];
    for (i, ((x, y), (u, v))) in src_n.iter().zip(dst_n.iter()).enumerate() {
        let (r0, r1) = a[9 * 2 * i..9 * (2 * i + 2)].split_at_mut(9);
        let (x, y, u, v) = (*x, *y, *u, *v);
        r0.copy_from_slice(&[x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, -u]);
        r1.copy_from_slice(&[0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, -v]);
    }

    // Normal equations M = A^T A (9x9); solve M h = 0 by fixing h8 = 1 —
    // valid whenever the true homography's h8 is not ~0 (no points at
    // infinity, which a symbol's corners never produce).
    let mut m = [[0f64; 9]; 9];
    for r in 0..9 {
        for c in 0..9 {
            m[r][c] = (0..rows).map(|k| a[k * 9 + r] * a[k * 9 + c]).sum();
        }
    }
    // Move the last column to the RHS: solve the leading 8x8 system.
    let mut b = [0f64; 8];
    for r in 0..8 {
        b[r] = -m[r][8];
    }
    let mut lead = [[0f64; 8]; 8];
    for r in 0..8 {
        lead[r].copy_from_slice(&m[r][0..8]);
    }
    let h08 = gauss_solve(&lead, &b)?;

    // Un-normalize: H = T_dst^-1 * H_n * T_src.
    let h_n = [
        h08[0], h08[1], h08[2], h08[3], h08[4], h08[5], h08[6], h08[7], 1.0,
    ];
    let t_dst_inv = invert3(&t_dst)?;
    let tmp = mul3(&t_dst_inv, &h_n);
    let h = mul3(&tmp, &t_src);
    let scale = 1.0 / h[8];
    let mut out = [0f64; 9];
    for (o, v) in out.iter_mut().zip(h.iter()) {
        *o = v * scale;
    }
    Some(out)
}

fn mul3(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut o = [0f64; 9];
    for r in 0..3 {
        for c in 0..3 {
            o[r * 3 + c] = a[r * 3] * b[c] + a[r * 3 + 1] * b[3 + c] + a[r * 3 + 2] * b[6 + c];
        }
    }
    o
}

fn invert3(m: &[f64; 9]) -> Option<[f64; 9]> {
    let (a, b, c, d, e, f, g, hh, i) = (m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8]);
    let det = a * (e * i - f * hh) - b * (d * i - f * g) + c * (d * hh - e * g);
    if det.abs() < 1e-12 {
        return None;
    }
    Some([
        (e * i - f * hh) / det,
        (c * hh - b * i) / det,
        (b * f - c * e) / det,
        (f * g - d * i) / det,
        (a * i - c * g) / det,
        (c * d - a * f) / det,
        (d * hh - e * g) / det,
        (b * g - a * hh) / det,
        (a * e - b * d) / det,
    ])
}

/// Gaussian elimination with partial pivoting. None on singular systems.
fn gauss_solve(a: &[[f64; 8]; 8], b: &[f64; 8]) -> Option<[f64; 8]> {
    let mut a = *a;
    let mut b = *b;
    for col in 0..8 {
        // pivot
        let (piv, _) = (col..8).fold((col, a[col][col].abs()), |(bi, bv), r| {
            if a[r][col].abs() > bv {
                (r, a[r][col].abs())
            } else {
                (bi, bv)
            }
        });
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for r in 0..8 {
            if r != col && a[r][col].abs() > 1e-15 {
                let f = a[r][col] / a[col][col];
                for c in col..8 {
                    a[r][c] -= f * a[col][c];
                }
                b[r] -= f * b[col];
            }
        }
    }
    let mut x = [0f64; 8];
    for r in 0..8 {
        x[r] = b[r] / a[r][r];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A known homography round-trips through the fitter.
    #[test]
    fn fits_a_synthetic_homography() {
        // Build a plausible symbol-plane map: scale + rotation + shear + a
        // strong projective term.
        let h_true = [300.0, 12.0, 40.0, -18.0, 295.0, 60.0, 0.0004, 0.0006, 1.0];
        let pairs: Vec<(f64, f64, f64, f64)> = (0..12)
            .map(|i| {
                let (x, y) = (i as f64 * 7.3, (i * 5 % 11) as f64 * 6.1);
                let (u, v) = apply_h(&h_true, x, y);
                (x, y, u, v)
            })
            .collect();
        let h = fit_homography(&pairs).expect("fit");
        for (x, y, u, v) in &pairs {
            let (pu, pv) = apply_h(&h, *x, *y);
            assert!((pu - u).abs() < 1e-3, "x {x} y {y}: {pu} vs {u}");
            assert!((pv - v).abs() < 1e-3, "x {x} y {y}: {pv} vs {v}");
        }
    }

    #[test]
    fn least_squares_absorbs_noise() {
        let h_true = [80.0, 3.0, 10.0, -4.0, 82.0, 15.0, 0.0002, 0.0001, 1.0];
        let mut pairs = Vec::new();
        for i in 0..15 {
            let (x, y) = (i as f64 * 3.1, (i * 7 % 9) as f64 * 4.0);
            let (mut u, mut v) = apply_h(&h_true, x, y);
            u += ((i * 13 % 7) as f64 - 3.0) * 0.02; // sub-pixel noise
            v += ((i * 11 % 5) as f64 - 2.0) * 0.02;
            pairs.push((x, y, u, v));
        }
        let h = fit_homography(&pairs).expect("fit");
        let (u, v) = apply_h(&h, 10.0, 10.0);
        let (eu, ev) = apply_h(&h_true, 10.0, 10.0);
        assert!(
            (u - eu).abs() < 0.5 && (v - ev).abs() < 0.5,
            "{u} {v} vs {eu} {ev}"
        );
    }

    #[test]
    fn rejects_degenerate_input() {
        // Collinear points: no homography exists.
        let pairs: Vec<(f64, f64, f64, f64)> = (0..5)
            .map(|i| (i as f64, 0.0, i as f64 * 2.0, 0.0))
            .collect();
        assert!(fit_homography(&pairs).is_none());
    }
}

/// One bit per module, row-major — a symbol's module grid sampled out of
/// an image.
#[derive(Debug, Clone, PartialEq)]
pub struct BitGrid {
    pub cols: u32,
    pub rows: u32,
    pub bits: Vec<bool>,
}

impl BitGrid {
    #[inline]
    pub fn ink(&self, x: u32, y: u32) -> bool {
        x < self.cols && y < self.rows && self.bits[(y * self.cols + x) as usize]
    }
}

/// Homography from a symbol quad to a `cols` x `rows` module plane.
///
/// Every 2D detector reduces to this: it knows the symbol's four image
/// corners (`Quad`, TL/TR/BR/BL in symbol space) and its module
/// dimensions; the projective map then straightens perspective, rotation
/// and shear at once. The module plane spans `0..cols` / `0..rows`.
pub fn quad_homography(quad: &crate::Quad, cols: f64, rows: f64) -> Option<[f64; 9]> {
    let src = [(0.0, 0.0), (cols, 0.0), (cols, rows), (0.0, rows)];
    let mut pairs = Vec::with_capacity(4);
    for (s, p) in src.iter().zip(quad.iter()) {
        pairs.push((s.0, s.1, p.x as f64, p.y as f64));
    }
    fit_homography(&pairs)
}

/// Sample a `cols` x `rows` module grid through `h` (module space →
/// image pixels). Each module is a **3x3 majority** around its centre:
/// at a ~3px module a 1px edge error would otherwise flip the module,
/// and a real print/camera edge is never exact. The caller's homography
/// carries the geometric correction; this is the shared "straighten and
/// read" step of every 2D family.
pub fn sample_grid(img: &crate::BitImage, h: &[f64; 9], cols: u32, rows: u32) -> BitGrid {
    let mut bits = Vec::with_capacity((cols * rows) as usize);
    for v in 0..rows {
        for u in 0..cols {
            let (x, y) = apply_h(h, u as f64 + 0.5, v as f64 + 0.5);
            let cx = x.round() as i64;
            let cy = y.round() as i64;
            let mut ink = 0;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0
                        && py >= 0
                        && px < img.width as i64
                        && py < img.height as i64
                        && img.ink(px as u32, py as u32)
                    {
                        ink += 1;
                    }
                }
            }
            bits.push(ink >= 5);
        }
    }
    BitGrid { cols, rows, bits }
}

#[cfg(test)]
mod grid_tests {
    use super::*;
    use crate::{BitImage, Point};

    /// A 7x3-module checkerboard painted through a known perspective map
    /// samples back to the same bits through `quad_homography`.
    #[test]
    fn quad_homography_round_trips_a_perspective_grid() {
        let (cols, rows, _m) = (7u32, 3u32, 10u32);
        // A sheared + scaled destination quad (as a phone would see it).
        let quad = [
            Point { x: 20.0, y: 15.0 },
            Point { x: 95.0, y: 22.0 },
            Point { x: 88.0, y: 60.0 },
            Point { x: 12.0, y: 55.0 },
        ];
        let h = quad_homography(&quad, cols as f64, rows as f64).expect("H");
        // paint: module (u,v) ink when (u+v) even
        let mut canvas = vec![0u8; 128 * 128];
        let width = 128u32;
        let bpr = (width.div_ceil(8).div_ceil(4) * 4) as usize;
        for v in 0..rows {
            for u in 0..cols {
                if (u + v) % 2 == 0 {
                    let (cx, cy) = apply_h(&h, u as f64 + 0.5, v as f64 + 0.5);
                    // fill the module's 10x10 cell around the centre
                    for dy in -4i32..=4 {
                        for dx in -4i32..=4 {
                            let x = (cx.round() as i32 + dx) as u32;
                            let y = (cy.round() as i32 + dy) as u32;
                            canvas[y as usize * bpr + (x >> 3) as usize] |= 1 << (x & 7);
                        }
                    }
                }
            }
        }
        let img = BitImage::new(width, 128, &canvas);
        let grid = sample_grid(&img, &h, cols, rows);
        for v in 0..rows {
            for u in 0..cols {
                assert_eq!(grid.ink(u, v), (u + v) % 2 == 0, "module {u},{v}");
            }
        }
    }
}

/// Sample a `cols` x `rows` module grid by **bilinear interpolation inside
/// the symbol quad** (TL,TR,BR,BL). Unlike [`quad_homography`], this makes
/// each row's module pitch exactly `row_width / cols`, which is what a
/// printed grid actually is: a homography fitted to four corner estimates
/// amplifies their ±2px noise into a strong fake perspective (measured: a
/// 2% edge taper yielded a 12% pitch variation and shifted an 8-module
/// start bar to 7). Use this when the symbol is a printed module grid;
/// keep the homography for genuine camera perspective of a planar grid.
pub fn sample_quad_grid(
    img: &crate::BitImage,
    quad: &crate::Quad,
    cols: u32,
    rows: u32,
) -> BitGrid {
    let [tl, tr, br, bl] = *quad;
    let mut bits = Vec::with_capacity((cols * rows) as usize);
    for v in 0..rows {
        let tv = (v as f32 + 0.5) / rows as f32;
        // Row endpoints: top edge (TL→TR) and bottom edge (BL→BR), then
        // the row line between them.
        let left_x = tl.x + (bl.x - tl.x) * tv;
        let left_y = tl.y + (bl.y - tl.y) * tv;
        let right_x = tr.x + (br.x - tr.x) * tv;
        let right_y = tr.y + (br.y - tr.y) * tv;
        for u in 0..cols {
            let tu = (u as f32 + 0.5) / cols as f32;
            let x = left_x + (right_x - left_x) * tu;
            let y = left_y + (right_y - left_y) * tu;
            let cx = x.round() as i64;
            let cy = y.round() as i64;
            let mut ink = 0;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0
                        && py >= 0
                        && px < img.width as i64
                        && py < img.height as i64
                        && img.ink(px as u32, py as u32)
                    {
                        ink += 1;
                    }
                }
            }
            bits.push(ink >= 5);
        }
    }
    BitGrid { cols, rows, bits }
}
