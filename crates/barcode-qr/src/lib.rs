//! Clean-room QR Code decoder (ISO/IEC 18004). Milestone 1: finder-pattern
//! detection.
//!
//! A QR symbol carries three identical 7x7-module finder patterns (dark
//! ring, 1-module light ring, 3x3 dark center) in three corners. Any
//! scanline through a finder shows a 1:1:3:1:1 ink/space rhythm. We scan
//! every row for that rhythm, cluster the candidates, and confirm each
//! cluster vertically — the result is the three finder centers plus the
//! module size, from which the sampling grid follows.

use barcode_core::{BitImage, DecodeResult, Point, Symbology, SymbologyDecoder};

pub mod codewords;
pub mod grid;
pub mod rs;
pub mod segments;

/// A detected finder pattern: center in pixels, the module size, and the
/// cluster strength (row candidates backing it — used to rank finders).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Finder {
    pub x: f32,
    pub y: f32,
    pub module: f32,
    pub strong: usize,
}

#[derive(Debug, Default)]
pub struct QrDecoder {
    /// Candidates from the current row: (x_center, y, module).
    row_cands: Vec<(f32, f32, f32)>,
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    /// Presence signal from the last decode(): 0 = no verified finders,
    /// 1 = finders found but the symbol did not decode (present but
    /// marginal — the auto-framing policy's torch trigger), 2 = decoded.
    last_presence: u8,
}

