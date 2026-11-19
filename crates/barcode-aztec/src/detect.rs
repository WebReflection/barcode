//! Bullseye detection: locate Aztec symbols in binarized frames.
//!
//! Clean-room; geometry from the spec summary (docs/STATUS.md aztec
//! blocks) and verified against zint oracle matrices: the bullseye is a
//! 9x9 (compact) or 13x13 (full) block of concentric square rings, dark
//! at even Chebyshev radius and white at odd. Pipeline (v0: clean, near-
//! axis-aligned frames; tilt/perspective is later work):
//!
//! 1. Row scan — a bullseye cross-section is a dark-white-...-dark
//!    palindrome of module-wide runs. The outermost dark runs may merge
//!    with adjacent dark mode/data cells, so only the inner runs must be
//!    equal-length. Yields (x, module scale, compact/full) candidates.
//! 2. Column verify — the column through a candidate crosses the same
//!    palindrome; its core run confirms the structure and pins the true
//!    center y (the row scan only sees the core's y-band).
//! 3. Ring-grid verify — the bullseye grid must match the ring pattern
//!    in its outer rings (loose) and exactly in the central 5x5. The
//!    exact core check is essential: the ring pattern is invariant under
//!    ±2-module shifts, and cross-section sub-matches land on exactly
//!    such aliases. Over-reporting is fine — mode-message RS arbitration
//!    in [`crate::read_stream`] is the final word (same philosophy as
//!    the DM locator).
//!
//! Input is an unpacked binarized frame, `dark[y * w + x]` ∈ {0, 1} —
//! the wasm bridge unpacks `BitImage` exactly as it does for DM.

/// One detected bullseye: center in pixel coordinates plus module scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bullseye {
    pub x: f32,
    pub y: f32,
    /// Pixels per module.
    pub scale: f32,
    /// True for a 13x13 bullseye (full Aztec), false for 9x9 (compact).
    /// A hint only: a full bullseye contains valid compact cross-sections,
    /// so the pipeline should try both specs and let `read_mode` decide.
    pub full: bool,
}

const MIN_SCALE: f32 = 3.0;
const MAX_SCALE: f32 = 96.0;
/// Inner-run length tolerance, relative to the core run.
const RUN_TOL: f32 = 0.25;
/// The outermost dark run may merge with dark mode/data cells; this caps
/// the merge so junk can't masquerade as a ring.
const MERGE_CAP: f32 = 4.0;
/// Fraction of outer-ring cells that must match the ring pattern.
const OUTER_MIN: f32 = 0.92;

struct Run {
    start: usize,
    len: usize,
    dark: bool,
}

fn runs_of(profile: &[u8]) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut i = 0;
    while i < profile.len() {
        let dark = profile[i] != 0;
        let mut j = i + 1;
        while j < profile.len() && (profile[j] != 0) == dark {
            j += 1;
        }
        runs.push(Run { start: i, len: j - i, dark });
        i = j;
    }
    runs
}

/// Cross-section match with the middle (core) run at index `m`:
/// `half` = 4 -> 9 runs (compact), 6 -> 13 runs (full). Ring parity must
/// alternate perfectly; inner runs must be module-wide; the outermost
/// dark runs may be longer (mode/data merge). Returns (core center x,
/// module scale) for a horizontal profile.
fn cross_match(runs: &[Run], m: usize, half: usize) -> Option<(f32, f32)> {
    if m < half || m + half >= runs.len() || !runs[m].dark {
        return None;
    }
    let s = runs[m].len as f32;
    if !(MIN_SCALE..=MAX_SCALE).contains(&s) {
        return None;
    }
    check_rings(runs, m, half, s)?;
    Some((runs[m].start as f32 + s / 2.0, s))
}

