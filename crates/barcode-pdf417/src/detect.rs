//! Scan-line detection and decode of PDF417 symbols in binarized
//! frames (v0: axis-aligned, upright).
//!
//! Strategy: every scan line is searched for a start pattern
//! [8,1,1,1,1,1,1,3] paired with a stop pattern [7,1,1,3,1,1,1,2,1]
//! at consistent module scale; each hit parses its own row structure
//! and identifies its symbol row via the left row indicator (the
//! cluster comes from trial-decoding the first data cell — the three
//! cluster tables are disjoint). Cell elements are majority-voted per
//! symbol row across scan lines; RS is the final arbiter.

use crate::modes::interpret;
use crate::stream::strip_pads;
use crate::tables::value_of;
use std::collections::HashMap;

const START_RUNS: [u8; 8] = [8, 1, 1, 1, 1, 1, 1, 3];
const STOP_RUNS: [u8; 9] = [7, 1, 1, 3, 1, 1, 1, 2, 1];
const MIN_S: u32 = 2;
const MAX_S: u32 = 40;

/// A decoded frame hit: payload plus the symbol's bounding quad
/// (TL, TR, BR, BL — axis-aligned in v0).
#[derive(Debug, Clone)]
pub struct FrameDecode {
    pub payload: Vec<u8>,
    pub quad: [f64; 8],
}

/// Decode the first PDF417 symbol found in a binarized frame
/// (`dark[y * w + x] != 0` = ink). Returns the payload bytes and the
/// symbol's bounding quad.
pub fn read_frame(dark: &[u8], w: usize, h: usize) -> Option<FrameDecode> {
    let mut hits = scan_candidates(dark, w, h);
    // Cluster hits into symbol footprints before voting: edge noise jitters
    // x0/x1 by a few px (and the pitch estimates with them), so exact-key
    // grouping would split one symbol into fragments that each see only
    // part of its codeword rows. Hits arrive in scan-line order.
    hits.sort_by_key(|hit| (hit.y, hit.x0));
    let mut clusters: Vec<(f64, f64, usize, Vec<(usize, f64, f64)>)> = Vec::new();
    for hit in hits {
        let rows = match clusters.iter_mut().find(|(x0, x1, ncols, _)| {
            *ncols == hit.ncols
                && (*x0 - hit.x0 as f64).abs() <= 6.0
                && (*x1 - hit.x1 as f64).abs() <= 6.0
        }) {
            Some((x0, x1, _, rows)) => {
                let n = rows.len() as f64;
                *x0 += (hit.x0 as f64 - *x0) / (n + 1.0);
                *x1 += (hit.x1 as f64 - *x1) / (n + 1.0);
                rows
            }
            None => {
                clusters.push((hit.x0 as f64, hit.x1 as f64, hit.ncols, Vec::new()));
                &mut clusters.last_mut().unwrap().3
            }
        };
        rows.push((hit.y, hit.s0, hit.s1));
    }
    clusters.sort_by_key(|(_, _, _, rows)| std::cmp::Reverse(rows.len()));
    for (x0, x1, ncols, rows) in &clusters {
        if let Some((payload, (bx0, bx1, by0, by1))) =
            decode_symbol(dark, w, (*x0 as usize, *x1 as usize, *ncols), rows)
        {
            let quad = [
                bx0 as f64, by0 as f64, bx1 as f64, by0 as f64, bx1 as f64, by1 as f64, bx0 as f64,
                by1 as f64,
            ];
            return Some(FrameDecode { payload, quad });
        }
    }
    None
}

#[derive(Clone, Copy)]
struct Hit {
    x0: usize,
    x1: usize,
    /// Fractional pitch at the start/stop ends: camera keystoning tapers
    /// the pitch across the symbol (~12% on the phone rig), so elements
    /// are snapped against a linearly interpolated pitch, not a constant.
    s0: f64,
    s1: f64,
    y: usize,
    ncols: usize,
}

/// Row runs as (dark, length) starting with the first pixel's color.
fn runs_of(row: &[u8]) -> Vec<(bool, u32)> {
    let mut out = Vec::new();
    let mut cur = row[0] != 0;
    let mut n = 0u32;
    for &d in row {
        let dark = d != 0;
        if dark == cur {
            n += 1;
        } else {
            out.push((cur, n));
            cur = dark;
            n = 1;
        }
    }
    out.push((cur, n));
    out
}

fn match_runs(runs: &[(bool, u32)], offset: usize, nominal: &[u8], s: u32) -> bool {
    if offset + nominal.len() > runs.len() {
        return false;
    }
    for (k, &want) in nominal.iter().enumerate() {
        let (dark, ln) = runs[offset + k];
        if dark != (k % 2 == 0) {
            return false;
        }
        // A long solid run loses up to a module at EACH end to camera blur
        // and keystone: the 8-module start/stop block on a real boarding
        // pass measured 50px against a 61px ideal while the neighbouring
        // 1-module elements still measured 7-10px. A flat ±s/2 tolerance
        // rejected it, so PDF417 only read on a lucky close-up frame.
        // Budget the tolerance as half a module + one eighth per module;
        // the RS/format validation stays the final arbiter.
        let tol = if want >= 3 {
            (want as u32 * s).div_ceil(8) + s / 2
        } else {
            s / 2
        };
        let lo = (want as u32 * s).saturating_sub(tol);
        let hi = want as u32 * s + tol + 1; // +1: screen sub-pixel seams
        if ln < lo || ln > hi {
            return false;
        }
    }
    true
}