impl QrDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Presence signal from the last decode() attempt (see `last_presence`).
    /// Only meaningful when the QR tier actually ran on the frame.
    pub fn presence(&self) -> u8 {
        self.last_presence
    }

    /// Detect the three finder patterns. Returns the three strongest
    /// clusters UNLABELED, or None.
    ///
    /// Two passes: horizontal scanlines, then vertical. A tilted finder
    /// stretches runs by 1/cos(tilt) — past ~40° the ratio check starves
    /// on one axis, and the other axis (whose stretch is 1/sin) still
    /// feeds the detector. Between the two passes every orientation from
    /// 0° to 90° is covered.
    pub fn find_finders(&mut self, image: &BitImage) -> Option<[Finder; 3]> {
        let mut cands: Vec<(f32, f32, f32)> = Vec::new();
        for y in 0..image.height {
            self.row_cands.clear();
            self.scan_line_candidates(image, y, true);
            for (x, _, m) in &self.row_cands {
                cands.push((*x, y as f32, *m));
            }
        }
        if std::env::var("QR_DBG").is_ok() {
            eprintln!("DBG row candidates {}", cands.len());
        }
        let row_result = Self::cluster_and_confirm(cands, image, true);
        if std::env::var("QR_DBG").is_ok() {
            eprintln!("DBG row pass: {:?}", row_result.as_ref().map(|f| f.iter().map(|q| (q.x, q.y, q.module)).collect::<Vec<_>>()));
        }
        if let found @ Some(_) = row_result {
            return found;
        }
        let mut cands: Vec<(f32, f32, f32)> = Vec::new();
        for x in 0..image.width {
            self.row_cands.clear();
            self.scan_line_candidates(image, x, false);
            for (y, _, m) in &self.row_cands {
                cands.push((x as f32, (*y) as f32, *m));
            }
        }
        let col_result = Self::cluster_and_confirm(cands, image, false);
        col_result
    }

    /// One scanline (row if `horizontal`, else column): every 5-run window
    /// ink-space-ink-space-ink whose widths fit 1:1:3:1:1 at some module
    /// size yields a candidate centered on the middle (3-wide) run.
    fn scan_line_candidates(&mut self, image: &BitImage, line: u32, horizontal: bool) {
        let len = if horizontal { image.width } else { image.height };
        let ink_at = |i: u32| {
            if horizontal {
                image.ink(i, line)
            } else {
                image.ink(line, i)
            }
        };
        self.runs.clear();
        self.offsets.clear();
        let mut cur = false;
        let mut n = 0u32;
        let mut run_start = 0u32;
        let mut i = 0u32;
        while i < len {
            let ink = ink_at(i);
            if ink == cur {
                n += 1;
            } else {
                self.runs.push((cur, std::mem::replace(&mut n, 1)));
                self.offsets.push(run_start);
                run_start = i;
                cur = ink;
            }
            i += 1;
        }
        if n > 0 {
            self.runs.push((cur, n));
            self.offsets.push(run_start);
        }

        self.row_cands.clear();
        if self.runs.len() < 5 {
            return;
        }
        for k in 0..=self.runs.len() - 5 {
            // The window must be ink, space, ink, space, ink.
            if !self.runs[k].0 || self.runs[k + 1].0 || !self.runs[k + 2].0 || self.runs[k + 3].0
                || !self.runs[k + 4].0
            {
                continue;
            }
            let w: [u32; 5] = [
                self.runs[k].1,
                self.runs[k + 1].1,
                self.runs[k + 2].1,
                self.runs[k + 3].1,
                self.runs[k + 4].1,
            ];
            let total: u32 = w.iter().sum();
            let m = total as f32 / 7.0;
            if m < 1.5 {
                continue;
            }
            // 1:1:3:1:1 within tolerance (blur shifts edges by ~1px).
            // Ratios are tilt-invariant (a tilted window stretches all
            // runs uniformly), so the tolerance stays tight; blur shifts
            // edges by ~1px.
            let ok = (1..=4).all(|i| {
                let target = if i == 2 { 3.0 } else { 1.0 };
                let lo = target * m * 0.6;
                let hi = if i == 2 { m * 3.5 } else { m * 1.45 };
                (w[i] as f32) >= lo && (w[i] as f32) <= hi
            });
            if !ok {
                continue;
            }
            // Center of the middle run, along the scanline.
            let c = self.offsets[k + 2] as f32 + w[2] as f32 / 2.0;
            self.row_cands.push((c, line as f32, m));
        }
    }

    /// Structural check: a true finder shows dark center, light ring and
    /// dark outer ring. Probes are sampled mid-zone and voted: real prints
    /// grow ink 1-3px past module edges (camera blur + threshold), so
    /// single-pixel probes at fixed radii grazing a boundary reject honest
    /// finders (2026-10-23 receipt field bug — the TL finder verified but
    /// the TR/BL failed on grown edges). Zones in modules from the center:
    /// dark ≤1.5, light 1.5-2.5, dark 2.5-3.5; samples sit at 0.5/2.0/3.0
    /// with ≥1.5m clearance from every boundary, and each zone tolerates a
    /// couple of stray pixels.
    ///
    /// `(dx, dy)` is the finder's own axis direction, fitted from the
    /// candidate cluster — probing along image x/y would land in the light
    /// ring for tilted finders and reject honest hits.
    fn verify_finder(img: &BitImage, f: &Finder, dx: f32, dy: f32) -> bool {
        // Under perspective the two in-plane scales differ (the v7 stress
        // fixture: 17px across vs ~25px vertically at the top-right
        // finder), so probing a circle at the run-derived module rejects
        // honest finders. `m_axis` is the cluster's span/3 — the scale
        // across the scan — and `m` the run-derived scale along it.
        Self::verify_finder_scales(img, f, dx, dy, f.module, f.module)
    }

    /// `verify_finder` with independent module scales along the cluster
    /// axis (`m_axis`) and across it (`m_perp`).
    fn verify_finder_scales(
        img: &BitImage,
        f: &Finder,
        dx: f32,
        dy: f32,
        m_axis: f32,
        m_perp: f32,
    ) -> bool {
        let probe = |a: f32, b: f32| -> bool {
            // (a, b) in finder-frame modules; image offset a*axis + b*perp.
            let px = (f.x + a * m_axis * dx - b * m_perp * dy).round() as i64;
            let py = (f.y + a * m_axis * dy + b * m_perp * dx).round() as i64;
            if px < 0 || py < 0 || px >= img.width as i64 || py >= img.height as i64 {
                return false;
            }
            img.ink(px as u32, py as u32)
        };
        // Dark 3x3 center: 9 mid-zone samples, tolerate 1 stray.
        let mut center_ink = 0;
        for da in [-0.5f32, 0.0, 0.5] {
            for db in [-0.5f32, 0.0, 0.5] {
                if probe(da, db) {
                    center_ink += 1;
                }
            }
        }
        // Ring frames: 4 sides x 5 mid-line samples. Radii 1.9/2.8 stay
        // inside their zones both when m is exact and when the cluster's m
        // is inflated ~15% by tilt (runs stretch 1/cos(tilt) along the
        // scanlines — a 30° tilt yields m 9.24 for a true 8).
        let mut light_ink = 0;
        let mut outer_ink = 0;
        for s in [-2.0f32, -1.0, 0.0, 1.0, 2.0] {
            for (a, b) in [(s, 1.9), (s, -1.9), (1.9, s), (-1.9, s)] {
                if probe(a, b) {
                    light_ink += 1;
                }
            }
            for (a, b) in [(s, 2.8), (s, -2.8), (2.8, s), (-2.8, s)] {
                if probe(a, b) {
                    outer_ink += 1;
                }
            }
        }
        center_ink >= 8 && light_ink <= 3 && outer_ink >= 17
    }

    /// Centroid / axis / structural-verify one candidate set.
    fn confirm_set(img: &BitImage, pts: &[(f32, f32, f32)]) -> Option<Finder> {
        let n = pts.len() as f32;
        let xs: Vec<f32> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f32> = pts.iter().map(|p| p.1).collect();
        let m = pts.iter().map(|p| p.2).sum::<f32>() / n;
        // Tilt-invariant shape check: candidates must line up on a
        // straight axis (small perpendicular residual) spanning at
        // least the ~3m dark center.
        let (dx, dy, mid_offset, span, resid) = Self::principal_axis(&xs, &ys);
        if span < 1.0 * m || resid > 0.8 * m {
            return None;
        }
        let cx = xs.iter().sum::<f32>() / n;
        let cy = ys.iter().sum::<f32>() / n;
        let f = Finder {
            x: cx + mid_offset * dx,
            y: cy + mid_offset * dy,
            module: m,
            strong: pts.len(),
        };
        // The candidate cloud spans the finder's 3-module dark centre
        // along the cluster axis; that is the local scale across the scan.
        let m_axis = (span / 3.0).clamp(0.5 * m, 2.0 * m);
        let ok = Self::verify_finder_scales(img, &f, dx, dy, m_axis, m);
        if std::env::var("QR_DBG").is_ok() {
            eprintln!("DBG   set n={} c=({:.0},{:.0}) m={m:.2} verify={ok}", pts.len(), f.x, f.y);
        }
        if ok {
            Some(f)
        } else {
            None
        }
    }

    /// Confirm one candidate cluster as a finder. When the direct centroid
    /// fails — typically because dense data rhythms CHAINED into the
    /// cluster (their local module size matches the finder's under
    /// perspective) — recover the finder's core: seed at the densest
    /// candidate, mean-shift a finder-sized kernel onto the candidate
    /// strip, then scan verify ALONG the streak — the strip fixes the
    /// finder's cross position and module size, and only the true center
    /// passes the structural probes.
    fn confirm_cluster(img: &BitImage, pts: &[(f32, f32, f32)], dbg: bool) -> Option<Finder> {
        if let Some(f) = Self::confirm_set(img, pts) {
            if dbg {
                eprintln!("DBG   verify=true (direct)");
            }
            return Some(f);
        }
        let n = pts.len();
        if n < 5 {
            return None;
        }
        // Streak direction from the whole cloud (stable even when polluted).
        let (sdx, sdy, _, _, _) = Self::principal_axis(
            &pts.iter().map(|p| p.0).collect::<Vec<_>>(),
            &pts.iter().map(|p| p.1).collect::<Vec<_>>(),
        );
        // Seeds: the densest candidates (most neighbours within 1.2m),
        // deduplicated within 2m, best three.
        let mut scored: Vec<(usize, usize)> = (0..n)
            .map(|i| {
                let r = 1.2 * pts[i].2;
                let c = (0..n)
                    .filter(|&j| {
                        let ddx = pts[j].0 - pts[i].0;
                        let ddy = pts[j].1 - pts[i].1;
                        ddx * ddx + ddy * ddy < r * r
                    })
                    .count();
                (i, c)
            })
            .filter(|&(_, c)| c >= 4)
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        let mut seeds: Vec<usize> = Vec::new();
        for &(i, _) in &scored {
            if seeds.iter().all(|&s| {
                let ddx = pts[s].0 - pts[i].0;
                let ddy = pts[s].1 - pts[i].1;
                ddx * ddx + ddy * ddy > (2.0 * pts[i].2).powi(2)
            }) {
                seeds.push(i);
            }
            if seeds.len() == 3 {
                break;
            }
        }
        if dbg {
            eprintln!(
                "DBG   seeds n={n}: {:?}",
                seeds.iter().map(|&s| (pts[s].0 as i32, pts[s].1 as i32, scored.iter().find(|&&(i, _)| i == s).map(|&(_, c)| c).unwrap_or(0))).collect::<Vec<_>>()
            );
        }
        for &seed in &seeds {
            // Mean-shift a 1.2m kernel onto the strip.
            let (mut kx, mut ky) = (pts[seed].0, pts[seed].1);
            let mut kept: Vec<(f32, f32, f32)> = Vec::new();
            for _ in 0..4 {
                kept = pts
                    .iter()
                    .filter(|p| {
                        let ddx = p.0 - kx;
                        let ddy = p.1 - ky;
                        let r = 1.2 * p.2;
                        ddx * ddx + ddy * ddy < r * r
                    })
                    .copied()
                    .collect();
                if kept.len() < 4 {
                    break;
                }
                let nx = kept.iter().map(|p| p.0).sum::<f32>() / kept.len() as f32;
                let ny = kept.iter().map(|p| p.1).sum::<f32>() / kept.len() as f32;
                let moved = (nx - kx).hypot(ny - ky) > 0.05 * pts[seed].2;
                kx = nx;
                ky = ny;
                if !moved {
                    break;
                }
            }
            if kept.len() < 4 {
                if dbg {
                    eprintln!("DBG   seed ({:.0},{:.0}): kernel kept={} too small", pts[seed].0, pts[seed].1, kept.len());
                }
                continue;
            }
            let m = kept.iter().map(|p| p.2).sum::<f32>() / kept.len() as f32;
            // 1-D verify scan along the streak through the kernel center.
            let step = 0.25 * m;
            let mut best_run: Option<(f32, f32)> = None; // (run start, run end)
            let mut run_start: Option<f32> = None;
            let mut t = -3.5 * m;
            while t <= 3.5 * m {
                let f = Finder {
                    x: kx + t * sdx,
                    y: ky + t * sdy,
                    module: m,
                    strong: kept.len(),
                };
                let ok = Self::verify_finder(img, &f, sdx, sdy);
                if std::env::var("QR_DBG").is_ok() && kept.len() > 20 {
                    eprintln!("DBG     t={t:.2} ({:.0},{:.0}) m={m:.2} verify={ok}", f.x, f.y);
                }
                if ok {
                    if run_start.is_none() {
                        run_start = Some(t);
                    }
                } else if let Some(rs) = run_start.take() {
                    let len = t - step - rs;
                    if best_run.map_or(true, |(s, e)| len > e - s) {
                        best_run = Some((rs, t - step));
                    }
                }
                t += step;
            }
            if let Some(rs) = run_start {
                let len = t - step - rs;
                if best_run.map_or(true, |(s, e)| len > e - s) {
                    best_run = Some((rs, t - step));
                }
            }
            let Some((rs, re)) = best_run else { continue };
            let mid = (rs + re) / 2.0;
            let f = Finder {
                x: kx + mid * sdx,
                y: ky + mid * sdy,
                module: m,
                strong: kept.len(),
            };
            if dbg {
                eprintln!(
                    "DBG   streak scan: kept={} m={m:.2} run=({rs:.2},{re:.2}) mid={mid:.2} at ({:.0},{:.0})",
                    kept.len(), f.x, f.y
                );
            }
            return Some(f);
        }
        None
    }

    /// Principal axis of the cluster's candidate cloud: returns the unit
    /// direction, the MIDPOINT offset along it (relative to the centroid),
    /// the span, and the RMS perpendicular residual. A true finder's
    /// candidates line up on its axis (small residual) and span the dark
    /// center's ~3 modules; data-rhythm noise does not. The midpoint (not
    /// the mean) centers the finder — per-scanline sampling skews the
    /// candidate distribution, and a biased center shifts every downstream
    /// sample by a fraction of a module.
    fn principal_axis(xs: &[f32], ys: &[f32]) -> (f32, f32, f32, f32, f32) {
        let n = xs.len() as f32;
        let mx = xs.iter().sum::<f32>() / n;
        let my = ys.iter().sum::<f32>() / n;
        let (mut sxx, mut syy, mut sxy) = (0.0f32, 0.0f32, 0.0f32);
        for (&x, &y) in xs.iter().zip(ys.iter()) {
            let (ex, ey) = (x - mx, y - my);
            sxx += ex * ex;
            syy += ey * ey;
            sxy += ex * ey;
        }
        let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        let (dx, dy) = (theta.cos(), theta.sin());
        let (mut lo, mut hi, mut sum2, mut along_sum) =
            (f32::MAX, f32::MIN, 0.0f32, 0.0f32);
        for (&x, &y) in xs.iter().zip(ys.iter()) {
            let (ex, ey) = (x - mx, y - my);
            let along = ex * dx + ey * dy;
            let perp = -ex * dy + ey * dx;
            lo = lo.min(along);
            hi = hi.max(along);
            along_sum += along;
            sum2 += perp * perp;
        }
        let mid_offset = (lo + hi) / 2.0 - along_sum / n;
        (dx, dy, mid_offset, hi - lo, (sum2 / n).sqrt())
    }

    /// Cluster row candidates into finders, keep those confirmed vertically
    /// AND structurally verified against the image (dense data areas spawn
    /// coincidental 1:1:3:1:1 rhythms that vertical confirmation alone
    /// accepts), then require three of them.
    fn cluster_and_confirm(
        cands: Vec<(f32, f32, f32)>,
        image: &BitImage,
        horizontal: bool,
    ) -> Option<[Finder; 3]> {
        // Union-find over locally-consistent neighbors: two candidates are
        // the same pattern if they sit within 1.5 modules of each other
        // (measured against the SMALLER candidate's own module size, so a
        // chain cannot leap between patterns using an inflated drifting
        // mean) and agree on module size. A finder's scanline candidates
        // are mutually close; alignment patterns and data rhythms form
        // their own clusters and are rejected later on structure.
        let n = cands.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn root(parent: &mut Vec<usize>, mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        // Chaining gates: the ALONG-SCAN delta is tight — a finder's own
        // candidates drift well under a module per scanline, while data
        // rhythms on the same scanline sit modules away (under perspective
        // their module size matches the finder's, so the m-ratio gate alone
        // cannot stop them attaching and dragging the centroid off). The
        // cross-scan delta stays loose: the finder's candidate streak spans
        // its whole height across many scanlines.
        let (along_mul, cross_mul) = if horizontal { (0.5, 2.2) } else { (2.2, 0.5) };
        for i in 0..n {
            let (x1, y1, m1) = cands[i];
            for j in (i + 1)..n {
                let (x2, y2, m2) = cands[j];
                let mm = m1.min(m2);
                if (x1 - x2).abs() < mm * along_mul
                    && (y1 - y2).abs() < mm * cross_mul
                    && (m1 / m2 - 1.0).abs() < 0.25
                {
                    let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                    if ri != rj {
                        parent[ri] = rj;
                    }
                }
            }
        }
        let mut groups: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..n {
            groups.entry(root(&mut parent, i)).or_default().push(i);
        }

        let dbg = std::env::var("QR_DBG").is_ok();
        let mut finders: Vec<Finder> = Vec::new();
        for idx in groups.values() {
            if idx.len() < 3 {
                continue;
            }
            let pts: Vec<(f32, f32, f32)> = idx.iter().map(|&i| cands[i]).collect();
            if dbg {
                let xs: Vec<f32> = pts.iter().map(|p| p.0).collect();
                let ys: Vec<f32> = pts.iter().map(|p| p.1).collect();
                let m: f32 = pts.iter().map(|p| p.2).sum::<f32>() / pts.len() as f32;
                let (_, _, _, span, resid) = Self::principal_axis(&xs, &ys);
                eprintln!(
                    "DBG cluster n={} m={m:.2} c=({:.0},{:.0}) span={span:.1} resid={resid:.2}",
                    pts.len(),
                    xs.iter().sum::<f32>() / xs.len() as f32,
                    ys.iter().sum::<f32>() / ys.len() as f32,
                );
            }
            if let Some(f) = Self::confirm_cluster(image, &pts, dbg) {
                finders.push(f);
            }
        }
        if finders.len() < 3 {
            return None;
        }

        // Verified finders can include data-rhythm look-alikes that got
        // lucky with the structural probes. A real symbol's three finders
        // agree on module size *roughly*: under perspective the local
        // scale varies across the symbol (the v2 stress fixture: 16px at
        // TR vs 27px at BL from the column pass, +71%), so the tolerance
        // must be wide. The geometry and format/BCH checks downstream are
        // the real arbiters — a look-alike that joins the triple makes
        // every labeling fail, it cannot produce a false decode.
        finders.sort_by(|a, b| b.strong.cmp(&a.strong));
        let mut chosen: Option<Vec<Finder>> = None;
        for seed in &finders {
            let group: Vec<Finder> = finders
                .iter()
                .filter(|f| (f.module / seed.module - 1.0).abs() < 0.85)
                .copied()
                .collect();
            if group.len() >= 3 {
                chosen = Some(group);
                break;
            }
        }
        let mut finders = match chosen {
            Some(f) => f,
            None => return None,
        };

        // A QR code is readable at ANY rotation — the three finders just
        // land in different image corners. Return the three strongest
        // clusters UNLABELED; the caller enumerates the six (tl, tr, bl)
        // labelings and lets the format-info BCH check pick the right one.
        finders.truncate(3);
        let [f0, f1, f2] = [finders[0], finders[1], finders[2]];
        Some([f0, f1, f2])
    }
}

