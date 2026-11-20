//! L-finder detection for ECC 200: locate the solid finder L of Data
//! Matrix symbols in grayscale frames.
//!
//! Clean-room; geometry from ISO 16022, thresholds tuned on the captured
//! frame in `packages/web/test-codes/field` (docs/STATUS.md, "FIELD
//! FIXTURE GROUND TRUTH"). Pipeline:
//!
//! 1. [`binarize`] — local-mean threshold (integral image), reused later
//!    for module sampling.
//! 2. [`find_corners`] — dark runs → axis-aligned bars → touching
//!    stroke ends become L-corner candidates. RS verification of sampled
//!    grids (decoder side) is what separates real symbols from text, so
//!    this stage may over-report.

/// Row-major grayscale frame.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub w: usize,
    pub h: usize,
    pub luma: &'a [u8],
}

/// Binarized frame (`dark[y * w + x]`); also the input for module sampling.
pub struct BinaryMap {
    pub w: usize,
    pub h: usize,
    pub dark: Vec<bool>,
}

/// Which corner of the symbol the L wraps (in symbol orientation terms:
/// the L sits on the left+bottom of an upright symbol, so a BottomLeft
/// corner means the symbol is upright in the frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerKind {
    BottomLeft,
    BottomRight,
    TopRight,
    TopLeft,
}

/// A candidate finder-L corner, in frame pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub x: f32,
    pub y: f32,
    pub kind: CornerKind,
    /// Length of the horizontal stroke in px (size hint only — tilted
    /// strokes fragment, so this is a lower bound).
    pub hlen: i32,
    /// Length of the vertical stroke in px.
    pub vlen: i32,
    /// Stroke thicknesses in px: a finder leg is one module wide, so the
    /// thickness must track the implied pitch (used by the fast path to
    /// reject UI bands and text bars however long they are).
    pub hth: i32,
    pub vth: i32,
}

// Tuning constants (see docs/STATUS.md field-fixture section).
const LOCAL_RADIUS: usize = 24; // local-mean window radius
const DARK_RATIO: f32 = 0.85; // dark iff luma < ratio * local mean
const MIN_RUN: usize = 15; // min dark run length to seed a bar
/// White slits narrower than this are print damage, not module gaps.
const BRIDGE_GAP: usize = 2;
const MIN_STROKE: i32 = 32; // min stroke length in px (8-row DMRE leg at pitch 4)
const MAX_THICKNESS: i32 = 30; // max stroke thickness in px
const END_TOL: i32 = 8; // stroke-end / stroke-band pairing slack (px)
const DEDUP_RADIUS: f32 = 6.0; // corner merge distance (px)

/// Local-mean binarization: dark iff `luma < DARK_RATIO * mean` over a
/// square window of side `2 * LOCAL_RADIUS + 1`. The window must stay
/// well above the module pitch so a solid stroke never fills it.
pub fn binarize(frame: &Frame) -> BinaryMap {
    let (w, h) = (frame.w, frame.h);
    // integral image with a leading zero row/column
    let stride = w + 1;
    let mut ii = vec![0u32; stride * (h + 1)];
    for y in 0..h {
        let mut rowsum = 0u32;
        for x in 0..w {
            rowsum += frame.luma[y * w + x] as u32;
            ii[(y + 1) * stride + x + 1] = ii[y * stride + x + 1] + rowsum;
        }
    }
    let mut dark = vec![false; w * h];
    for y in 0..h {
        let y0 = y.saturating_sub(LOCAL_RADIUS);
        let y1 = (y + LOCAL_RADIUS + 1).min(h);
        for x in 0..w {
            let x0 = x.saturating_sub(LOCAL_RADIUS);
            let x1 = (x + LOCAL_RADIUS + 1).min(w);
            let s = ii[y1 * stride + x1] + ii[y0 * stride + x0]
                - ii[y0 * stride + x1]
                - ii[y1 * stride + x0];
            let n = ((x1 - x0) * (y1 - y0)) as f32;
            dark[y * w + x] = (frame.luma[y * w + x] as f32) < DARK_RATIO * (s as f32 / n);
        }
    }
    BinaryMap { w, h, dark }
}

#[derive(Clone, Copy, Debug)]
pub struct Bar {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32, // exclusive
}