fn scan_candidates(dark: &[u8], w: usize, h: usize) -> Vec<Hit> {
    let mut hits = Vec::new();
    for y in 0..h {
        let row = &dark[y * w..y * w + w];
        let runs = runs_of(row);
        if runs.len() < 17 {
            continue;
        }
        // run start positions
        let mut xs = Vec::with_capacity(runs.len());
        let mut acc = 0u32;
        for (_, ln) in &runs {
            xs.push(acc);
            acc += ln;
        }
        let mut starts = Vec::new();
        for i in 0..runs.len().saturating_sub(7) {
            let w17: u32 = runs[i..i + 8].iter().map(|r| r.1).sum();
            // s candidates from the window width: the 17-module sum carries
            // edge noise, so exact divisibility by 17 (true only for 1px
            // synthetic matrices) never fires on camera frames.
            let est = w17 as f64 / 17.0;
            for s in [est.floor() as u32, est.ceil() as u32] {
                if !(MIN_S..=MAX_S).contains(&s) || !match_runs(&runs, i, &START_RUNS, s) {
                    continue;
                }
                starts.push((i, s, est, xs[i]));
                break;
            }
        }
        if starts.is_empty() {
            continue;
        }
        let mut stops = Vec::new();
        for i in 0..runs.len().saturating_sub(8) {
            let w18: u32 = runs[i..i + 9].iter().map(|r| r.1).sum();
            let est = w18 as f64 / 18.0;
            for s in [est.floor() as u32, est.ceil() as u32] {
                if !(MIN_S..=MAX_S).contains(&s) || !match_runs(&runs, i, &STOP_RUNS, s) {
                    continue;
                }
                stops.push((i, s, est, xs[i] + w18));
                break;
            }
        }
        for (_si, ss, s0, x0) in &starts {
            for (_ti, ts, s1, x1) in &stops {
                if ss.abs_diff(*ts) > (ss / 8).max(1) {
                    continue;
                }
                // Pitch tapers across the symbol (keystone); width in
                // modules is measured against the end-pitch average,
                // which is the correct divisor for the full span.
                let s_avg = (s0 + s1) / 2.0;
                let width_m = (*x1 - x0) as f64 / s_avg;
                let ncols = ((width_m - 69.0) / 17.0).round() as isize;
                if !(1..=30).contains(&ncols) {
                    continue;
                }
                // Pitch-estimate error scales with width (quantized integer
                // s at both ends + keystone taper), so the tolerance does
                // too; capped well under half the 17-module ncols spacing.
                let tol = (1.0 + 0.015 * width_m).min(8.0);
                if (width_m - (69.0 + 17.0 * ncols as f64)).abs() > tol {
                    continue;
                }
                hits.push(Hit {
                    x0: *x0 as usize,
                    x1: *x1 as usize,
                    s0: *s0,
                    s1: *s1,
                    y,
                    ncols: ncols as usize,
                });
            }
        }
    }
    hits
}