/// All six (tl, tr, bl) labelings of a finder triple. Exactly one matches
/// the symbol's true orientation; the mirrored ones produce grids whose
/// format read fails the BCH check, so the enumeration is self-selecting.
pub const LABELINGS: [[usize; 3]; 6] =
    [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];

/// Try every labeling of the finder triangle and return the first that
/// decodes END TO END (format → codewords → segments). A format-only gate
/// is not enough: perspective drift can push the true orientation's
/// format word into correction territory while a mirrored labeling's
/// garbage happens to validate — the Reed-Solomon pass is the judge.
pub(crate) fn locate_symbol(
    img: &BitImage,
    finders: &[Finder; 3],
) -> Option<(grid::Grid, [Finder; 3], grid::EcLevel, u8, Vec<u8>)> {
    for perm in LABELINGS {
        let [tl, tr, bl] =
            [finders[perm[0]], finders[perm[1]], finders[perm[2]]];
        let dbg = std::env::var("QR_DBG").is_ok();
        let Some(grid) = grid::Grid::from_finders(&tl, &tr, &bl) else {
            if dbg {
                eprintln!("locate: finders -> no grid for perm {perm:?}");
            }
            continue;
        };
        let Some((ec, mask)) = grid::read_format(img, &grid) else {
            if dbg {
                eprintln!("locate: grid size {} -> format failed", grid.size);
            }
            continue;
        };
        let Some(codewords) = codewords::read_codewords(img, &grid, ec, mask) else {
            if dbg {
                eprintln!("locate: ec {ec:?} mask {mask} -> codewords failed");
            }
            continue;
        };
        if dbg {
            eprintln!(
                "locate: size {} ec {ec:?} mask {mask} codewords {}",
                grid.size,
                codewords.len()
            );
        }
        return Some((grid, [tl, tr, bl], ec, mask, codewords));
    }
    // Phase 2: perspective. The finders exist but no affine labeling
    // decodes — under strong perspective the affine map cannot bend far
    // enough (its error grows away from the finder triangle until modules
    // sample wrong). Measure each finder's outer-ring edge midpoints —
    // their module-space coordinates are exact regardless of perspective —
    // fit the homography through all 15 correspondences per labeling, and
    // retry the full chain sampling through it. Candidate sizes come from
    // the mean module estimate ±1 snap, since foreshortening skews the
    // span-based estimate.
    locate_symbol_perspective(img, finders)
}