/// Ring parity/length check shared by the row and column passes.
fn check_rings(runs: &[Run], m: usize, half: usize, s: f32) -> Option<()> {
    for o in 1..=half {
        let want_dark = o % 2 == 0;
        for &side in &[-1i32, 1] {
            let r = &runs[(m as i32 + side * o as i32) as usize];
            if r.dark != want_dark {
                return None;
            }
            let l = r.len as f32;
            // Outermost ring: dark data may merge into it (compact symbols
            // have data hard against the bullseye — a 5-module run is
            // ordinary; first seen on a 19x19 with a solid data column).
            // Full-range keeps the tight cap: its reference grid bounds
            // merges. Rings o=1..=half-1 stay exact (RUN_TOL).
            let cap = if half == 4 { 8.0 } else { MERGE_CAP };
            let ok = if o < half {
                (l - s).abs() <= RUN_TOL * s
            } else {
                // Runs are integer pixels; the estimated scale is the core
                // run's integer length. At a fractional module pitch (e.g.
                // 8.8 px) the outer ring can be one pixel shorter than the
                // core — a 35-vs-36 outer run rejected a clean symbol at
                // the live 1056px mask while 1088px decoded. One pixel of
                // slack is rounding, not a structure relaxation; the exact
                // 5x5 core and the mode message still arbitrate.
                l + 1.0 >= s && l <= cap * s
            };
            if !ok {
                return None;
            }
        }
    }
    Some(())
}

pub fn find_bullseyes(dark: &[u8], w: usize, h: usize) -> Vec<Bullseye> {
    if w == 0 || h == 0 || dark.len() < w * h {
        return Vec::new();
    }
    let mut cands: Vec<Bullseye> = Vec::new();
    for y in 0..h {
        let runs = runs_of(&dark[y * w..y * w + w]);
        for m in 0..runs.len() {
            // Full first so its cross-sections are classified as full;
            // compact sub-matches of a full bullseye fail the exact core
            // check anyway, and dedup prefers `full` at a shared center.
            for (half, full) in [(6usize, true), (4, false)] {
                if let Some((x, s)) = cross_match(&runs, m, half) {
                    if let Some(c) = column_verify(dark, w, h, x, y as f32, s, full) {
                        if grid_verify(dark, w, h, &c) {
                            cands.push(c);
                        }
                    }
                }
            }
        }
    }
    dedup(cands)
}

/// The column through `x` must cross the same palindrome; its core run
/// (the dark run containing `y_hint`) pins the true center y.
fn column_verify(
    dark: &[u8],
    w: usize,
    h: usize,
    x: f32,
    y_hint: f32,
    s: f32,
    full: bool,
) -> Option<Bullseye> {
    let half = if full { 6 } else { 4 };
    let xi = x.round() as usize;
    if xi >= w {
        return None;
    }
    let col: Vec<u8> = (0..h).map(|yy| dark[yy * w + xi]).collect();
    let runs = runs_of(&col);
    let yi = (y_hint.round() as usize).min(h.saturating_sub(1));
    let m = runs
        .iter()
        .position(|r| r.dark && r.start <= yi && yi < r.start + r.len)?;
    if m < half || m + half >= runs.len() {
        return None;
    }
    let core = &runs[m];
    // The core is bounded by the r=1 white ring, so it can never merge.
    if (core.len as f32 - s).abs() > RUN_TOL * s {
        return None;
    }
    check_rings(&runs, m, half, s)?;
    Some(Bullseye {
        x,
        y: core.start as f32 + core.len as f32 / 2.0,
        scale: s,
        full,
    })
}

fn sample(dark: &[u8], w: usize, h: usize, x: f32, y: f32) -> u8 {
    let xi = x.round() as isize;
    let yi = y.round() as isize;
    if xi < 0 || yi < 0 || xi >= w as isize || yi >= h as isize {
        return 0;
    }
    dark[yi as usize * w + xi as usize]
}

/// Outer rings loose, central 5x5 exact. A shifted grid center (the ±2
/// module alias) mismatches exactly the cells adjacent to the core and
/// fails the exact check; junk around a real pattern fails the outer
/// score (e.g. a compact symbol read as full scores ~0.74 < OUTER_MIN).
fn grid_verify(dark: &[u8], w: usize, h: usize, b: &Bullseye) -> bool {
    let half: i32 = if b.full { 6 } else { 4 };
    let ring_ok = |i: i32, j: i32| -> bool {
        let want = (i.abs().max(j.abs())) % 2 == 0;
        (sample(dark, w, h, b.x + i as f32 * b.scale, b.y + j as f32 * b.scale) != 0) == want
    };
    let total = (2 * half + 1) * (2 * half + 1);
    let hit = (-half..=half).map(|i| (-half..=half).filter(|&j| ring_ok(i, j)).count() as i32).sum::<i32>();
    if (hit as f32) < OUTER_MIN * total as f32 {
        return false;
    }
    (-2..=2).all(|i| (-2..=2).all(|j| ring_ok(i, j)))
}