/// Parse one scan line into cells (module elements), or None.
fn parse_line(
    dark: &[u8],
    w: usize,
    y: usize,
    x0: usize,
    x1: usize,
    s0: f64,
    s1: f64,
    ncols: usize,
) -> Option<Vec<Vec<u8>>> {
    // Slice with a margin: the cluster's x0/x1 are means over jittering
    // per-row estimates, so the raw slice can clip the start/stop bars.
    // The margin only ever reaches into the quiet zone, which the start
    // search below simply skips over.
    let lo = x0.saturating_sub(8);
    let hi = (x1 + 8).min(w);
    let row = &dark[y * w + lo..y * w + hi];
    let runs = runs_of(row);
    // Snap every run to (color, modules) with a locally accumulated,
    // pitch-interpolated grid: a constant pitch (integer or fractional)
    // accumulates rounding drift across a keystoned symbol and collapses
    // small runs. Sub-module noise runs snap to 0 and vanish; if that
    // merges two runs of one color, the merge is color-correct by
    // construction (elements alternate starting dark).
    let span = (x1 - x0) as f64;
    let mut elems: Vec<(bool, usize, u32)> = Vec::new();
    let mut pos = 0u32;
    for &(rdark, ln) in runs.iter() {
        let mid = pos as f64 + ln as f64 / 2.0;
        let pitch = s0 + (s1 - s0) * (mid / span);
        let wd = (ln as f64 / pitch + 0.5).floor() as usize;
        let at = pos;
        pos += ln;
        if wd == 0 {
            continue; // sub-module noise
        }
        match elems.last_mut() {
            Some((d, n, _)) if *d == rdark => *n += wd,
            _ => elems.push((rdark, wd, at)),
        }
    }
    // Locate the code: 8 elements alternating from dark with nominal
    // start widths. Anything before it (quiet, quiet-zone noise) is
    // ignored; anything past the stop never enters the extraction.
    //
    // The widths are SNAPPED to modules, and camera blur erodes a solid
    // run by up to a module at each end: the 8-module block measured 6
    // after snapping on a real boarding pass. This is a LOCATOR, not the
    // sample grid — the grid below is anchored on the pixel edges — so
    // allow a couple of modules on the wide runs. Demanding an exact 8
    // here lost every scanline through a blurred pass.
    let elems_match = |i: usize, nominal: &[u8]| -> bool {
        i + nominal.len() <= elems.len()
            && (0..nominal.len()).all(|k| {
                let want = nominal[k] as i64;
                elems[i + k].0 == (k % 2 == 0)
                    && (elems[i + k].1 as i64 - want).abs() <= if want >= 3 { 2 } else { 1 }
            })
    };
    let start = (0..elems.len().saturating_sub(7)).find(|&i| elems_match(i, &START_RUNS))?;
    let mut bounds = vec![17usize, 34];
    for k in 0..ncols {
        bounds.push(34 + 17 * (k + 1));
    }
    bounds.push(34 + 17 * ncols + 17);
    bounds.push(34 + 17 * ncols + 35);

    // 1) Run-based extraction — exact for synthetic prints (the
    //    historical path; pixel runs map 1:1 to module elements).
    let from_elems = || -> Option<Vec<Vec<u8>>> {
        let mut cells = Vec::with_capacity(bounds.len());
        let mut pos = 0usize; // modules consumed since the start bar edge
        let mut idx = start; // element index into `elems`
        let mut widths: Vec<usize> = elems.iter().map(|&(_, n, _)| n).collect();
        for (ci, &end_m) in bounds.iter().enumerate() {
            let mut cell = Vec::new();
            while pos < end_m {
                let n = *widths.get(idx)?;
                let take = (end_m - pos).min(n) as u8;
                cell.push(take);
                pos += take as usize;
                if take as usize == n {
                    idx += 1;
                    if idx >= widths.len() && pos < end_m {
                        return None;
                    }
                } else {
                    widths[idx] -= take as usize;
                }
            }
            let want_n = if ci == bounds.len() - 1 { 9 } else { 8 };
            let want_sum = if ci == bounds.len() - 1 { 18 } else { 17 };
            if cell.len() != want_n
                || cell.iter().sum::<u8>() != want_sum
                || cell.iter().min() == Some(&0)
            {
                return None;
            }
            cells.push(cell);
        }
        Some(cells)
    };
    if let Some(cells) = from_elems() {
        return Some(cells);
    }

    // 2) Sampled-grid extraction. Real prints jitter run edges by ±1px;
    //    at a ~3px module that flips a 1-module element between 2 and
    //    4px, and a row's cells then come out with 6 or 9 elements and
    //    the whole scanline is lost. Sampling module CENTRES between the
    //    detected start and stop patterns is immune to edge jitter — the
    //    boarding-pass specimen that never parsed yields valid cells on
    //    most of its rows this way.
    let stop = (0..elems.len().saturating_sub(8)).find(|&i| elems_match(i, &STOP_RUNS))?;
    let start_px = elems[start].2;
    let stop_end = if stop + 9 < elems.len() {
        elems[stop + 9].2
    } else {
        (hi - lo) as u32
    };
    if stop_end <= start_px {
        return None;
    }
    let modules = 69 + 17 * ncols;
    let pitch = (stop_end - start_px) as f64 / modules as f64;
    if pitch < 1.0 {
        return None;
    }
    let mut cells = Vec::with_capacity(bounds.len());
    let mut from = 0usize;
    for (ci, &end_m) in bounds.iter().enumerate() {
        let seg = &bits_window(row, start_px, pitch, from, end_m);
        from = end_m;
        let mut cur = seg[0];
        if !cur {
            return None; // every cell starts with a bar
        }
        let mut cell: Vec<u8> = Vec::new();
        let mut n = 0u8;
        for &b in seg {
            if b == cur {
                n += 1;
            } else {
                cell.push(n);
                cur = b;
                n = 1;
            }
        }
        cell.push(n);
        let want_n = if ci == bounds.len() - 1 { 9 } else { 8 };
        let want_sum = if ci == bounds.len() - 1 { 18 } else { 17 };
        if cell.len() != want_n
            || cell.iter().sum::<u8>() != want_sum
            || cell.iter().min() == Some(&0)
        {
            return None;
        }
        cells.push(cell);
    }
    Some(cells)
}