/// The outer edge of a finder's dark ring along one direction, in pixels.
/// The ring is a ~1-module ink run whose outer edge sits at ~3.5 modules;
/// the separator beyond it is structurally light. `m` is the cluster's
/// module estimate, which under rotation/perspective is inflated along
/// the scan axis and can put the nominal 3.0-module probe in the light
/// gap — so locate the ring as a run rather than assuming a start point.
fn ring_edge(
    img: &BitImage,
    cx: f64,
    cy: f64,
    dx: f64,
    dy: f64,
    m: f64,
) -> Option<f64> {
    let ink_at = |r: f64| -> bool {
        let px = (cx + r * dx).round() as i64;
        let py = (cy + r * dy).round() as i64;
        if px < 0 || py < 0 || px >= img.width as i64 || py >= img.height as i64 {
            return false;
        }
        img.ink(px as u32, py as u32)
    };
    // Ink runs from 1.0 to 4.8 modules along the probe.
    let step = 0.05 * m;
    let mut runs: Vec<(f64, f64)> = Vec::new();
    let mut start: Option<f64> = None;
    let mut r = 1.0 * m;
    while r <= 4.8 * m {
        if ink_at(r) {
            start.get_or_insert(r);
        } else if let Some(s) = start.take() {
            runs.push((s, r - step));
        }
        r += step;
    }
    if let Some(s) = start {
        runs.push((s, 4.8 * m));
    }
    // The dark center square (ends <= ~2.3m when the center is biased) is
    // rejected by the `end >= 2.6m` gate; the ring is the run nearest to
    // 3.5 modules.
    runs.iter()
        .filter(|&&(s, e)| e - s >= 0.4 * m && e >= 2.6 * m && e <= 4.3 * m)
        .min_by_key(|&&(s, e)| (((s + e) / 2.0 - 3.5 * m) / m).abs() as i32)
        .map(|&(_, e)| e + 0.05 * m / 2.0)
}