fn dark_runs(at: impl Fn(usize) -> bool, n: usize) -> Vec<(usize, usize)> {
    let mut raw = Vec::new();
    let mut start = None;
    for i in 0..=n {
        let d = i < n && at(i);
        match (d, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                raw.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    // Bridge sub-module white slits before the length filter: a crease or
    // a print defect can cut a solid finder leg into fragments that each
    // fall under MIN_RUN (a real GS1 DataMatrix reported 3..24px pieces
    // of a ~100px leg, each pair separated by a 2px crease). The module
    // grid cannot bridge, because its white gaps are one pitch or more.
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (s, e) in raw {
        if let Some(last) = out.last_mut() {
            if s - last.1 <= BRIDGE_GAP {
                last.1 = e;
                continue;
            }
        }
        out.push((s, e));
    }
    out.retain(|(s, e)| e - s >= MIN_RUN);
    out
}

/// Cluster dark runs along scan lines into bars. `vertical` scans
/// per-column (bars tall in y); otherwise per-row. A run merges into a bar
/// that ended on the previous scan line when it spans the bar's last range
/// (axis-aligned strokes) or is of comparable length and overlaps it
/// substantially (strokes tilted in the frame).
fn collect_bars(map: &BinaryMap, vertical: bool) -> Vec<Bar> {
    // Scan lines run along the direction the bars stand: per-column for
    // vertical bars (w lines of h pixels), per-row for horizontal ones
    // (h lines of w pixels). (These were swapped once; every fixture was
    // square, so it hid until a non-square camera frame hit it.)
    let (lines, line_len) = if vertical { (map.w, map.h) } else { (map.h, map.w) };
    let at = |line: usize, i: usize| -> bool {
        if vertical {
            map.dark[i * map.w + line]
        } else {
            map.dark[line * map.w + i]
        }
    };
    // Bars tracked in scan coords (a = scan line index, b = along-line).
    struct Raw {
        a0: i32,
        b0: i32,
        a1: i32,
        b1: i32,
    }
    // Every bar still growing lives in `open`; a run may extend any bar
    // that ended on the previous line, not just the most recent one. The
    // single-cursor version closed the open bar on ANY non-matching run,
    // which shredded a tilted stroke: its run is the *last* dark run of
    // each column (data blobs precede it), so each column finalized the
    // previous fragment and started a new bar. The invoice DMRE's solid
    // finder border (~4° off-axis) thus reported vl=93 for a 146px leg,
    // ranking the true corner ~72nd and starving it of the budget
    // (2026-11-18 field corpus: 18s no-read on the phone, both modes).
    let mut open: Vec<Raw> = Vec::new();
    let mut bars: Vec<Raw> = Vec::new();
    for line in 0..lines {
        for (b0, b1) in dark_runs(|i| at(line, i), line_len) {
            let (b0, b1) = (b0 as i32, b1 as i32);
            let mut extended = false;
            for raw in open.iter_mut() {
                // Only bars whose last scan line is the previous one: two
                // runs on the same line are not one stroke.
                if raw.a1 != line as i32 {
                    continue;
                }
                // A merge must SPAN the bar's last-seen range AND not
                // extend it by more than the slack: data blobs sit strictly
                // inside the leg's range (span kills them), while edge
                // fragments — e.g. the perpendicular leg's own column in
                // this scan — sit at the range's end and would let a much
                // wider run "span" them once thickness grew the slack (the
                // extension bound kills that: a real leg row tracks its
                // range within a px or two; a fat bar's row does not). The
                // slack scales with the bar's own thickness — a leg is one
                // module thick, so the thickness tracks the pitch: enough
                // to absorb a keystoned frame's per-scan-line drift at any
                // scale. (span/10 was catastrophic at small pitch: 22px ~
                // 4 modules.)
                let th = raw.a1 - raw.a0;
                let slack = th.clamp(2, 8);
                let ext = th.clamp(3, 8);
                let spans = b0 <= raw.b0 + slack
                    && b1 >= raw.b1 - slack
                    && b0 >= raw.b0 - ext
                    && b1 <= raw.b1 + ext;
                // Tilted stroke: in a fixed scan line a stroke that drifts
                // along the line is cut into segments that each shift with
                // the line. The spans rule cannot see those (the new range
                // starts well before the old one), so admit a continuation
                // when the two ranges are of comparable length and overlap
                // substantially. The equal-length requirement is what
                // still keeps short data blobs from gluing themselves onto
                // a leg, and it cannot chain unrelated strokes either: the
                // overlap must cover half of the shorter range.
                let (len_new, len_old) = (b1 - b0, raw.b1 - raw.b0);
                let similar = len_new * 2 >= len_old && len_old * 2 >= len_new;
                let overlap = b1.min(raw.b1) - b0.max(raw.b0);
                let tilted = similar && overlap * 2 >= len_new.min(len_old);
                if spans || tilted {
                    raw.a0 = raw.a0.min(line as i32);
                    raw.b0 = raw.b0.min(b0);
                    raw.a1 = line as i32 + 1;
                    raw.b1 = raw.b1.max(b1);
                    extended = true;
                    break;
                }
            }
            if !extended {
                open.push(Raw { a0: line as i32, b0, a1: line as i32 + 1, b1 });
            }
        }
        // Bars that no run extended on this line are complete.
        let mut keep = Vec::with_capacity(open.len());
        for raw in open.drain(..) {
            if raw.a1 == line as i32 + 1 {
                keep.push(raw);
            } else {
                bars.push(raw);
            }
        }
        open = keep;
    }
    bars.extend(open);
    bars.into_iter()
        .map(|b| if vertical {
            Bar { x0: b.a0, y0: b.b0, x1: b.a1, y1: b.b1 }
        } else {
            Bar { x0: b.b0, y0: b.a0, x1: b.b1, y1: b.a1 }
        })
        .collect()
}

/// Debug access for scratch tests: raw bar extraction (pre-filter).
pub fn debug_bars(map: &BinaryMap, vertical: bool) -> Vec<Bar> {
    collect_bars(map, vertical)
}

/// Debug access for scratch tests: candidate corners.
pub fn debug_corners(map: &BinaryMap) -> Vec<Corner> {
    find_corners(map)
}


/// Join collinear fragments of the same stroke.
///
/// A crease, a highlight or a print defect can cut a solid finder leg into
/// pieces that each fall under [`MIN_STROKE`] (a real GS1 DataMatrix at
/// pitch 4.2 reported a 92px horizontal leg and a 29px vertical one, so no
/// corner formed at all). Fragments of one stroke share their cross-axis
/// band, match in thickness, and are separated along the stroke by less
/// than one module — the module grid cannot chain, because its dark gaps
/// are one pitch or more (pitch >= [`PITCH_MIN`]).
fn merge_collinear(bars: Vec<Bar>, vertical: bool) -> Vec<Bar> {
    let mut bars = bars;
    bars.sort_unstable_by_key(|b| if vertical { b.y0 } else { b.x0 });
    let mut out: Vec<Bar> = Vec::new();
    for bar in bars {
        let mut merged = false;
        for prev in out.iter_mut().rev() {
            let (a0, a1, b0, b1) = if vertical {
                (prev.y0, prev.y1, bar.y0, bar.y1)
            } else {
                (prev.x0, prev.x1, bar.x0, bar.x1)
            };
            let (c0, c1, d0, d1) = if vertical {
                (prev.x0, prev.x1, bar.x0, bar.x1)
            } else {
                (prev.y0, prev.y1, bar.y0, bar.y1)
            };
            // Same stroke band and thickness: the fragments overlap
            // across the stroke.
            let overlap = c1.min(d1) - c0.max(d0);
            let min_th = (c1 - c0).min(d1 - d0);
            if overlap * 2 < min_th || (c1 - c0 - (d1 - d0)).abs() > 1 {
                continue;
            }
            // Along the stroke: touching or separated by damage, never by
            // a module gap (one pitch).
            if b0.max(a0) - b1.min(a1) > 3 {
                continue;
            }
            if vertical {
                prev.y0 = prev.y0.min(bar.y0);
                prev.y1 = prev.y1.max(bar.y1);
                prev.x0 = prev.x0.min(bar.x0);
                prev.x1 = prev.x1.max(bar.x1);
            } else {
                prev.x0 = prev.x0.min(bar.x0);
                prev.x1 = prev.x1.max(bar.x1);
                prev.y0 = prev.y0.min(bar.y0);
                prev.y1 = prev.y1.max(bar.y1);
            }
            merged = true;
            break;
        }
        if !merged {
            out.push(bar);
        }
    }
    out
}

fn filtered_bars(map: &BinaryMap, vertical: bool) -> Vec<Bar> {
    merge_collinear(collect_bars(map, vertical), vertical)
        .into_iter()
        .filter(|b| {
            let (len, th) = if vertical {
                (b.y1 - b.y0, b.x1 - b.x0)
            } else {
                (b.x1 - b.x0, b.y1 - b.y0)
            };
            len >= MIN_STROKE && th <= MAX_THICKNESS && len >= 4 * th
        })
        .collect()
}

/// Detect finder-L corner candidates. Returns deduplicated corners; the
/// true symbol corner is expected among them, plus false positives from
/// text/graphics (rejected later by RS verification of sampled grids).
pub fn find_corners(map: &BinaryMap) -> Vec<Corner> {
    let hbars = filtered_bars(map, false);
    let vbars = filtered_bars(map, true);
    let mut raw: Vec<Corner> = Vec::new();
    for v in &vbars {
        for hb in &hbars {
            // each stroke has two ends; an L forms where one end of the
            // vertical stroke meets the horizontal stroke's band and one
            // end of the horizontal stroke meets the vertical's band
            for (vend, vkind) in [(v.y0, false), (v.y1, true)] {
                if vend < hb.y0 - END_TOL || vend > hb.y1 + END_TOL {
                    continue;
                }
                for (hend, hright) in [(hb.x0, false), (hb.x1, true)] {
                    if hend < v.x0 - END_TOL || hend > v.x1 + END_TOL {
                        continue;
                    }
                    let kind = match (vkind, hright) {
                        (true, true) => CornerKind::BottomRight,
                        (true, false) => CornerKind::BottomLeft,
                        (false, true) => CornerKind::TopRight,
                        (false, false) => CornerKind::TopLeft,
                    };
                    raw.push(Corner {
                        x: hend as f32,
                        y: vend as f32,
                        kind,
                        hlen: hb.x1 - hb.x0,
                        vlen: v.y1 - v.y0,
                        hth: hb.y1 - hb.y0,
                        vth: v.x1 - v.x0,
                    });
                }
            }
        }
    }
    // dedup: merge corners within DEDUP_RADIUS (tilted strokes yield
    // several fragment corners for the same L)
    let mut out: Vec<Corner> = Vec::new();
    for c in raw {
        match out.iter_mut().find(|e| {
            (e.x - c.x).abs() <= DEDUP_RADIUS && (e.y - c.y).abs() <= DEDUP_RADIUS
        }) {
            Some(e) => {
                e.x = e.x.min(c.x);
                e.y = e.y.min(c.y);
                e.hlen = e.hlen.max(c.hlen);
                e.vlen = e.vlen.max(c.vlen);
                // thickness too: keystone wedges shred a leg into fragment
                // bars of differing thickness, and the corner must carry
                // the thickest chained core or the fast path's thickness
                // seed underestimates the pitch
                e.hth = e.hth.max(c.hth);
                e.vth = e.vth.max(c.vth);
            }
            None => out.push(c),
        }
    }
    out
}

// ---- step B: corner -> decode -------------------------------------------

use crate::tables;
use crate::tables::SymbolSpec;

/// A located and decoded symbol.
#[derive(Clone, Debug)]
pub struct Location {
    /// The L corner (outer corner) in frame coordinates.
    pub corner: (f32, f32),
    pub kind: CornerKind,
    pub rows: usize,
    pub cols: usize,
    /// Module pitch in px.
    pub pitch: f32,
    /// In-plane content rotation in radians (searched, ~0 for flat scans).
    pub angle: f32,
    pub decoded: crate::Decoded,
}

/// Canonical axis directions for a corner kind: the image-space direction
/// (y down) in which canonical row / column indices increase, for an
/// untilted symbol, plus whether the horizontal stroke carries the rows
/// (else the cols). Derived from rotation symmetry; the BottomRight case
/// is pinned by the viva field fixture (docs/STATUS.md).
pub fn axes(kind: CornerKind) -> ((f32, f32), (f32, f32), bool) {
    match kind {
        CornerKind::BottomLeft => ((0.0, 1.0), (1.0, 0.0), false),
        CornerKind::BottomRight => ((1.0, 0.0), (0.0, -1.0), true),
        CornerKind::TopRight => ((0.0, -1.0), (-1.0, 0.0), false),
        CornerKind::TopLeft => ((-1.0, 0.0), (0.0, 1.0), true),
    }
}

// search grid parameters; RS correction absorbs sub-step residuals
const PITCH_MIN: f32 = 4.0;
const PITCH_MAX: f32 = 16.0;
const ANGLE_RANGE: f32 = 6.0; // degrees, symmetric: sign convention-free
const ANGLE_STEP: f32 = 1.0;
const OFFSET_RANGE: f32 = 3.0; // px around the coarse corner
const OFFSET_STEP: f32 = 1.5;
const L_MIN_FRACTION: f32 = 0.8; // finder L must be mostly dark

#[inline]
fn dark_at(map: &BinaryMap, x: f32, y: f32) -> bool {
    let xi = x.round() as isize;
    let yi = y.round() as isize;
    if xi < 0 || yi < 0 || xi >= map.w as isize || yi >= map.h as isize {
        return false;
    }
    map.dark[yi as usize * map.w + xi as usize]
}

/// A sampled module grid, optionally with per-module uncertainty.
struct GridSample {
    /// Module values, `r * cols + c`.
    modules: Vec<u8>,
    /// Non-zero where the module's own five samples disagreed. Empty when
    /// quality sampling was not requested.
    uncertain: Vec<u8>,
}

/// Quadrant offsets (in module units) for the quality samples: inside the
/// module at every supported pitch (4..16 px), far enough from the centre
/// that a smeared/merged module splits the vote.
const QUAD: [(f32, f32); 4] = [
    (-0.22, -0.22),
    (-0.22, 0.22),
    (0.22, -0.22),
    (0.22, 0.22),
];

/// Sample the canonical-ordered module matrix for one grid hypothesis.
/// Returns None when the finder L is not mostly dark (cheap reject before
/// the expensive decode).
///
/// With `quality`, four quadrant samples per module are taken as well: a
/// 2–2 split (or a centre that disagrees with a 3–1 majority) marks the
/// module uncertain. The caller turns uncertain modules into RS erasures
/// (item 6b) — on a hard binarized mask this vote is the only evidence
/// that a module's value is not trustworthy.
fn sample_grid_impl(
    map: &BinaryMap,
    kind: CornerKind,
    rows: usize,
    cols: usize,
    ox: f32,
    oy: f32,
    pitch: f32,
    theta: f32,
    quality: bool,
) -> Option<GridSample> {
    let (rd, cd, _) = axes(kind);
    let (ct, st) = (theta.cos(), theta.sin());
    let rot = |d: (f32, f32)| (d.0 * ct - d.1 * st, d.0 * st + d.1 * ct);
    let (rrx, rry) = rot(rd);
    let (crx, cry) = rot(cd);
    let pos = |r: f32, c: f32| -> (f32, f32) {
        let dr = r + 0.5 - rows as f32; // relative to the L corner row
        (
            ox + dr * pitch * rrx + (c + 0.5) * pitch * crx,
            oy + dr * pitch * rry + (c + 0.5) * pitch * cry,
        )
    };
    // finder L first: canonical col 0 + bottom row
    let mut l_dark = 0usize;
    let l_total = rows + cols;
    for r in 0..rows {
        let (x, y) = pos(r as f32, 0.0);
        l_dark += dark_at(map, x, y) as usize;
    }
    for c in 0..cols {
        let (x, y) = pos((rows - 1) as f32, c as f32);
        l_dark += dark_at(map, x, y) as usize;
    }
    if (l_dark as f32) < L_MIN_FRACTION * l_total as f32 {
        return None;
    }
    let mut m = vec![0u8; rows * cols];
    let mut u = vec![0u8; if quality { rows * cols } else { 0 }];
    for r in 0..rows {
        for c in 0..cols {
            let (x, y) = pos(r as f32, c as f32);
            let center = dark_at(map, x, y);
            m[r * cols + c] = center as u8;
            if quality {
                let mut votes = 0usize;
                for &(dr, dc) in &QUAD {
                    let (qx, qy) = pos(r as f32 + dr, c as f32 + dc);
                    votes += dark_at(map, qx, qy) as usize;
                }
                // 2-2 is a true tie; a centre that disagrees with a 3-1
                // majority is a misregistered/smeared module too.
                let majority = if votes >= 3 { true } else if votes <= 1 { false } else { center };
                u[r * cols + c] = ((votes == 2) || (majority != center)) as u8;
            }
        }
    }
    Some(GridSample { modules: m, uncertain: u })
}

fn sample_grid(
    map: &BinaryMap,
    kind: CornerKind,
    rows: usize,
    cols: usize,
    ox: f32,
    oy: f32,
    pitch: f32,
    theta: f32,
) -> Option<Vec<u8>> {
    sample_grid_impl(map, kind, rows, cols, ox, oy, pitch, theta, false).map(|s| s.modules)
}

/// Quality-sampled variant: modules plus the per-module uncertainty mask.
/// Costs four extra samples per module; only the reference locator's
/// erasure last resort pays for it (`try_corner_erasures`).
fn sample_grid_quality(
    map: &BinaryMap,
    kind: CornerKind,
    rows: usize,
    cols: usize,
    ox: f32,
    oy: f32,
    pitch: f32,
    theta: f32,
) -> Option<GridSample> {
    sample_grid_impl(map, kind, rows, cols, ox, oy, pitch, theta, true)
}

/// Symmetric search order: 0 first, then ±step, ±2·step, … ≤ range.
/// The corner estimate is already within a pixel or two and the in-plane
/// rotation is near zero in the common case, so a real symbol is reached
/// in the first attempts instead of after the whole negative half of the
/// grid (a cheap latency win on every DM search; coverage is unchanged).
fn centered(range: f32, step: f32) -> Vec<f32> {
    let mut out = vec![0.0f32];
    let mut v = step;
    while v <= range + 1e-9 {
        out.push(v);
        out.push(-v);
        v += step;
    }
    out
}

/// Try to decode a symbol for one corner candidate.
fn try_corner(map: &BinaryMap, corner: &Corner, budget: &mut u64) -> Option<Location> {
    try_corner_sweep(map, corner, budget, false)
}

/// The erasure last resort (item 6b): the same hypothesis sweep, sampling
/// each module's quadrant vote and passing modules whose samples disagreed
/// to Reed–Solomon as known erasures. Run only AFTER the plain sweep has
/// found nothing, so a strong plain hit can never be shadowed by an
/// erasure-corrected candidate at an earlier (wrong) hypothesis.
fn try_corner_erasures(map: &BinaryMap, corner: &Corner, budget: &mut u64) -> Option<Location> {
    try_corner_sweep(map, corner, budget, true)
}

fn try_corner_sweep(
    map: &BinaryMap,
    corner: &Corner,
    budget: &mut u64,
    erasure: bool,
) -> Option<Location> {
    let (rd, cd, rows_on_h) = axes(corner.kind);
    let _ = (rd, cd);
    let (hl, vl) = (corner.hlen as f32, corner.vlen as f32);
    // Candidate (spec, pitch) pairs, ordered by how well the corner's own
    // leg lengths predict the pitch (|hl/hdim - pitch| + |vl/vdim - pitch|):
    // a true corner hits within the first handful of attempts, where the
    // linear ladder used to burn the whole budget on large specs at small
    // pitches before ever reaching the true one. Same candidates, same
    // coverage — only the order changed.
    let mut cands: Vec<(f32, &'static SymbolSpec, f32)> = Vec::new();
    let mut pitch = PITCH_MIN;
    while pitch <= PITCH_MAX + 1e-9 {
        for spec in tables::SYMBOLS {
            let (rows, cols) = (spec.rows, spec.cols);
            let hdim = if rows_on_h { rows } else { cols } as f32;
            let vdim = if rows_on_h { cols } else { rows } as f32;
            // stroke fragments are lower bounds; bbox merge can inflate
            if hl < 0.5 * hdim * pitch || hl > hdim * pitch + 3.0 * pitch {
                continue;
            }
            if vl < 0.5 * vdim * pitch || vl > vdim * pitch + 3.0 * pitch {
                continue;
            }
            let err = (hl / hdim - pitch).abs() + (vl / vdim - pitch).abs();
            cands.push((err, spec, pitch));
        }
        pitch += 1.0;
    }
    cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, spec, pitch) in cands {
        let (rows, cols) = (spec.rows, spec.cols);
        // Quality sampling costs four extra samples per module.
        let cost = (rows * cols) as u64 * if erasure { 5 } else { 1 };
        for &deg in &centered(ANGLE_RANGE, ANGLE_STEP) {
            let theta = deg.to_radians();
            for &dx in &centered(OFFSET_RANGE, OFFSET_STEP) {
                for &dy in &centered(OFFSET_RANGE, OFFSET_STEP) {
                    // every grid attempt costs `cost` samples; the budget
                    // bounds a junk-rich frame to ~tens of ms
                    *budget = budget.saturating_sub(cost);
                    if *budget == 0 {
                        return None;
                    }
                    let decoded = if erasure {
                        sample_grid_quality(
                            map,
                            corner.kind,
                            rows,
                            cols,
                            corner.x + dx,
                            corner.y + dy,
                            pitch,
                            theta,
                        )
                        .and_then(|q| {
                            crate::decode_upright_with_erasures(
                                &q.modules,
                                rows,
                                cols,
                                &q.uncertain,
                            )
                        })
                    } else {
                        sample_grid(
                            map,
                            corner.kind,
                            rows,
                            cols,
                            corner.x + dx,
                            corner.y + dy,
                            pitch,
                            theta,
                        )
                        .and_then(|m| crate::decode_upright(&m, rows, cols))
                    };
                    if let Some(decoded) = decoded {
                        return Some(Location {
                            corner: (corner.x, corner.y),
                            kind: corner.kind,
                            rows,
                            cols,
                            pitch,
                            angle: theta,
                            decoded,
                        });
                    }
                }
            }
        }
    }
    None
}

/// Locate and decode all Data Matrix symbols in a grayscale frame.
pub fn locate(frame: &Frame) -> Vec<Location> {
    locate_binary(&binarize(frame))
}

/// Locate and decode on an already-binarized frame (e.g. the GPU
/// pipeline's mask — its Bradley rule matches [`binarize`]).
pub fn locate_binary(map: &BinaryMap) -> Vec<Location> {
    locate_binary_budget(map, SAMPLE_BUDGET * 6)
}

/// Reference locate with an explicit sample allowance.
pub fn locate_binary_budget(map: &BinaryMap, budget: u64) -> Vec<Location> {
    locate_binary_budget_corners(map, budget, usize::MAX)
}

/// Reference locate bounded to the `limit` best-ordered corners.
///
/// The sample allowance does not charge a corner's fixed costs (grid
/// setup, the L probe), and a junk-rich 1080p mask carries hundreds of
/// corners: one still frame measured 40s even though its counter stayed
/// inside the budget. Native-resolution stills therefore cap the corner
/// list; the downscale legs remain for symbols ranked beyond it.
pub fn locate_binary_budget_corners(map: &BinaryMap, budget: u64, limit: usize) -> Vec<Location> {
    let mut corners = find_corners(map);
    // Same ordering lesson as the fast path: consistency first (legs that
    // agree on a legal spec), leg length second. Ordering by raw leg
    // length lets long UI bands burn the whole allowance before the true
    // corner — one GS1 DataMatrix sat behind 59 corners and needed a 5s
    // budget where consistency-first finds it in ~200ms.
    corners.sort_by(|a, b| {
        let ra = best_spec_ratio(a);
        let rb = best_spec_ratio(b);
        ra.partial_cmp(&rb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then((b.hlen + b.vlen).cmp(&(a.hlen + a.vlen)))
    });
    let mut out: Vec<Location> = Vec::new();
    let mut budget = budget;
    for c in corners.iter().take(limit) {
        if budget == 0 {
            break;
        }
        let mut found = try_corner(map, c, &mut budget);
        // Erasure last resort (item 6b): the same sweep, but modules whose
        // quadrant samples disagreed become known erasures. Only after the
        // plain sweep missed, with at most half of what remains so later
        // corners keep a share of the allowance.
        if found.is_none() && budget > 0 {
            let allowance = budget / 2;
            let mut erasure_budget = allowance;
            found = try_corner_erasures(map, c, &mut erasure_budget);
            budget -= allowance - erasure_budget;
        }
        if let Some(loc) = found {
            let fresh = !out.iter().any(|l| {
                l.rows == loc.rows
                    && l.cols == loc.cols
                    && l.decoded.payload == loc.decoded.payload
            });
            if fresh {
                out.push(loc);
            }
        }
    }
    out
}

/// Module-sampling allowance per fast-locate call (~tens of ms): corners
/// are tried longest-legs-first, so the true symbol gets the first slice
/// and junk corners (text edges) only burn what remains.
const SAMPLE_BUDGET: u64 = 6_000_000;

/// Fast-path corner decode for box-downscaled masks: the legs measure the
/// symbol's own dimensions (hl \u{2248} hdim * pitch, vl \u{2248} vdim * pitch), so
/// their agreement picks the spec AND the pitch — no exhaustive sweep. A
/// stroke fragment deflates a leg and a bbox merge inflates it, so the
/// ladder spans a little beyond both estimates; specs are tried
/// tightest-aspect-fit first so a coincidental fit cannot burn the budget
/// before the right shape. The exhaustive [`try_corner`] remains the
/// reference path for native fixtures.
fn try_corner_fast(map: &BinaryMap, corner: &Corner, budget: &mut u64) -> Option<Location> {
    let (rd, cd, rows_on_h) = axes(corner.kind);
    let _ = (rd, cd);
    let (hl, vl) = (corner.hlen as f32, corner.vlen as f32);
    // Seed the pitch from the thicker bar: a finder leg is one module
    // thick, and even a raggedly-chained fragment knows its thickness.
    // Leg LENGTHS are unreliable on camera frames (keystone fragments
    // shorten them, merges inflate them) — treat them as loose bounds
    // only. Corners whose both bars are slivers cannot be legs at any
    // decodable pitch.
    let thick = corner.hth.max(corner.vth) as f32;
    // Thin-but-long legs: a printed module grid often binarizes to a 1px
    // finder L while its leg LENGTHS still agree within a few percent
    // (invoice DMRE: 282/36 = 7.83 vs 95/12 = 7.92, thickness 1px). The
    // old hard thickness gate rejected exactly those corners with
    // spent=0, even though the same symbol decoded from a tight crop whose
    // thickness measured 4-8px. Accept a thin corner when a symbol spec's
    // two leg pitches agree tightly and land in the searchable range.
    let thin = thick < 0.7 * PITCH_MIN;
    let (tlo, thi) = (0.6 * thick, 1.7 * thick);
    let mut fits: Vec<(f32, f32, f32, &tables::SymbolSpec, usize, usize, f32)> = Vec::new();
    for spec in tables::SYMBOLS {
        let (rows, cols) = (spec.rows, spec.cols);
        let hdim = if rows_on_h { rows } else { cols } as f32;
        let vdim = if rows_on_h { cols } else { rows } as f32;
        let (p_h, p_v) = (hl / hdim, vl / vdim);
        let ratio = p_h.max(p_v) / p_h.min(p_v);
        // Legs that agree pin the pitch better than the thickness does. A
        // stroke tilted in the frame is chained across scan lines into a
        // bbox whose thickness spans the drift (the invoice DMRE: 8px for
        // a 4px border at pitch 4.07), so the thickness floor would
        // exclude the very pitch the legs measured exactly.
        let tight = ratio <= 1.15 && p_h.min(p_v) >= PITCH_MIN && p_h.max(p_v) <= PITCH_MAX;
        if thin || tight {
            // Both legs must imply the same pitch, in the legal window.
            if !tight {
                continue;
            }
            // Legs are eroded by blur/thresholding, so they are lower
            // bounds: the longer implied pitch is the better estimate.
            let p = p_h.max(p_v);
            fits.push((ratio, p * 0.92, p * 1.08, spec, rows, cols, p));
            continue;
        }
        // Pitch window: thickness-derived. When the legs themselves are
        // long enough to imply a pitch consistent with it, intersect —
        // otherwise (fragments/merges) trust the thickness alone.
        let mut lo = tlo.max(PITCH_MIN);
        let mut hi = thi.min(PITCH_MAX);
        let leg_lo = 0.85 * p_h.min(p_v);
        let leg_hi = 1.15 * p_h.max(p_v);
        if leg_hi > leg_lo && p_h.min(p_v) >= 0.5 * tlo && p_h.max(p_v) <= 2.0 * thi {
            lo = lo.max(leg_lo);
            hi = hi.min(leg_hi);
        }
        if hi < lo {
            continue;
        }
        // Per-leg plausibility vs the window: a leg is a fragment
        // (lower bound) or a merge (upper bound).
        if hl < 0.4 * hdim * lo || hl > 1.8 * hdim * hi {
            continue;
        }
        if vl < 0.4 * vdim * lo || vl > 1.8 * vdim * hi {
            continue;
        }
        fits.push((ratio, lo, hi, spec, rows, cols, p_h.max(p_v)));
    }
    // Equal aspect ratios cannot separate 12x36 from 16x48: both are 3:1.
    // The finder leg is one module thick, so the spec whose implied pitch
    // agrees with the measured stroke thickness is the real one. Use the
    // SMALLER thickness: a tilted/keystoned stroke is chained across scan
    // lines and reports a bbox thickness spanning its drift, i.e. an
    // over-estimate; erosion only makes it thinner.
    let thick32 = corner.hth.min(corner.vth).max(1) as f32;
    fits.sort_by(|a, b| {
        let ka = (a.0, (a.6 - thick32).abs() / thick32);
        let kb = (b.0, (b.6 - thick32).abs() / thick32);
        ka.partial_cmp(&kb).unwrap()
    });
    for (_ratio, lo, hi, _spec, rows, cols, p_est) in fits {
        // Pitch ladder ordered by the leg-derived estimate, not bottom-up:
        // agreeing legs pin the pitch to a few percent (the wedge-legged
        // rig frame's true pitch sat ~9 steps above `lo` — bottom-up burned
        // full grid sweeps on every wrong pitch before reaching it, turning
        // a ~50ms hit into ~800ms). When the legs disagree (fragments) the
        // estimate is still the best center available.
        let step = 0.15f32.max((hi - lo) / 12.0);
        let mut pitches: Vec<f32> = Vec::new();
        let mut n = (lo / step).ceil();
        while n * step <= hi + 1e-9 {
            pitches.push(n * step);
            n += 1.0;
        }
        pitches.sort_by(|a, b| {
            (a - p_est).abs().partial_cmp(&(b - p_est).abs()).unwrap()
        });
        for pitch in pitches {
            // Probe the finder L near the corner (fragment bar ends sit a
            // few px off the true origin, so probe the offset range's
            // corners too): if the L is dark nowhere at this pitch, no
            // offset/angle will save it — skip the sweep. Junk corners
            // die here for a handful of samples per pitch.
            let mut probe_ok = false;
            'probe: for &pdx in &[-OFFSET_RANGE, 0.0, OFFSET_RANGE] {
                for &pdy in &[-OFFSET_RANGE, 0.0, OFFSET_RANGE] {
                    if sample_grid(map, corner.kind, rows, cols, corner.x + pdx, corner.y + pdy, pitch, 0.0).is_some() {
                        probe_ok = true;
                        break 'probe;
                    }
                }
            }
            if !probe_ok {
                continue;
            }
            for &deg in &centered(ANGLE_RANGE, ANGLE_STEP) {
                let theta = deg.to_radians();
                // Dashed-edge prefilter: the two edges opposite the solid L
                // are a checkerboard (module dark iff (row+col) even) — the
                // DM signature no UI band carries. Sampled along the theta-
                // rotated axes so tilted symbols are checked on their own
                // terms; junk corners die here for ~rows+cols samples
                // instead of grinding the offset sweep (which is ~4000x
                // pricier). Only applied when the legs agree with this
                // spec's dims — otherwise the edge positions are guesses
                // and a real symbol could be rejected (soft signal).
                let (ct, st) = (theta.cos(), theta.sin());
                let (rrx, rry) = (rd.0 * ct - rd.1 * st, rd.0 * st + rd.1 * ct);
                let (crx, cry) = (cd.0 * ct - cd.1 * st, cd.0 * st + cd.1 * ct);
                let (hd, vd) = if rows_on_h { (rows as f32, cols as f32) } else { (cols as f32, rows as f32) };
                if (hl - hd * pitch).abs() < 1.5 * pitch && (vl - vd * pitch).abs() < 1.5 * pitch {
                    // The corner is only a coarse estimate, so check the
                    // dashed edges at the same offsets the sweep will try.
                    // Using the raw corner rejected the correct hypothesis
                    // on the portrait field mask (a ~3px x shift was enough
                    // to break the 75% parity vote) before the offset loop
                    // ever ran.
                    let edges_ok = |ox: f32, oy: f32| -> bool {
                        let pos = |r: f32, c: f32| {
                            (
                                ox + (r + 0.5 - rows as f32) * pitch * rrx
                                    + (c + 0.5) * pitch * crx,
                                oy + (r + 0.5 - rows as f32) * pitch * rry
                                    + (c + 0.5) * pitch * cry,
                            )
                        };
                        let mut good = 0usize;
                        let mut total = 0usize;
                        for c in 0..cols {
                            let (px, py) = pos(0.0, c as f32);
                            if dark_at(map, px, py) == (c % 2 == 0) {
                                good += 1;
                            }
                            total += 1;
                        }
                        for r in 0..rows {
                            let (px, py) = pos(r as f32, (cols - 1) as f32);
                            if dark_at(map, px, py) == ((r + cols - 1) % 2 == 0) {
                                good += 1;
                            }
                            total += 1;
                        }
                        (good as f32) >= 0.75 * total as f32
                    };
                    let mut ok = false;
                    for &ex in &[-OFFSET_RANGE, 0.0, OFFSET_RANGE] {
                        for &ey in &[-OFFSET_RANGE, 0.0, OFFSET_RANGE] {
                            if edges_ok(corner.x + ex, corner.y + ey) {
                                ok = true;
                                break;
                            }
                        }
                        if ok {
                            break;
                        }
                    }
                    *budget = budget.saturating_sub(9 * (rows + cols) as u64);
                    if *budget == 0 {
                        return None;
                    }
                    if !ok {
                        continue;
                    }
                }
                for &dx in &centered(OFFSET_RANGE, OFFSET_STEP) {
                    for &dy in &centered(OFFSET_RANGE, OFFSET_STEP) {
                        *budget = budget.saturating_sub((rows * cols) as u64);
                        if *budget == 0 {
                            return None;
                        }
                        if let Some(m) = sample_grid(
                            map,
                            corner.kind,
                            rows,
                            cols,
                            corner.x + dx,
                            corner.y + dy,
                            pitch,
                            theta,
                        ) {
                            if let Some(decoded) = crate::decode_upright(&m, rows, cols) {
                                return Some(Location {
                                    corner: (corner.x, corner.y),
                                    kind: corner.kind,
                                    rows,
                                    cols,
                                    pitch,
                                    angle: theta,
                                    decoded,
                                });
                            }
                            // The 5x quality sample stays on the reference
                            // path (`try_corner_erasures`): on the fast
                            // path it can shadow a plain hit at a later
                            // hypothesis or eat the per-corner cap.
                        }
                    }
                }
            }
        }
    }
    None
}

/// Best leg-shape consistency over all specs: min(p_h/p_v). A real DM's
/// two legs imply the same pitch (ratio ~1); junk Ls (UI bands) only fit
/// some spec at wildly different pitches. Used to order corner attempts.
fn best_spec_ratio(c: &Corner) -> f32 {
    let (_, _, rows_on_h) = axes(c.kind);
    let (hl, vl) = (c.hlen as f32, c.vlen as f32);
    let mut best = f32::INFINITY;
    for spec in tables::SYMBOLS {
        let hdim = if rows_on_h { spec.rows } else { spec.cols } as f32;
        let vdim = if rows_on_h { spec.cols } else { spec.rows } as f32;
        let (p_h, p_v) = (hl / hdim, vl / vdim);
        // only specs whose implied pitch is in the decodable ballpark
        if p_h.max(p_v) > PITCH_MAX * 1.5 || p_h.min(p_v) < PITCH_MIN * 0.5 {
            continue;
        }
        let ratio = p_h.max(p_v) / p_h.min(p_v);
        if ratio < best {
            best = ratio;
        }
    }
    best
}

/// Fast locate for downscaled masks — same contract as [`locate_binary`].
pub fn locate_binary_fast(map: &BinaryMap) -> Vec<Location> {
    locate_binary_fast_budget(map, SAMPLE_BUDGET)
}

/// Fast locate with an explicit sample allowance.
pub fn locate_binary_fast_budget(map: &BinaryMap, budget: u64) -> Vec<Location> {
    let mut corners = find_corners(map);
    // Consistency first, leg length second: a wedged true corner's legs
    // still agree on the pitch, while junk UI bands only fit some spec at
    // mismatched pitches. (Pure leg-sum order let a 6M-sample UI band
    // starve the true corner sitting 8th.)
    corners.sort_by(|a, b| {
        let ra = best_spec_ratio(a);
        let rb = best_spec_ratio(b);
        ra.partial_cmp(&rb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then((b.hlen + b.vlen).cmp(&(a.hlen + a.vlen)))
    });
    let mut budget = budget;
    // Per-corner cap: no single junk corner (long-legged UI bands pass the
    // L-probe and grind full grid sweeps) may starve the true corner
    // sitting later in the order. A real corner — with the estimate-ordered
    // pitch ladder — hits within ~20K samples; junk gets cut off early.
    let per_corner = (SAMPLE_BUDGET / 8).max(200_000);
    let mut out: Vec<Location> = Vec::new();
    for c in &corners {
        if budget == 0 {
            break;
        }
        let mut cbudget = per_corner.min(budget);
        let before = cbudget;
        let found = try_corner_fast(map, c, &mut cbudget);
        budget -= before - cbudget;
        if found.is_some() {
            let loc = found.unwrap();
            let fresh = !out.iter().any(|l| {
                l.rows == loc.rows
                    && l.cols == loc.cols
                    && l.decoded.payload == loc.decoded.payload
            });
            if fresh {
                out.push(loc);
            }
        }
    }
    out
}

impl Location {
    /// The symbol's quadrilateral mapped into frame coordinates, ordered
    /// top-left, top-right, bottom-right, bottom-left in symbol space
    /// (the same convention as `barcode_core::Quad`).
    pub fn quad(&self) -> [(f32, f32); 4] {
        let (rd, cd, _) = axes(self.kind);
        let (ct, st) = (self.angle.cos(), self.angle.sin());
        let rot = |d: (f32, f32)| (d.0 * ct - d.1 * st, d.0 * st + d.1 * ct);
        let (rrx, rry) = rot(rd);
        let (crx, cry) = rot(cd);
        let at = |fr: f32, fc: f32| {
            (
                self.corner.0
                    + (fr - self.rows as f32) * self.pitch * rrx
                    + fc * self.pitch * crx,
                self.corner.1
                    + (fr - self.rows as f32) * self.pitch * rry
                    + fc * self.pitch * cry,
            )
        };
        [
            at(0.0, 0.0),
            at(0.0, self.cols as f32),
            at(self.rows as f32, self.cols as f32),
            at(self.rows as f32, 0.0),
        ]
    }
}