/// Module-centre bits for modules `from..to` of one scan line, the grid
/// anchored at `start_px` with a constant `pitch`. Each sample is a 3px
/// majority (edge jitter, binarizer speckle).
fn bits_window(row: &[u8], start_px: u32, pitch: f64, from: usize, to: usize) -> Vec<bool> {
    let mut out = Vec::with_capacity(to - from);
    for m in from..to {
        let center = start_px as f64 + (m as f64 + 0.5) * pitch;
        let c = center.round() as i64;
        let mut dark = 0;
        for d in -1i64..=1 {
            let i = c + d;
            if i >= 0 && i < row.len() as i64 && row[i as usize] != 0 {
                dark += 1;
            }
        }
        out.push(dark >= 2);
    }
    out
}

/// Decode one symbol footprint: per-scan-line row identification,
/// majority vote, stream assembly, RS, mode interpretation.
fn decode_symbol(
    dark: &[u8],
    w: usize,
    key: (usize, usize, usize),
    ys: &[(usize, f64, f64)],
) -> Option<(Vec<u8>, (usize, usize, usize, usize))> {
    let (x0, x1, ncols) = key;
    // votes[symbol row][cell index][(elements)] -> count
    let mut votes: HashMap<usize, HashMap<usize, HashMap<Vec<u8>, usize>>> = HashMap::new();
    let mut why = [0usize; 4]; // parse / cluster / value / row-mismatch
    for &(y, s0, s1) in ys {
        let Some(cells) = parse_line(dark, w, y, x0, x1, s0, s1, ncols) else {
            why[0] += 1;
            continue;
        };
        // cluster trial via the first data cell
        let cluster = (0..=6)
            .step_by(3)
            .find(|&c| value_of(c, &cells[2]).is_some());
        let Some(cluster) = cluster else {
            why[1] += 1;
            continue;
        };
        let Some(left) = value_of(cluster, &cells[1]) else {
            why[2] += 1;
            continue;
        };
        let Some(right) = value_of(cluster, &cells[2 + ncols]) else {
            why[2] += 1;
            continue;
        };
        let x_ind = left / 30;
        if right / 30 != x_ind {
            why[3] += 1;
            continue;
        }
        let row = (3 * x_ind + (cluster as u16) / 3 + 1) as usize;
        let per = votes.entry(row).or_default();
        for (i, cell) in cells.iter().enumerate() {
            *per.entry(i).or_default().entry(cell.clone()).or_insert(0) += 1;
        }
    }
    if votes.is_empty() {
        return None;
    }
    let nrows = *votes.keys().max()?;
    // Second pass: a row whose indicator codewords never survived the
    // cell jitter (crease, keystone, low print contrast) would otherwise
    // fail the whole symbol ("a codeword row is missing entirely").
    // Rows are evenly spaced across the cluster's scanline span, so the
    // remaining scanlines can be assigned by vertical position and their
    // DATA cells voted in — the row's cluster is known from the index.
    let missing: Vec<usize> = (1..=nrows).filter(|r| !votes.contains_key(r)).collect();
    if !missing.is_empty() {
        let (ymin, ymax) = ys
            .iter()
            .fold((usize::MAX, 0), |(a, b), &(y, ..)| (a.min(y), b.max(y)));
        let pitch = (ymax - ymin + 1) as f64 / nrows as f64;
        for &(y, s0, s1) in ys {
            let Some(cells) = parse_line(dark, w, y, x0, x1, s0, s1, ncols) else {
                continue;
            };
            let row = (((y as f64 - ymin as f64) / pitch).floor() as usize).min(nrows - 1) + 1;
            if !missing.contains(&row) {
                continue;
            }
            let per = votes.entry(row).or_default();
            for (i, cell) in cells.iter().enumerate().skip(2).take(ncols) {
                *per.entry(i).or_default().entry(cell.clone()).or_insert(0) += 1;
            }
        }
    }
    let mut stream = Vec::new();
    // A row whose cells never survived the print (a boarding-pass crease
    // measured 7 of 19 rows) is inserted as zero placeholders: RS treats
    // the wrong codewords as errors, and the symbol's EC level is chosen
    // for exactly this. Previously one lost row failed the whole symbol.
    let mut placeholders: Vec<usize> = Vec::new();
    for row in 1..=nrows {
        let cluster = ((row - 1) % 3) * 3;
        let per = votes.get(&row);
        for i in 2..2 + ncols {
            let value = per
                .and_then(|cells| cells.get(&i))
                .and_then(|seen| seen.iter().max_by_key(|(_, &c)| c))
                .and_then(|(cell, _)| value_of(cluster, cell));
            match value {
                Some(v) => stream.push(v),
                None => {
                    placeholders.push(stream.len());
                    stream.push(0);
                }
            }
        }
    }
    // Mostly-unreadable footprint: reject before RS (see correct_stream).
    if placeholders.len() * 4 > stream.len() * 3 {
        return None;
    }
    let full = crate::correct_stream(stream, &placeholders)?;
    let desc = full[0] as usize;
    let data = strip_pads(&full[1..desc]);
    let payload = interpret(data)?;
    // bounding box from the identified scan lines (± half a band)
    let (ymin, ymax) = ys
        .iter()
        .fold((usize::MAX, 0), |(a, b), &(y, ..)| (a.min(y), b.max(y)));
    let quad = (
        x0,
        x1,
        ymin.saturating_sub(1),
        (ymax + 2).min(dark.len() / w),
    );
    Some((payload, quad))
}