/// Measure one finder's four ring-edge midpoints and emit (module-space u,
/// v, image x, y) correspondences. `role`: 0 = tl, 1 = tr, 2 = bl; the
/// module-space coordinates follow from the role and the symbol size.
fn measure_finder_points(
    img: &BitImage,
    f: &Finder,
    ax: (f64, f64),
    ay: (f64, f64),
    role: u8,
    n: u32,
) -> Option<Vec<(f64, f64, f64, f64)>> {
    let (cx, cy) = (f.x as f64, f.y as f64);
    let m = f.module as f64;
    let nf = n as f64;
    // Edge midpoint src coords per role: (west, east, north, south).
    let (wu, wv, eu, ev, nu_, nv_, su, sv) = match role {
        0 => (0.0, 3.5, 7.0, 3.5, 3.5, 0.0, 3.5, 7.0),
        1 => (nf - 7.0, 3.5, nf, 3.5, nf - 3.5, 0.0, nf - 3.5, 7.0),
        _ => (0.0, nf - 3.5, 7.0, nf - 3.5, 3.5, nf - 7.0, 3.5, nf),
    };
    let mut pairs = Vec::with_capacity(5);
    // The finder's own center is an exact correspondence too.
    let (cu, cv) = match role {
        0 => (3.5, 3.5),
        1 => (nf - 3.5, 3.5),
        _ => (3.5, nf - 3.5),
    };
    pairs.push((cu, cv, cx, cy));
    for (dir, (u, v)) in [
        ((-ax.0, -ax.1), (wu, wv)),
        ((ax.0, ax.1), (eu, ev)),
        ((-ay.0, -ay.1), (nu_, nv_)),
        ((ay.0, ay.1), (su, sv)),
    ] {
        let r = ring_edge(img, cx, cy, dir.0, dir.1, m)?;
        pairs.push((u, v, cx + r * dir.0, cy + r * dir.1));
    }
    Some(pairs)
}