/// One bullseye is seen on many scanlines; merge candidates that share a
/// center, preferring the full-core classification.
fn dedup(mut cands: Vec<Bullseye>) -> Vec<Bullseye> {
    cands.sort_by(|a, b| {
        b.full
            .cmp(&a.full)
            .then(b.x.total_cmp(&a.x))
            .then(b.y.total_cmp(&a.y))
    });
    let mut kept: Vec<Bullseye> = Vec::new();
    for c in cands {
        let near = kept.iter().any(|k| {
            let s = k.scale.min(c.scale);
            ((k.x - c.x).abs().hypot(k.y - c.y)) < 1.5 * s
        });
        if !near {
            kept.push(c);
        }
    }
    kept
}

/// Symbol orientation in the frame, determined by the corner marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Orientation {
    /// Quarter turns (screen coords, y down) from symbol frame to frame.
    pub rotation: u8,
    pub mirrored: bool,
}

/// Maps symbol-frame module offsets `(dx, dy)` from the core center to
/// frame pixels: mirror, then rotate, then scale and translate.
#[derive(Clone, Copy)]
pub struct FrameMap {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub orient: Orientation,
}

/// Quadrant offsets (in module units) for the quality samples: inside the
/// module at camera scales, far enough from the centre that a smeared or
/// merged module splits the vote.
const QUAD: [(f32, f32); 4] = [
    (-0.22, -0.22),
    (-0.22, 0.22),
    (0.22, -0.22),
    (0.22, 0.22),
];

impl FrameMap {
    pub fn pixel(&self, dx: i32, dy: i32) -> (f32, f32) {
        self.pixel_f(dx as f32, dy as f32)
    }

    /// Module offsets may be fractional (the quadrant quality samples).
    pub fn pixel_f(&self, dx: f32, dy: f32) -> (f32, f32) {
        let (mut u, v) = (dx, dy);
        if self.orient.mirrored {
            u = -u;
        }
        let (u, v) = match self.orient.rotation {
            0 => (u, v),
            1 => (-v, u),
            2 => (-u, -v),
            _ => (v, -u),
        };
        (self.x + u * self.scale, self.y + v * self.scale)
    }

    /// Sample the frame at a symbol-module offset (true = dark).
    pub fn module(&self, dark: &[u8], w: usize, ht: usize, dx: i32, dy: i32) -> bool {
        let (px, py) = self.pixel(dx, dy);
        sample(dark, w, ht, px, py) != 0
    }

    /// Sample a module and its uncertainty: `(dark, uncertain)`.
    /// Four quadrant samples; a 2–2 tie, or a centre that disagrees with a
    /// 3–1 majority, means the module's value cannot be trusted — the
    /// caller turns it into a Reed–Solomon erasure (item 6c).
    pub fn module_quality(
        &self,
        dark: &[u8],
        w: usize,
        ht: usize,
        dx: i32,
        dy: i32,
    ) -> (bool, bool) {
        let (px, py) = self.pixel(dx, dy);
        let center = sample(dark, w, ht, px, py) != 0;
        let mut votes = 0usize;
        for &(ox, oy) in &QUAD {
            let (qx, qy) = self.pixel_f(dx as f32 + ox, dy as f32 + oy);
            votes += (sample(dark, w, ht, qx, qy) != 0) as usize;
        }
        let majority = if votes >= 3 { true } else if votes <= 1 { false } else { center };
        (center, votes == 2 || majority != center)
    }

    fn bit(&self, dark: &[u8], w: usize, ht: usize, dx: i32, dy: i32) -> bool {
        self.module(dark, w, ht, dx, dy)
    }
}

/// Core half-width in modules: 11x11 compact core, 15x15 full core.
pub fn core_half(full: bool) -> i32 {
    if full {
        7
    } else {
        5
    }
}