// ---------------------------------------------------------------------------
// Rectified detection: the v0 path needs near-horizontal rows. A phone photo
// shears them (the KLM boarding pass: horizontal edges 1.86 deg vs vertical
// 0.41 deg), so a horizontal scanline crosses 2-3 symbol rows and the run
// parser starves. Instead: estimate the symbol's FOUR CORNERS, project the
// module plane through a homography (barcode_core::geometry, shared with the
// other 2D families), sample the module grid, and hand the exact matrix to
// the existing codeword/RS decoder.
// ---------------------------------------------------------------------------

use barcode_core::{BitImage, Point, Quad};

/// Least-squares fit `v = a + b*u` through points.
fn fit_linear(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    if pts.len() < 4 {
        return None;
    }
    let n = pts.len() as f64;
    let su: f64 = pts.iter().map(|p| p.0).sum();
    let sv: f64 = pts.iter().map(|p| p.1).sum();
    let suu: f64 = pts.iter().map(|p| p.0 * p.0).sum();
    let suv: f64 = pts.iter().map(|p| p.0 * p.1).sum();
    let den = n * suu - su * su;
    if den.abs() < 1e-9 {
        return None;
    }
    let b = (n * suv - su * sv) / den;
    let a = (sv - b * su) / n;
    Some((a, b))
}