/// Alignment pattern center coordinates per version (module index), used
/// as BOTH row and column; the three finder-corner combinations are
/// skipped (ISO/IEC 18004 annex E). Version 1 has none. Public spec
/// data, same table the fixture renderer uses.
#[rustfmt::skip]
const ALIGNMENT_COORDS: [&[u32]; 41] = [
  &[],
  &[],
  &[6, 18],
  &[6, 22],
  &[6, 26],
  &[6, 30],
  &[6, 34],
  &[6, 22, 38],
  &[6, 24, 42],
  &[6, 26, 46],
  &[6, 28, 50],
  &[6, 30, 54],
  &[6, 32, 58],
  &[6, 34, 62],
  &[6, 26, 46, 66],
  &[6, 26, 48, 70],
  &[6, 26, 50, 74],
  &[6, 30, 54, 78],
  &[6, 30, 56, 82],
  &[6, 30, 58, 86],
  &[6, 34, 62, 90],
  &[6, 28, 50, 72, 94],
  &[6, 26, 50, 74, 98],
  &[6, 30, 54, 78, 102],
  &[6, 28, 54, 80, 106],
  &[6, 32, 58, 84, 110],
  &[6, 30, 58, 86, 114],
  &[6, 34, 62, 90, 118],
  &[6, 26, 50, 74, 98, 122],
  &[6, 30, 54, 78, 102, 126],
  &[6, 26, 52, 78, 104, 130],
  &[6, 30, 56, 82, 108, 134],
  &[6, 34, 60, 86, 112, 138],
  &[6, 30, 58, 86, 114, 142],
  &[6, 34, 62, 90, 118, 146],
  &[6, 30, 54, 78, 102, 126, 150],
  &[6, 24, 50, 76, 102, 128, 154],
  &[6, 28, 54, 80, 106, 132, 158],
  &[6, 32, 58, 84, 110, 136, 162],
  &[6, 26, 54, 82, 110, 138, 166],
  &[6, 30, 58, 86, 114, 142, 170],
];

/// The geometric centres of a symbol's alignment patterns, in module
/// coordinates (module index i spans [i, i+1), so the pattern centred on
/// module index r has its geometric centre at r + 0.5).
fn alignment_centers(size: u32) -> Vec<(f64, f64)> {
    let version = size.saturating_sub(17) / 4;
    let coords = ALIGNMENT_COORDS
        .get(version as usize)
        .copied()
        .unwrap_or(&[]);
    if coords.is_empty() {
        return Vec::new();
    }
    let (first, last) = (coords[0], coords[coords.len() - 1]);
    let mut out = Vec::with_capacity(coords.len() * coords.len());
    for &r in coords {
        for &c in coords {
            if (r, c) == (first, first) || (r, c) == (first, last) || (r, c) == (last, first) {
                continue;
            }
            out.push((c as f64 + 0.5, r as f64 + 0.5));
        }
    }
    out
}

/// Locate an alignment pattern's centre near the homography's prediction.
/// Search ±2.25 modules in 0.25-module steps (the finder-only fit's
/// residual is largest at the far corner) and score the 5x5 bullseye
/// (dark outer ring, light inner ring, dark centre) through the local
/// module basis. Returns the image point when the score is unambiguous.
fn alignment_at(img: &BitImage, h: &[f64; 9], u: f64, v: f64) -> Option<(f64, f64)> {
    let p0 = barcode_core::geometry::apply_h(h, u, v);
    let pu = {
        let p = barcode_core::geometry::apply_h(h, u + 1.0, v);
        (p.0 - p0.0, p.1 - p0.1)
    };
    let pv = {
        let p = barcode_core::geometry::apply_h(h, u, v + 1.0);
        (p.0 - p0.0, p.1 - p0.1)
    };
    let ink = |x: f64, y: f64| -> bool {
        let px = x.round() as i64;
        let py = y.round() as i64;
        px >= 0
            && py >= 0
            && px < img.width as i64
            && py < img.height as i64
            && img.ink(px as u32, py as u32)
    };
    // The score plateaus wherever the whole probe grid stays inside its
    // modules, so average every top-scoring position instead of taking the
    // first one (which would bias the centre to the search's start edge).
    let mut best = 0u32;
    let mut sum = (0.0f64, 0.0f64, 0u32);
    for su in -12i32..=12 {
        for sv in -12i32..=12 {
            let (du, dv) = (su as f64 * 0.25, sv as f64 * 0.25);
            let px = p0.0 + du * pu.0 + dv * pv.0;
            let py = p0.1 + du * pu.1 + dv * pv.1;
            let mut score = 0u32;
            for i in -2i32..=2 {
                for j in -2i32..=2 {
                    let want = i.abs().max(j.abs()) == 2 || (i == 0 && j == 0);
                    let p = (px + i as f64 * pu.0 + j as f64 * pv.0,
                             py + i as f64 * pu.1 + j as f64 * pv.1);
                    if ink(p.0, p.1) == want {
                        score += 1;
                    }
                }
            }
            if score > best {
                best = score;
                sum = (px, py, 1);
            } else if score == best && best > 0 {
                sum.0 += px;
                sum.1 += py;
                sum.2 += 1;
            }
        }
    }
    // 23/25: the bullseye is distinctive enough that data cannot fake it.
    if best >= 23 {
        Some((sum.0 / sum.2 as f64, sum.1 / sum.2 as f64))
    } else {
        None
    }
}

/// Fit the module → image homography for one labeling from the three
/// finders' measured geometry.
fn perspective_homography(
    img: &BitImage,
    labeled: &[Finder; 3],
    ax: (f64, f64),
    ay: (f64, f64),
    n: u32,
) -> Option<[f64; 9]> {
    let mut pairs = Vec::with_capacity(15);
    for (role, f) in labeled.iter().enumerate() {
        pairs.extend(measure_finder_points(img, f, ax, ay, role as u8, n)?);
    }
    let h0 = barcode_core::geometry::fit_homography(&pairs)?;
    // Alignment patterns anchor the symbol's far side, where a
    // finder-only homography carries the most residual. v2-v6 have one
    // (bottom-right), v7+ a grid of them. Each accepted bullseye is an
    // exact module-space correspondence, so re-fit with them included.
    let mut found = 0usize;
    for (u, v) in alignment_centers(n) {
        if let Some((x, y)) = alignment_at(img, &h0, u, v) {
            pairs.push((u, v, x, y));
            found += 1;
        }
    }
    if std::env::var("QR_DBG").is_ok() && found > 0 {
        eprintln!("perspective: {found}/{} alignment patterns anchored", alignment_centers(n).len());
    }
    barcode_core::geometry::fit_homography(&pairs)
}