/// Orientation from the core corner marks. Each core corner owns a
/// 3-cell L — the side cell toward the previous corner (cycle
/// TL->TR->BR->BL), the corner cell, the side cell toward the next —
/// dark in a zint-verified canonical pattern per corner (counts
/// 3,2,1,0 clockwise; "decoding begins at the corner with three black
/// pixels"). Counts alone cannot separate rotation from reflection, so
/// the full triple arrangement is matched: exactly one of the 8
/// transforms reproduces the canon.
fn corner_canon(half: i32) -> [([[i32; 2]; 3], [bool; 3]); 4] {
    // [prev, corner, next] module offsets from the core center, per
    // corner, plus the expected dark pattern.
    [
        ([[-half, -half + 1], [-half, -half], [-half + 1, -half]], [true, true, true]), // TL
        ([[half - 1, -half], [half, -half], [half, -half + 1]], [false, true, true]),   // TR
        ([[half, half - 1], [half, half], [half - 1, half]], [true, false, false]),     // BR
        ([[-half + 1, half], [-half, half], [-half, half - 1]], [false, false, false]), // BL
    ]
}

/// `half` = [`core_half`]; a wrong guess finds no orientation, so the
/// compact/full question is self-validating here too.
pub fn find_orientation(
    dark: &[u8],
    w: usize,
    ht: usize,
    b: &Bullseye,
    half: i32,
) -> Option<Orientation> {
    for mirrored in [false, true] {
        for rotation in 0..4u8 {
            let map = FrameMap {
                x: b.x,
                y: b.y,
                scale: b.scale,
                orient: Orientation { rotation, mirrored },
            };
            let ok = corner_canon(half).iter().all(|(cells, want)| {
                cells
                    .iter()
                    .zip(want)
                    .all(|(c, &want_bit)| map.bit(dark, w, ht, c[0], c[1]) == want_bit)
            });
            if ok {
                return Some(Orientation { rotation, mirrored });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(rows: &[&str]) -> Vec<u8> {
        rows.iter()
            .flat_map(|r| r.bytes().map(|b| (b == b'#') as u8))
            .collect()
    }

    /// Upscale a symbol matrix into a synthetic binarized frame.
    fn render(matrix: &[u8], size: usize, scale: usize, quiet: usize) -> Vec<u8> {
        let dim = (size + 2 * quiet) * scale;
        let mut f = vec![0u8; dim * dim];
        for r in 0..size {
            for c in 0..size {
                if matrix[r * size + c] == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let y = (r + quiet) * scale + dy;
                        let x = (c + quiet) * scale + dx;
                        f[y * dim + x] = 1;
                    }
                }
            }
        }
        f
    }

    /// A fractional module pitch quantizes each run independently: the
    /// outer ring can be one pixel shorter than the core. That is exactly
    /// the live 1056px inverted-Aztec miss (clean mask, 35px outer ring,
    /// 36px core); the detector must accept it, while still rejecting a
    /// genuinely short run.
    #[test]
    fn outer_ring_tolerates_one_pixel_rounding() {
        let runs = |lens: &[usize]| -> Vec<Run> {
            lens.iter()
                .enumerate()
                .map(|(i, &len)| Run { start: i * 40, len, dark: i % 2 == 0 })
                .collect()
        };
        let nine = [35, 35, 35, 35, 36, 35, 35, 35, 35];
        assert!(check_rings(&runs(&nine), 4, 4, 36.0).is_some());
        let two_short = [35, 35, 35, 35, 36, 35, 35, 35, 33];
        assert!(check_rings(&runs(&two_short), 4, 4, 36.0).is_none());
    }

    fn rot90(matrix: &[u8], size: usize) -> Vec<u8> {
        let mut out = vec![0u8; size * size];
        for r in 0..size {
            for c in 0..size {
                out[c * size + (size - 1 - r)] = matrix[r * size + c];
            }
        }
        out
    }

    // Frozen zint oracles (tests/zint_golden.rs; tools/aztec_probe.py for
    // the full symbol; '#' = dark = 1).
    const B_V2: &[&str] = &[
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
    const HELLO_V1: &[&str] = &[
        "..##...##....#.",
        "##.##.##..#..##",
        "#.##....#...##.",
        "..###########.#",
        "#.##.......##..",
        ".#.#.#####.####",
        "...#.#...#.##..",
        "####.#.#.#.#..#",
        "..##.#...#.#...",
        "##.#.#####.#...",
        ".#.#.......#.#.",
        ".#.###########.",
        "#....##...#...#",
        "#....#....#.#.#",
        "#....##.#...##.",
    ];
    // 'FULL23' full Aztec, 23x23, zint option_2=6.
    const FULL23: &[&str] = &[
        "...##.###....##.##...##",
        "##......######.###.#.##",
        "##..#..#..#.....#..##..",
        ".##.##..#..##.#..####.#",
        "#..###....#.......#.###",
        ".####################..",
        "#..###...........#..#..",
        "#....#.#########.#...#.",
        "##.###.#.......#.#..###",
        ".#...#.#.#####.#.####..",
        "##..##.#.#...#.#.#.#...",
        ".#.#.#.#.#.#.#.#.#.#.#.",
        "##...#.#.#...#.#.######",
        "....##.#.#####.#.##..##",
        "#..###.#.......#.#..##.",
        ".....#.#########.##.###",
        "#.##.#...........##..##",
        "##...###############.##",
        "##....#........##......",
        ".....#..#..#..##.#.#.##",
        ".#.##...###.#..#....##.",
        "##.#.####..#...##.###..",
        "...#.######.##.##..#.#.",
    ];

    fn assert_one(frame: &[u8], w: usize, h: usize, cx: f32, cy: f32, scale: f32, full: bool) {
        let found = find_bullseyes(frame, w, h);
        assert_eq!(found.len(), 1, "expected one bullseye, got {found:?}");
        let b = found[0];
        assert!((b.x - cx).abs() < 1.5, "x {} vs {cx}", b.x);
        assert!((b.y - cy).abs() < 1.5, "y {} vs {cy}", b.y);
        assert!((b.scale - scale).abs() < 0.5, "scale {} vs {scale}", b.scale);
        assert_eq!(b.full, full);
    }

    #[test]
    fn finds_compact_bullseye() {
        // 19x19 compact: bullseye center module (9,9) -> (4+9.5)*8 = 108.
        let f = render(&m(B_V2), 19, 8, 4);
        let dim = (19 + 8) * 8;
        assert_one(&f, dim, dim, 108.0, 108.0, 8.0, false);
    }

    #[test]
    fn finds_compact_bullseye_other_scale() {
        // 15x15 compact at scale 12, quiet 3 -> (3+7.5)*12 = 126.
        let f = render(&m(HELLO_V1), 15, 12, 3);
        let dim = (15 + 6) * 12;
        assert_one(&f, dim, dim, 126.0, 126.0, 12.0, false);
    }

    #[test]
    fn finds_full_bullseye() {
        // 23x23 full: bullseye center module (11,11) -> (5+11.5)*10 = 165.
        let f = render(&m(FULL23), 23, 10, 5);
        let dim = (23 + 10) * 10;
        assert_one(&f, dim, dim, 165.0, 165.0, 10.0, true);
    }

    #[test]
    fn finds_rotated_symbol() {
        let f = render(&rot90(&m(B_V2), 19), 19, 8, 4);
        let dim = (19 + 8) * 8;
        assert_one(&f, dim, dim, 108.0, 108.0, 8.0, false);
    }

    #[test]
    fn rejects_subscale_and_blank() {
        let f = render(&m(B_V2), 19, 2, 4); // below MIN_SCALE
        assert!(find_bullseyes(&f, f.len() / ((19 + 8) * 2), (19 + 8) * 2).is_empty());
        assert!(find_bullseyes(&vec![0u8; 400 * 300], 400, 300).is_empty());
    }

    fn orient_of(matrix: &[u8], size: usize, scale: usize, quiet: usize, full: bool) -> Orientation {
        let f = render(matrix, size, scale, quiet);
        let dim = (size + 2 * quiet) * scale;
        let found = find_bullseyes(&f, dim, dim);
        assert_eq!(found.len(), 1);
        let b = found[0];
        assert_eq!(b.full, full);
        find_orientation(&f, dim, dim, &b, core_half(full)).expect("orientation")
    }

    /// Apply mirror-then-rotate to a square matrix.
    fn transform(matrix: &[u8], size: usize, rot: u8, mirrored: bool) -> Vec<u8> {
        let mut m = matrix.to_vec();
        if mirrored {
            let mut out = vec![0u8; size * size];
            for r in 0..size {
                for c in 0..size {
                    out[r * size + c] = m[r * size + (size - 1 - c)];
                }
            }
            m = out;
        }
        for _ in 0..rot {
            m = rot90(&m, size);
        }
        m
    }

    #[test]
    fn orientation_identity_all_core_sizes() {
        assert_eq!(orient_of(&m(B_V2), 19, 8, 4, false), Orientation { rotation: 0, mirrored: false });
        assert_eq!(orient_of(&m(HELLO_V1), 15, 12, 3, false), Orientation { rotation: 0, mirrored: false });
        assert_eq!(orient_of(&m(FULL23), 23, 10, 5, true), Orientation { rotation: 0, mirrored: false });
    }

    #[test]
    fn orientation_all_eight_transforms() {
        // Every rotation+mirror of a symbol must be reported as exactly
        // that transform. The bullseye center is transform-invariant
        // (odd size, centered), so the frame geometry stays comparable.
        for mirrored in [false, true] {
            for rot in 0..4u8 {
                let mat = transform(&m(B_V2), 19, rot, mirrored);
                let o = orient_of(&mat, 19, 8, 4, false);
                assert_eq!(
                    o,
                    Orientation { rotation: rot, mirrored },
                    "rot {rot} mirrored {mirrored}"
                );
            }
        }
    }

    #[test]
    fn orientation_rejects_wrong_core_half() {
        let f = render(&m(B_V2), 19, 8, 4);
        let dim = (19 + 8) * 8;
        let b = &find_bullseyes(&f, dim, dim)[0];
        assert_eq!(find_orientation(&f, dim, dim, b, core_half(true)), None);
        let ff = render(&m(FULL23), 23, 10, 5);
        let fdim = (23 + 10) * 10;
        let fb = &find_bullseyes(&ff, fdim, fdim)[0];
        assert_eq!(find_orientation(&ff, fdim, fdim, fb, core_half(false)), None);
    }

    // ---- frame pipeline (crate::read_frame): these tests live here
    // because the oracle fixtures and render helpers are defined above.

    #[test]
    fn read_frame_decodes_oracle_frames() {
        for (rows, size, scale, quiet, _full, want) in [
            (B_V2, 19usize, 8usize, 4usize, false, &b"B"[..]),
            (HELLO_V1, 15, 12, 3, false, b"hello world"),
            (FULL23, 23, 10, 5, true, b"FULL23"),
        ] {
            let f = render(&m(rows), size, scale, quiet);
            let dim = (size + 2 * quiet) * scale;
            let hits = crate::read_frame(&f, dim, dim);
            assert_eq!(hits.len(), 1, "size {size}");
            assert_eq!(crate::text::decode_text(&hits[0].stream), want, "size {size}");
            // TL corner module center sits half a module into the quiet zone.
            let expect = (quiet as f32 + 0.5) * scale as f32;
            assert!(
                (hits[0].quad[0] - expect).abs() < 1.0 && (hits[0].quad[1] - expect).abs() < 1.0,
                "quad {:?} vs {expect}",
                &hits[0].quad[..2]
            );
        }
    }

    #[test]
    fn read_frame_decodes_all_transforms() {
        for mirrored in [false, true] {
            for rot in 0..4u8 {
                let mat = transform(&m(HELLO_V1), 15, rot, mirrored);
                let f = render(&mat, 15, 10, 3);
                let dim = (15 + 6) * 10;
                let hits = crate::read_frame(&f, dim, dim);
                assert_eq!(hits.len(), 1, "rot {rot} mirrored {mirrored}");
                assert_eq!(crate::text::decode_text(&hits[0].stream), b"hello world");
            }
        }
    }

    #[test]
    fn read_frame_blank_frame_is_empty() {
        assert!(crate::read_frame(&vec![0u8; 300 * 200], 300, 200).is_empty());
    }
}