/// The longest dark run **along an edge line** `x = a + b*y` (offset into
/// the bar) within `[y0, y1]`, as (top, bottom) pixel rows. The PDF417
/// start/stop bars are solid for the symbol's full height; following the
/// tilted edge keeps the probe inside the 1-module stop bar.
fn edge_vertical_run(
    dark: &[u8],
    w: usize,
    y0: usize,
    y1: usize,
    edge: (f64, f64),
    offset: i64,
) -> Option<(usize, usize)> {
    if y1 <= y0 {
        return None;
    }
    let mut best: Option<(usize, usize)> = None;
    let mut start: Option<usize> = None;
    for y in y0..=y1 {
        let x = (edge.0 + edge.1 * y as f64).round() as i64 + offset;
        let ink = x >= 0 && (x as usize) < w && dark[y * w + x as usize] != 0;
        match (ink, start) {
            (true, None) => start = Some(y),
            (false, Some(s)) => {
                let len = y - s;
                if best.map_or(true, |(a, b)| len > b - a) {
                    best = Some((s, y - 1));
                }
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        let len = y1 - s + 1;
        if best.map_or(true, |(a, b)| len > b - a) {
            best = Some((s, y1));
        }
    }
    best
}

/// Estimate the symbol quadrilateral and its data-column count from the
/// start/stop hits plus the ink edges above and below them.
fn estimate_quad(hits: &[Hit], dark: &[u8], w: usize, h: usize) -> Option<(Quad, usize)> {
    if hits.len() < 12 {
        return None;
    }
    // The hits should agree on the data-column count; use the mode.
    let mut counts = std::collections::HashMap::new();
    for hit in hits {
        *counts.entry(hit.ncols).or_insert(0usize) += 1;
    }
    let Some((&ncols, _)) = counts.iter().max_by_key(|(_, &c)| c) else {
        return None;
    };
    if !(1..=30).contains(&ncols) {
        return None;
    }

    // Left/right edges: the start bar's left edge and the stop bar's right
    // edge per scanline (symbol outer edges), vs the scanline's y.
    let mut left_pts = Vec::with_capacity(hits.len());
    let mut right_pts = Vec::with_capacity(hits.len());
    for hit in hits {
        left_pts.push((hit.y as f64, hit.x0 as f64));
        right_pts.push((hit.y as f64, hit.x1 as f64));
    }
    let left = fit_linear(&left_pts)?;
    let right = fit_linear(&right_pts)?;

    let y_lo = hits.iter().map(|hit| hit.y).min()? as f64;
    let y_hi = hits.iter().map(|hit| hit.y).max()? as f64;
    let x_lo = hits.iter().map(|hit| hit.x0).min()? as f64;
    let x_hi = hits.iter().map(|hit| hit.x1).max()? as f64;
    if x_hi - x_lo < 24.0 || y_hi - y_lo < 8.0 {
        return None;
    }

    // Top/bottom corners from the START and STOP bars' vertical extents.
    // Both are solid bars spanning the symbol's full height, so the longest
    // dark run in their column gives the corner pair directly. The previous
    // first/last-ink-per-column profile is jagged by the first data row
    // (±6px) and picks up text above the symbol — on the boarding pass it
    // put the top edge 4-12px low, which shifted row 0's right side into
    // the next row.
    let band = ((y_hi - y_lo) / 4.0).clamp(10.0, 48.0);
    let y_scan0 = (y_lo - 2.0 * band).max(0.0) as usize;
    let y_scan1 = ((y_hi + 2.0 * band) as usize).min(h - 1);
    // Just inside each bar (the start bar is ~8 modules wide, the stop's
    // final bar 1 module; 2px in is safely inside both).
    let (lt, lb) = match edge_vertical_run(dark, w, y_scan0, y_scan1, left, 2) {
        Some(run) => run,
        None => return None,
    };
    let (rt, rb) = match edge_vertical_run(dark, w, y_scan0, y_scan1, right, -2) {
        Some(run) => run,
        None => return None,
    };
    let tl = Point {
        x: (left.0 + left.1 * lt as f64) as f32,
        y: lt as f32,
    };
    let bl = Point {
        x: (left.0 + left.1 * lb as f64) as f32,
        y: lb as f32,
    };
    let tr = Point {
        x: (right.0 + right.1 * rt as f64) as f32,
        y: rt as f32,
    };
    let br = Point {
        x: (right.0 + right.1 * rb as f64) as f32,
        y: rb as f32,
    };
    // Sanity: a wide, short, positively oriented quadrilateral inside a
    // small margin of the frame.
    let margin = 16.0;
    for p in [tl, tr, br, bl] {
        if p.x < -margin || p.y < -margin || p.x > w as f32 + margin || p.y > h as f32 + margin {
            return None;
        }
    }
    let width = ((tr.x - tl.x) + (br.x - bl.x)) / 2.0;
    let height = ((bl.y - tl.y) + (br.y - tr.y)) / 2.0;
    if width < 24.0 || height < 6.0 || width < height {
        return None;
    }
    Some(([tl, tr, br, bl], ncols))
}

/// One sampled symbol-row line: the module bits along the quad's row
/// line at fraction `tv`, horizontally anchored on the start bar.
struct RowLine {
    bits: Vec<bool>,
}

/// Run lengths of a module-bit run, or `None` unless it starts with a bar
/// (cell patterns are bar-first).
fn bits_runs(bits: &[bool]) -> Option<Vec<u8>> {
    if bits.is_empty() || !bits[0] {
        return None;
    }
    let mut out: Vec<u8> = Vec::with_capacity(9);
    let mut cur = true;
    let mut n = 0u8;
    for &b in bits {
        if b == cur {
            n += 1;
        } else {
            out.push(n);
            cur = b;
            n = 1;
        }
    }
    out.push(n);
    Some(out)
}

impl RowLine {
    /// Run lengths of modules `from..to`, or `None` when the window does
    /// not look like a codeword (must start with a bar and have 8
    /// elements).
    fn elements(&self, from: usize, to: usize) -> Option<Vec<u8>> {
        let runs = bits_runs(&self.bits[from..to])?;
        if runs.len() == 8 && runs.iter().sum::<u8>() == 17 {
            Some(runs)
        } else {
            None
        }
    }
}

/// Sample the module bits along one symbol-row line at `tv`. Horizontal
/// position is anchored per line on the start bar and the stop bar's
/// final 1-module bar (`±6px` search), so print bleed / keystone taper
/// does not accumulate a pitch error across 5-30 data columns. Vertical
/// position follows the row line, so shear is already removed by the
/// quad. Each module is integrated vertically (5 samples) because a
/// thermal print erodes 1-module bars to a single pixel that a 3x3
/// majority rejects.
fn sample_row_line(
    img: &BitImage,
    quad: &barcode_core::Quad,
    cols: u32,
    tv: f32,
) -> Option<RowLine> {
    let [tl, tr, br, bl] = *quad;
    let w = img.width as i64;
    let h = img.height as i64;
    let ink = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h && img.ink(x as u32, y as u32);
    let left_x = tl.x + (bl.x - tl.x) * tv;
    let left_y = tl.y + (bl.y - tl.y) * tv;
    let right_x = tr.x + (br.x - tr.x) * tv;
    let right_y = tr.y + (br.y - tr.y) * tv;
    let probe_y = left_y.round() as i64;
    let mut x_left = left_x;
    for dx in -6i64..=6 {
        let x = left_x.round() as i64 + dx;
        if ink(x, probe_y) && ink(x + 1, probe_y) && ink(x + 2, probe_y) {
            x_left = x as f32;
            break;
        }
    }
    let probe_y = right_y.round() as i64;
    let mut x_right = right_x;
    for dx in -6i64..=6 {
        let x = right_x.round() as i64 - dx;
        if ink(x, probe_y) {
            x_right = x as f32;
            break;
        }
    }
    if x_right <= x_left + 8.0 {
        return None;
    }
    let pitch = (x_right + 1.0 - x_left) / cols as f32;
    // The quad-derived origin is good enough for the data/indicator
    // modules; the start pattern itself is sync, and the tolerant stream
    // reader no longer discards a row over an eroded edge bar. (Aligning
    // the origin to the start pattern was tried and *hurt* the indicator
    // sampling: a 1px shift flips the 1-module indicator elements.)
    let mut bits = Vec::with_capacity(cols as usize);
    for u in 0..cols {
        let tu = (u as f32 + 0.5) / cols as f32;
        let cx = (x_left + pitch * (u as f32 + 0.5)).round() as i64;
        let cy = (left_y + (right_y - left_y) * tu).round() as i64;
        let mut count = 0;
        for dy in -2i64..=2 {
            if ink(cx, cy + dy) {
                count += 1;
            }
        }
        bits.push(count >= 3);
    }
    Some(RowLine { bits })
}

/// The symbol row this line belongs to (0-based), read from its left row
/// indicator. The cluster is unknown a priori, so trial-decode the first
/// data column against the three cluster tables (the same trick the
/// scanline path uses), then require the right indicator to agree on the
/// row-index group `x`.
fn line_row(line: &RowLine, modules: u32) -> Option<usize> {
    let m = modules as usize;
    let first_data = line.elements(34, 51)?;
    let cluster = [0usize, 3, 6]
        .into_iter()
        .find(|&c| value_of(c, &first_data).is_some())?;
    let left = value_of(cluster, &line.elements(17, 34)?)?;
    let x_ind = left / 30;
    let right = value_of(cluster, &line.elements(m - 35, m - 18)?)?;
    if right / 30 != x_ind {
        return None;
    }
    Some(3 * x_ind as usize + cluster / 3)
}

/// Map every symbol-row band to the `tv` range it actually occupies by
/// reading the row indicator at each vertical step. A crease displaces
/// the middle rows by more than a row height, which uniform v spacing
/// cannot follow (the KLM boarding pass: 19 rows, all indicators valid,
/// but the middle rows' cells flipped). Returns `(tv, row)`.
fn row_labels(img: &BitImage, quad: &barcode_core::Quad, modules: u32) -> Vec<(f32, usize)> {
    let [tl, _tr, br, bl] = *quad;
    let height = ((bl.y - tl.y) + (br.y - tl.y)) / 2.0;
    let steps = (height as usize).clamp(8, 4096);
    let mut out = Vec::with_capacity(steps / 2);
    for s in 0..=steps {
        let tv = s as f32 / steps as f32;
        let Some(line) = sample_row_line(img, quad, modules, tv) else {
            continue;
        };
        if let Some(row) = line_row(&line, modules) {
            out.push((tv, row));
        }
    }
    out
}

/// Sample the module grid using the decoded row labels: each symbol row
/// is sampled only inside the vertical band its row indicator says it
/// occupies (middle 70%, so boundary lines that belong to the neighbour
/// are excluded). Rows whose indicator never decoded get their band
/// interpolated from the nearest decoded neighbours — a crease moves the
/// band by a few pixels, and uniform spacing puts the sample on the wrong
/// row exactly where the crease is (the KLM pass: rows 12-15 drift ~5px).
fn sample_rows_snapped(
    img: &BitImage,
    quad: &barcode_core::Quad,
    modules: u32,
    rows: u32,
    labels: &[(f32, usize)],
) -> barcode_core::geometry::BitGrid {
    const MAX_LINES: usize = 16;
    let nrows = rows as usize;
    // Observed band per decoded row.
    let mut seen: Vec<Option<(f32, f32)>> = vec![None; nrows];
    for v in 0..nrows {
        let mut tvs: Vec<f32> = labels
            .iter()
            .filter(|(_, row)| *row == v)
            .map(|(tv, _)| *tv)
            .collect();
        if tvs.len() >= 3 {
            tvs.sort_unstable_by(|a, b| a.total_cmp(b));
            let n = tvs.len();
            let lo = n * 15 / 100;
            let hi = (n - lo).max(lo + 1);
            seen[v] = Some((tvs[lo], tvs[hi - 1]));
        }
    }
    let centers: Vec<Option<f32>> = seen
        .iter()
        .map(|b| b.map(|(lo, hi)| (lo + hi) / 2.0))
        .collect();
    // Fill missing bands by linear interpolation between the nearest
    // decoded rows (extrapolate with the average local spacing at the ends).
    let mut bands: Vec<(f32, f32)> = vec![(0.0, 0.0); nrows];
    for v in 0..nrows {
        if let Some(band) = seen[v] {
            bands[v] = band;
            continue;
        }
        let prev = (0..v).rev().find(|&p| centers[p].is_some());
        let next = (v + 1..nrows).find(|&n| centers[n].is_some());
        let half = match (prev, next) {
            (Some(p), Some(n)) => {
                let span = (n - p) as f32;
                let c = centers[p].unwrap()
                    + (centers[n].unwrap() - centers[p].unwrap()) * ((v - p) as f32 / span);
                let hp = seen[p].map(|(a, b)| b - a).unwrap_or(0.0);
                let hn = seen[n].map(|(a, b)| b - a).unwrap_or(hp);
                let hw = ((hp + (hn - hp) * ((v - p) as f32 / span)) / 2.0).max(0.01);
                (c, hw)
            }
            (Some(p), None) => {
                let step = if p > 0 {
                    centers[p].unwrap() - centers[p - 1].unwrap_or(centers[p].unwrap())
                } else {
                    0.0
                };
                (
                    centers[p].unwrap() + step * (v - p) as f32,
                    0.35 / rows as f32,
                )
            }
            (None, Some(n)) => {
                let step = if n + 1 < nrows {
                    centers[n + 1].unwrap_or(centers[n].unwrap()) - centers[n].unwrap()
                } else {
                    0.0
                };
                (
                    centers[n].unwrap() - step * (n - v) as f32,
                    0.35 / rows as f32,
                )
            }
            (None, None) => (((v as f32 + 0.5) / rows as f32), 0.35 / rows as f32),
        };
        let (c, hw) = half;
        bands[v] = ((c - hw).clamp(0.0, 1.0), (c + hw).clamp(0.0, 1.0));
    }
    let mut bits = Vec::with_capacity((modules * rows) as usize);
    for v in 0..nrows {
        let (lo_tv, hi_tv) = bands[v];
        let mut tvs: Vec<f32> = if lo_tv < hi_tv {
            // Uniform lines inside the band; 9 like the old spacing, capped.
            let n = ((hi_tv - lo_tv) * 400.0).round().clamp(3.0, 16.0) as usize;
            (0..n)
                .map(|k| lo_tv + (hi_tv - lo_tv) * (k as f32 + 0.5) / n as f32)
                .collect()
        } else {
            let mid = (v as f32 + 0.5) / rows as f32;
            (0..9)
                .map(|k| mid + (k as f32 - 4.0) * 0.7 / (9.0 * rows as f32))
                .collect()
        };
        if tvs.len() > MAX_LINES {
            let stride = tvs.len() as f32 / MAX_LINES as f32;
            tvs = (0..MAX_LINES)
                .map(|k| tvs[(k as f32 * stride) as usize])
                .collect();
        }
        let mut votes = vec![0u8; modules as usize];
        let mut used = 0u8;
        for tv in tvs {
            let Some(line) = sample_row_line(img, quad, modules, tv) else {
                continue;
            };
            used += 1;
            for (u, &bit) in line.bits.iter().enumerate() {
                if bit {
                    votes[u] += 1;
                }
            }
        }
        for vote in &votes {
            bits.push(used > 0 && *vote * 2 > used);
        }
    }
    barcode_core::geometry::BitGrid {
        cols: modules,
        rows,
        bits,
    }
}

/// Decode the first PDF417 symbol in a binarized frame, via the rectified
/// detector: corners -> projective module grid -> exact-matrix decoder.
/// This is the fallback for sheared/keystoned phone photos; the scanline
/// path stays the fast primary.
pub fn read_frame_rectified(img: &BitImage) -> Option<FrameDecode> {
    let w = img.width as usize;
    let h = img.height as usize;
    let mut dark = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            dark[y * w + x] = img.ink(x as u32, y as u32) as u8;
        }
    }
    let hits = scan_candidates(&dark, w, h);
    let (quad, ncols) = estimate_quad(&hits, &dark, w, h)?;
    let modules = 69 + 17 * ncols;
    let decode_grid = |grid: &barcode_core::geometry::BitGrid, rows: usize| {
        let modules_bits: Vec<u8> = grid.bits.iter().map(|&b| b as u8).collect();
        crate::decode_exact(&modules_bits, modules, rows)
    };
    let quad_f = [
        quad[0].x as f64,
        quad[0].y as f64,
        quad[1].x as f64,
        quad[1].y as f64,
        quad[2].x as f64,
        quad[2].y as f64,
        quad[3].x as f64,
        quad[3].y as f64,
    ];
    // The symbol's row count and per-row vertical bands come from the
    // row indicators themselves: scan the quad's row lines, decode the
    // indicator on each, and group by the row it names. This follows a
    // creased print instead of assuming uniform row spacing.
    let labels = row_labels(img, &quad, modules as u32);
    let snapped_rows = labels.iter().map(|(_, row)| row + 1).max();
    if let Some(rows) = snapped_rows {
        if (2..=90).contains(&rows) {
            let grid = sample_rows_snapped(img, &quad, modules as u32, rows as u32, &labels);
            if let Some(payload) = decode_grid(&grid, rows) {
                return Some(FrameDecode {
                    payload,
                    quad: quad_f,
                });
            }
        }
    }
    // Fallback: the row count is unknown before decoding; the symbol's
    // height in modules / the spec's 3-module minimum row height gives a
    // small ladder, and decode_exact is cheap — try the full legal range
    // with uniform row spacing.
    for rows in 2..=90usize {
        if Some(rows) == snapped_rows {
            continue; // already tried with better band placement
        }
        let grid = sample_rows_snapped(img, &quad, modules as u32, rows as u32, &[]);
        if let Some(payload) = decode_grid(&grid, rows) {
            return Some(FrameDecode {
                payload,
                quad: quad_f,
            });
        }
    }
    None
}