fn locate_symbol_perspective(
    img: &BitImage,
    finders: &[Finder; 3],
) -> Option<(grid::Grid, [Finder; 3], grid::EcLevel, u8, Vec<u8>)> {
    for perm in LABELINGS {
        let [tl, tr, bl] =
            [finders[perm[0]], finders[perm[1]], finders[perm[2]]];
        // Loose geometry gate: perspective compresses one axis, so the
        // squareness tolerance is wider than the affine path's; the
        // format read remains the real judge.
        let dx = ((tr.x - tl.x) as f64, (tr.y - tl.y) as f64);
        let dy = ((bl.x - tl.x) as f64, (bl.y - tl.y) as f64);
        let lx = dx.0.hypot(dx.1);
        let ly = dy.0.hypot(dy.1);
        if lx < 8.0 || ly < 8.0 || (lx / ly - 1.0).abs() > 0.6 {
            continue;
        }
        let ax = (dx.0 / lx, dx.1 / lx);
        let ay = (dy.0 / ly, dy.1 / ly);
        let m = ((tl.module + tr.module + bl.module) / 3.0) as f64;
        if m <= 0.0 {
            continue;
        }
        let n_raw = 7.0 + lx.max(ly) / m;
        let base = 21 + 4 * (((n_raw - 21.0) / 4.0).round().max(0.0) as i32);
        for size in [base - 4, base, base + 4] {
            let n = size as u32;
            if !(21..=177).contains(&n) {
                continue;
            }
            let Some(h) = perspective_homography(img, &[tl, tr, bl], ax, ay, n) else {
                if std::env::var("QR_DBG").is_ok() { eprintln!("perspective: perm {perm:?} n {n} -> no homography"); }
                continue;
            };
            let grid = grid::Grid::from_homography(n, h);
            let Some((ec, mask)) = grid::read_format(img, &grid) else {
                if std::env::var("QR_DBG").is_ok() { eprintln!("perspective: perm {perm:?} n {n} -> format failed"); }
                continue;
            };
            let Some(codewords) = codewords::read_codewords(img, &grid, ec, mask) else {
                if std::env::var("QR_DBG").is_ok() { eprintln!("perspective: perm {perm:?} n {n} -> codewords failed"); }
                continue;
            };
            if std::env::var("QR_DBG").is_ok() { eprintln!("perspective: perm {perm:?} n {n} ec {ec:?} mask {mask} -> OK"); }
            return Some((grid, [tl, tr, bl], ec, mask, codewords));
        }
    }
    None
}

impl SymbologyDecoder for QrDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::QRCode
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        // Finders → labeled grid → format (EC level + mask) → codewords →
        // text. Any stage failing means "no decodable symbol here".
        let Some(finders) = self.find_finders(image) else {
            self.last_presence = 0;
            return Vec::new();
        };
        self.last_presence = 1;
        let Some((grid, _labeled, _ec, _mask, codewords)) = locate_symbol(image, &finders) else {
            return Vec::new();
        };
        if std::env::var("QR_DBG").is_ok() {
            eprintln!("data ({} bytes): {:02x?}", codewords.len(), codewords);
        }
        let version = (grid.size - 17) / 4;
        let Some(payload) = segments::parse_segments(version, &codewords) else {
            return Vec::new();
        };
        let text = segments::payload_text(&payload);
        self.last_presence = 2;

        // Symbol quad: the grid maps the module-space corners (exact under
        // perspective for projective grids, parallelogram-closed otherwise).
        let position = grid
            .symbol_quad()
            .map(|(x, y)| Point { x, y });
        vec![DecodeResult {
            symbology: Symbology::QRCode,
            text,
            raw: payload,
            position,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draw a finder pattern's 7x7 modules with the top-left module at
    /// (x0, y0): dark outer ring + dark 3x3 center, light between.
    fn draw_finder(canvas: &mut barcode_fixtures::Canvas, x0: u32, y0: u32, m: u32) {
        for dy in 0..7u32 {
            for dx in 0..7u32 {
                let ring = dx == 0 || dx == 6 || dy == 0 || dy == 6;
                let center = (2..=4).contains(&dx) && (2..=4).contains(&dy);
                if ring || center {
                    canvas.fill(x0 + dx * m, y0 + dy * m, m, m);
                }
            }
        }
    }

    #[test]
    fn detects_three_finders_on_synthetic_canvas() {
        // Three finders on a plain canvas, module 4px.
        let m = 4u32;
        let mut canvas = barcode_fixtures::Canvas::new(320, 320);
        // positions in px (top-left module corner)
        let tl = (20u32, 20u32);
        let tr = (200u32, 20u32);
        let bl = (20u32, 200u32);
        draw_finder(&mut canvas, tl.0, tl.1, m);
        draw_finder(&mut canvas, tr.0, tr.1, m);
        draw_finder(&mut canvas, bl.0, bl.1, m);

        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = QrDecoder::new();
        let finders = d.find_finders(&img).expect("finders not found");

        let center = |f: &Finder| (f.x, f.y);
        // expected centers: corner + 3.5 modules
        let expected = [
            (tl.0 as f32 + 3.5 * m as f32, tl.1 as f32 + 3.5 * m as f32),
            (tr.0 as f32 + 3.5 * m as f32, tr.1 as f32 + 3.5 * m as f32),
            (bl.0 as f32 + 3.5 * m as f32, bl.1 as f32 + 3.5 * m as f32),
        ];
        for e in expected {
            let hit = finders.iter().any(|f| {
                let (cx, cy) = center(f);
                (cx - e.0).abs() < m as f32 && (cy - e.1).abs() < m as f32
            });
            assert!(hit, "finder at {e:?} not detected; got {finders:?}");
        }
        for f in &finders {
            assert!(
                (f.module - m as f32).abs() <= 0.6,
                "module estimate off: {}",
                f.module
            );
        }
    }
}

#[cfg(test)]
mod perspective_tests {
    use super::*;
    use barcode_core::geometry::{apply_h, fit_homography};

    /// Render the fixture canvas through a homography: each output pixel
    /// is inverse-mapped into the source's module space and samples the
    /// axis-aligned render. This is a TRUE perspective image (what the
    /// camera sees), not an affine skew.
    fn warp(canvas: &barcode_fixtures::Canvas, h: [f64; 9], out: u32) -> (Vec<u8>, u32) {
        let m = 4.0f64; // encode_qr's module size
        let quiet = 6.0 * m;
        // module space -> source pixel: (u, v) -> (quiet + u*m, quiet + v*m)
        // so the inverse chain is: output -> (u,v) -> source pixel.
        let hi = {
            // invert the module->output H, then compose with the pixel shift
            let inv = invert3_pub(h);
            // composed: out -> module u,v  ->  src px
            let (a, b, c, d, e, f, g, hh, ii) =
                (inv[0] * m, inv[1] * m, inv[2] * m + quiet, inv[3] * m, inv[4] * m, inv[5] * m + quiet, inv[6], inv[7], inv[8]);
            [a, b, c, d, e, f, g, hh, ii]
        };
        let mut data = vec![0u8; ((out + 31) / 32 * 4) as usize * out as usize];
        let bpr = ((out + 31) / 32 * 4) as usize;
        for y in 0..out {
            for x in 0..out {
                let (sx, sy) = apply_h(&hi, x as f64, y as f64);
                let (sxi, syi) = (sx.round() as i64, sy.round() as i64);
                let ink = sxi >= 0 && syi >= 0
                    && sxi < canvas.width as i64 && syi < canvas.height as i64
                    && canvas.bits()[syi as usize * canvas.bytes_per_row as usize + (sxi as usize >> 3)]
                        >> (sxi as usize & 7)
                        & 1
                        == 1;
                if ink {
                    data[y as usize * bpr + (x as usize >> 3)] |= 1 << (x & 7);
                }
            }
        }
        (data, bpr as u32)
    }

    fn invert3_pub(m: [f64; 9]) -> [f64; 9] {
        let (a, b, c, d, e, f, g, hh, i) = (m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8]);
        let det = a * (e * i - f * hh) - b * (d * i - f * g) + c * (d * hh - e * g);
        assert!(det.abs() > 1e-12);
        [
            (e * i - f * hh) / det, (c * hh - b * i) / det, (b * f - c * e) / det,
            (f * g - d * i) / det, (a * i - c * g) / det, (c * d - a * f) / det,
            (d * hh - e * g) / det, (b * g - a * hh) / det, (a * e - b * d) / det,
        ]
    }

    /// Render `text` at the requested version (auto-selected) through a
    /// strong perspective quad and return the decoder result plus the
    /// version the fixture chose.
    fn decode_perspective(text: &str, out: u32) -> (usize, String) {
        let canvas = barcode_fixtures::encode_qr_text(text, 1, 3, 4).expect("fixture encode");
        let m = 4.0f64;
        let n = canvas.width as f64 / m - 12.0;
        let version = ((n as u32) - 17) / 4;

        // Strong perspective: the symbol plane tilted back — the top edge
        // compressed, the bottom edge long, plus rotation.
        let s = out as f64 / 700.0;
        let quad = [
            (140.0 * s, 60.0 * s),
            (560.0 * s, 130.0 * s),
            (620.0 * s, 600.0 * s),
            (50.0 * s, 560.0 * s),
        ];
        let corners = [(0.0, 0.0), (n, 0.0), (n, n), (0.0, n)];
        let pairs: Vec<(f64, f64, f64, f64)> = corners
            .iter()
            .zip(quad.iter())
            .map(|(&(u, v), &(x, y))| (u, v, x, y))
            .collect();
        let h = fit_homography(&pairs).expect("homography from the quad");

        let (wbits, _wbpr) = warp(&canvas, h, out);
        let img = BitImage::new(out, out, &wbits);

        let mut d = QrDecoder::new();
        let results = d.decode(&img);
        (version as usize, results.first().map(|r| r.text.clone()).unwrap_or_default())
    }

    /// Item 3 acceptance: the finders alone cannot bend far enough for a
    /// strongly perspective-tilted symbol; the alignment patterns anchor
    /// the far corner and the homography decodes.
    #[test]
    fn decodes_a_strongly_perspective_distorted_symbol() {
        // v2: one alignment pattern bottom-right (the old ignored case).
        let text = "PERSPECTIVE QR TEST 0123456789";
        let (version, got) = decode_perspective(text, 700);
        assert_eq!(version, 2, "fixture must be version 2");
        assert_eq!(got, text, "v2 perspective decode");
    }

    /// v7+ has a grid of alignment patterns (six at version 7) across the
    /// symbol, which is the case a single far-corner anchor cannot cover.
    #[test]
    fn decodes_a_strongly_perspective_distorted_v7_symbol() {
        let text = "QR PERSPECTIVE ALIGNMENT PATTERN TEST 0123456789 ".repeat(4);
        let (version, got) = decode_perspective(&text, 1400);
        assert!(version >= 7, "fixture must be v7+, got v{version}");
        assert_eq!(got, text, "v{version} perspective decode");
    }
}
