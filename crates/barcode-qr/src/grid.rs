//! Milestone 2: sampling grid + format/version information (ISO/IEC 18004).
//!
//! The grid is an axis-aligned affine map: the top-left finder center sits
//! at module (3.5, 3.5) and the symbol spans `size` modules per side, so
//! the finder-to-finder span covers exactly `size - 7` modules. Perspective
//! correction arrives with the camera-path work; synthetic and
//! screen-scanned symbols are axis-aligned enough for this stage.
//!
//! Format info: a 15-bit BCH(15,5) word — 2 EC-level bits (MSBs), 3 mask
//! bits, 10 BCH bits — placed in two copies and XOR-masked with 0x5412.
//! A copy is accepted when its BCH remainder is zero.

use std::sync::OnceLock;

use barcode_core::BitImage;

use crate::Finder;

/// Quadrant offsets (in module units) for the quality samples: inside a
/// module at camera scales, far enough from the centre that a smeared or
/// merged module splits the vote.
const QUAD: [(f32, f32); 4] = [
    (-0.22, -0.22),
    (-0.22, 0.22),
    (0.22, -0.22),
    (0.22, 0.22),
];

/// EC level from the 2-bit format field (ISO 18004 §8.9: M=00, L=01, H=10, Q=11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcLevel {
    M,
    L,
    H,
    Q,
}

impl EcLevel {
    fn from_bits(bits: u8) -> Option<EcLevel> {
        Some(match bits {
            0b00 => EcLevel::M,
            0b01 => EcLevel::L,
            0b10 => EcLevel::H,
            0b11 => EcLevel::Q,
            _ => return None,
        })
    }
}

const FORMAT_GEN: u16 = 0x537; // x^10 + x^8 + x^5 + x^4 + x^2 + x + 1
const FORMAT_MASK: u16 = 0x5412; // 101010000010010

/// Polynomial remainder of a word under the generator (XOR-shift long
/// division — the binary-polynomial analog of integer mod). `gen` carries
/// the generator's high bit; `gen_bits` is its degree + 1.
fn poly_rem(word: u32, gen: u32, gen_bits: u32) -> u32 {
    let mut v = word;
    for bit in (gen_bits - 1..n_bits_of(word)).rev() {
        if v & (1 << bit) != 0 {
            v ^= gen << (bit - (gen_bits - 1));
        }
    }
    v & ((1 << (gen_bits - 1)) - 1)
}

fn n_bits_of(word: u32) -> u32 {
    32 - word.leading_zeros()
}

/// Syndrome -> error-pattern map for a BCH code: every n-bit error pattern
/// of weight <= max_errors, keyed by its remainder under the generator.
/// Built once per code (the two QR codes are known constants).
fn error_map(gen: u32, gen_bits: u32, n: u32, max_errors: u32) -> &'static std::collections::HashMap<u32, u32> {
    static FORMAT: OnceLock<std::collections::HashMap<u32, u32>> = OnceLock::new();
    static VERSION: OnceLock<std::collections::HashMap<u32, u32>> = OnceLock::new();
    let which = if gen == FORMAT_GEN as u32 { &FORMAT } else { &VERSION };
    which.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for pattern in 1u32..(1 << n) {
            if pattern.count_ones() as u32 > max_errors {
                continue;
            }
            let syndrome = poly_rem(pattern, gen, gen_bits);
            map.entry(syndrome).or_insert(pattern);
        }
        map
    })
}

/// Correct up to `max_errors` bit errors in an n-bit BCH word under `gen`.
/// Returns the corrected word, or None if the syndrome is not a known
/// correctable pattern (too much damage).
fn bch_correct(word: u32, gen: u32, gen_bits: u32, n: u32, max_errors: u32) -> Option<u32> {
    if n_bits_of(word) > n {
        return None;
    }
    let syndrome = poly_rem(word, gen, gen_bits);
    if syndrome == 0 {
        return Some(word);
    }
    let pattern = *error_map(gen, gen_bits, n, max_errors).get(&syndrome)?;
    Some(word ^ pattern)
}

/// Module coordinate of format bit k (0 = EC MSB) for copy 1, which wraps
/// the top-left finder. Layout per ISO 18004 §8.9 figure 25.
/// Format info positions in (x, y) = (column, row) — the SAME convention
/// as `Grid::sample` and the placement walk. Copy 1 wraps the top-left
/// finder: bits 14-8 along row 8, bits 7-0 up column 8. (An earlier
/// version stored these as (row, col); every non-90° rotation then read
/// a transposed word and died at the format check.)
pub const COPY1: [(u32, u32); 15] = [
    (0, 8), (1, 8), (2, 8), (3, 8), (4, 8), (5, 8), (7, 8), (8, 8), (8, 7), (8, 5), (8, 4),
    (8, 3), (8, 2), (8, 1), (8, 0),
];

/// Copy 2: bits 0-7 under the top-right finder, bits 8-14 right of the
/// bottom-left finder.
pub fn copy2(n: u32) -> [(u32, u32); 15] {
    let mut p = [(0u32, 0u32); 15];
    for (i, slot) in p.iter_mut().take(8).enumerate() {
        *slot = (8, n - 1 - i as u32);
    }
    for (i, slot) in p.iter_mut().skip(8).enumerate() {
        *slot = (n - 1 - i as u32, 8);
    }
    p
}

/// A module-coordinate to pixel mapping derived from the three finder
/// centers. The grid is a full AFFINE map — two step vectors, one per
/// symbol axis — so rotated and skewed symbols (box faces viewed at an
/// angle) are sampled correctly, not just axis-aligned ones.
///
/// When `h` is set the grid is instead a full PROJECTIVE map (perspective
/// path): sampling goes through the homography, whose coordinates are the
/// symbol's module space with the tl finder center at (3.5, 3.5) and one
/// module = 1 unit. The affine fields then hold a local linearization of
/// the same map (kept coherent for diagnostics and size computation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Modules per side (21..=177, always 17 + 4*version).
    pub size: u32,
    /// Image position of module (3.5, 3.5) — the tl finder's center.
    pub tl: (f32, f32),
    /// Image delta per +1 module along the symbol x axis.
    pub step_x: (f32, f32),
    /// Image delta per +1 module along the symbol y axis.
    pub step_y: (f32, f32),
    /// Projective module → pixel map, when perspective-corrected.
    pub h: Option<[f64; 9]>,
}

impl Grid {
    /// Derive the grid from the ordered (tl, tr, bl) finder centers — in
    /// SYMBOL space, i.e. the symbol's top-left/top-right/bottom-left
    /// corners, whichever image corners those happen to be. The symbol
    /// size follows from the finder spans and the finders' own module
    /// estimate; the step vectors carry rotation and skew.
    pub fn from_finders(tl: &Finder, tr: &Finder, bl: &Finder) -> Option<Grid> {
        let module = (tl.module + tr.module + bl.module) / 3.0;
        if module <= 0.0 {
            return None;
        }
        // Raw per-module steps from the finder spans (n - 7 modules each).
        let dx = (tr.x - tl.x, tr.y - tl.y);
        let dy = (bl.x - tl.x, bl.y - tl.y);
        let len_x = (dx.0 * dx.0 + dx.1 * dx.1).sqrt();
        let len_y = (dy.0 * dy.0 + dy.1 * dy.1).sqrt();
        if len_x < module * 4.0 || len_y < module * 4.0 {
            return None; // spans far too short for any real symbol
        }
        // The symbol must be (near) square: equal step lengths, and the
        // two axes near-perpendicular (skew up to ~±17° tolerated).
        if (len_x / len_y - 1.0).abs() > 0.25 {
            return None;
        }
        let dot = dx.0 * dy.0 + dx.1 * dy.1;
        if dot.abs() / (len_x * len_y) > 0.3 {
            return None;
        }
        // Symbol size from the pitch, snapped to a valid QR size.
        let n_raw = 7.0 + len_x / module;
        let size = 21 + 4 * ((((n_raw - 21.0) / 4.0).round() as i32).max(0) as u32);
        if size > 177 {
            return None;
        }
        let span = (size - 7) as f32;
        Some(Grid {
            size,
            tl: (tl.x, tl.y),
            step_x: (dx.0 / span, dx.1 / span),
            step_y: (dy.0 / span, dy.1 / span),
            h: None,
        })
    }

    /// A perspective-corrected grid: sampling goes through the homography
    /// (module space, tl finder center at (3.5, 3.5), 1 unit per module).
    /// The affine fields are filled with the map's local linearization at
    /// the tl finder center so diagnostics stay meaningful.
    pub fn from_homography(size: u32, h: [f64; 9]) -> Grid {
        let at = |u: f64, v: f64| barcode_core::geometry::apply_h(&h, u, v);
        let (tx, ty) = at(3.5, 3.5);
        let (xx, xy) = at(4.5, 3.5);
        let (yx, yy) = at(3.5, 4.5);
        Grid {
            size,
            tl: (tx as f32, ty as f32),
            step_x: ((xx - tx) as f32, (xy - ty) as f32),
            step_y: ((yx - tx) as f32, (yy - ty) as f32),
            h: Some(h),
        }
    }

    /// The symbol's four corners in image space, in result-quad order
    /// (TL, TR, BR, BL). Projective grids map the corners through H —
    /// exact under perspective; affine grids close the parallelogram from
    /// the step vectors.
    pub fn symbol_quad(&self) -> [(f32, f32); 4] {
        let n = self.size as f64;
        if let Some(h) = &self.h {
            let c = |u: f64, v: f64| {
                let (x, y) = barcode_core::geometry::apply_h(h, u, v);
                (x as f32, y as f32)
            };
            return [c(0.0, 0.0), c(n, 0.0), c(n, n), c(0.0, n)];
        }
        let corner = |u: f64, v: f64| {
            (
                self.tl.0 + ((u - 3.5) as f32) * self.step_x.0 + ((v - 3.5) as f32) * self.step_y.0,
                self.tl.1 + ((u - 3.5) as f32) * self.step_x.1 + ((v - 3.5) as f32) * self.step_y.1,
            )
        };
        [
            corner(0.0, 0.0),
            corner(n, 0.0),
            corner(n, n),
            corner(0.0, n),
        ]
    }

    /// Sample one module (dark?) at module coordinates (mx, my). Module
    /// `mx` spans [mx, mx+1) in module units, so its center is at mx+0.5 —
    /// i.e. (mx - 3) modules from the top-left finder center (3.5), along
    /// the symbol's own (possibly rotated/skewed) axes.
    pub fn sample(&self, img: &BitImage, mx: i32, my: i32) -> bool {
        self.sample_f(img, mx as f32 + 0.5, my as f32 + 0.5)
    }

    /// Sample at fractional module-space coordinates (the quadrant quality
    /// samples). Absolute module coords, not offsets from the finder.
    pub fn sample_f(&self, img: &BitImage, u: f32, v: f32) -> bool {
        let (fx, fy) = if let Some(h) = &self.h {
            barcode_core::geometry::apply_h(h, u as f64, v as f64)
        } else {
            (
                (self.tl.0 + (u - 3.5) * self.step_x.0 + (v - 3.5) * self.step_y.0) as f64,
                (self.tl.1 + (u - 3.5) * self.step_x.1 + (v - 3.5) * self.step_y.1) as f64,
            )
        };
        let px = fx.round() as i64;
        let py = fy.round() as i64;
        if px < 0 || py < 0 || px >= img.width as i64 || py >= img.height as i64 {
            return false;
        }
        img.ink(px as u32, py as u32)
    }

    /// Sample a module and its uncertainty: `(dark, uncertain)`. Four
    /// quadrant samples inside the module; a 2–2 tie, or a centre that
    /// disagrees with a 3–1 majority, means the value cannot be trusted —
    /// the caller turns it into a Reed–Solomon erasure (item 6d).
    pub fn sample_quality(&self, img: &BitImage, mx: i32, my: i32) -> (bool, bool) {
        let (u, v) = (mx as f32 + 0.5, my as f32 + 0.5);
        let center = self.sample_f(img, u, v);
        let mut votes = 0usize;
        for (du, dv) in QUAD {
            votes += self.sample_f(img, u + du, v + dv) as usize;
        }
        let majority = if votes >= 3 { true } else if votes <= 1 { false } else { center };
        (center, votes == 2 || majority != center)
    }

    /// Per-module uncertainty mask (`size * size`, `my * size + mx`).
    /// Costs four extra samples per module; only the erasure retry in
    /// [`crate::codewords::read_codewords`] calls it.
    pub fn uncertainty(&self, img: &BitImage) -> Vec<u8> {
        let n = self.size;
        let mut out = vec![0u8; (n * n) as usize];
        for my in 0..n {
            for mx in 0..n {
                out[(my * n + mx) as usize] =
                    self.sample_quality(img, mx as i32, my as i32).1 as u8;
            }
        }
        out
    }

    /// The 15-bit format word from one copy's positions, un-masked.
    pub fn format_word(&self, img: &BitImage, positions: &[(u32, u32); 15]) -> u16 {
        let mut word = 0u16;
        for (k, (mx, my)) in positions.iter().enumerate() {
            if self.sample(img, *mx as i32, *my as i32) {
                word |= 1 << (14 - k as u32);
            }
        }
        word ^ FORMAT_MASK
    }
}

/// Read the format information: try both copies; each is BCH-corrected
/// (up to 3 bit errors — perspective drift flips exactly those) before
/// extraction. Returns (EC level, mask pattern 0-7).
/// Read the format word: (EC level, mask, quality). Quality 2 = at least
/// one copy sampled with a zero syndrome (bit-exact); quality 1 = no clean
/// copy but both copies corrected and agreed (weaker — a mirrored grid's
/// transposed garbage can also land here, so callers should prefer
/// quality-2 labelings when several validate).
pub fn read_format_ranked(img: &BitImage, grid: &Grid) -> Option<(EcLevel, u8, u8)> {
    let w1 = grid.format_word(img, &COPY1);
    let w2 = grid.format_word(img, &copy2(grid.size));
    let clean = |w: u16| poly_rem(w as u32, FORMAT_GEN as u32, 11) == 0;
    let c1 = bch_correct(w1 as u32, FORMAT_GEN as u32, 11, 15, 3);
    let c2 = bch_correct(w2 as u32, FORMAT_GEN as u32, 11, 15, 3);
    // A clean copy wins outright. Otherwise both copies must correct AND
    // agree: a single "corrected" copy is trusted by ~56% of random
    // garbage (the syndrome map covers 576 of 1024 syndromes), but two
    // independent copies agreeing on garbage is rare.
    let (word, quality) = if clean(w1) {
        (w1 as u32, 2)
    } else if clean(w2) {
        (w2 as u32, 2)
    } else if let (Some(a), Some(b)) = (c1, c2) {
        if a == b { (a, 1) } else { return None; }
    } else {
        return None;
    };
    let word = word as u16;
    let ec = EcLevel::from_bits(((word >> 13) & 3) as u8)?;
    let mask = ((word >> 10) & 7) as u8;
    // NOTE: all four EC levels exist at every version 1-40 — there is no
    // version/EC validity coupling to exploit here (an earlier draft
    // invented one and it silently rejected real symbols).
    Some((ec, mask, quality))
}

pub fn read_format(img: &BitImage, grid: &Grid) -> Option<(EcLevel, u8)> {
    read_format_ranked(img, grid).map(|(ec, mask, _)| (ec, mask))
}

/// Read the version information (v7+ symbols carry it in two blocks: 6x3
/// beside the top-right finder, 3x6 beside the bottom-left finder). A block
/// is accepted when its BCH checks out AND its version matches the grid
/// geometry. Versions 1-6 have no version info → None.
pub fn read_version(img: &BitImage, grid: &Grid) -> Option<u32> {
    let n = grid.size;
    let expected = (n - 17) / 4;
    if expected < 7 {
        return None;
    }
    for block in 0..2 {
        // bit k of the word: top-right block at (row k/3, col n-11+k%3),
        // bottom-left at (row n-11+k%3, col k/3).
        let mut word = 0u32;
        for k in 0..18u32 {
            let (mx, my) = if block == 0 {
                (n - 11 + k % 3, k / 3)
            } else {
                (k / 3, n - 11 + k % 3)
            };
            if grid.sample(img, mx as i32, my as i32) {
                word |= 1 << k;
            }
        }
        let corrected = bch_correct(word, 0x1F25, 13, 18, 3)?;
        let version = (corrected >> 12) as u32;
        if version == expected {
            return Some(version);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use barcode_fixtures::Canvas;

    fn image_of(canvas: &Canvas) -> BitImage<'_> {
        BitImage::new(canvas.width, canvas.height, canvas.bits())
    }

    fn grid_of(canvas: &Canvas) -> Grid {
        let img = image_of(canvas);
        let mut d = crate::QrDecoder::new();
        let finders = d.find_finders(&img).expect("finders not found");
        // Finders come back UNLABELED. Prefer the labeling whose codewords
        // actually correct — a permissive BCH corrector can validate a
        // mirrored labeling's garbage, and only Reed-Solomon is a reliable
        // judge. Among format-valid labelings with no RS verdict (pattern-
        // only canvases), prefer the highest format confidence: a clean
        // bit-exact copy (2) beats corrected agreement (1) — mirrored
        // grids never sample a clean copy of a real symbol.
        let mut fallback: Option<(Grid, u8)> = None;
        for perm in crate::LABELINGS {
            let [tl, tr, bl] =
                [finders[perm[0]], finders[perm[1]], finders[perm[2]]];
            let Some(grid) = Grid::from_finders(&tl, &tr, &bl) else { continue };
            let Some((ec, mask, quality)) = read_format_ranked(&img, &grid) else { continue };
            if crate::codewords::read_codewords(&img, &grid, ec, mask).is_some() {
                return grid;
            }
            if fallback.map_or(true, |(_, q)| quality > q) {
                fallback = Some((grid, quality));
            }
        }
        fallback.expect("no valid labeling").0
    }

    #[test]
    fn reads_every_ec_and_mask_combination() {
        for ec in 0u8..4 {
            for mask in 0u8..8 {
                let canvas = barcode_fixtures::encode_qr_pattern(1, ec, mask);
                let grid = grid_of(&canvas);
                assert_eq!(grid.size, 21, "v1 size");
                let got = read_format(&image_of(&canvas), &grid);
                let want = (EcLevel::from_bits(ec).unwrap(), mask);
                assert_eq!(got, Some(want), "ec={ec} mask={mask}");
            }
        }
    }

    #[test]
    fn sizes_a_version_2_symbol() {
        let canvas = barcode_fixtures::encode_qr_pattern(2, 1, 5);
        let grid = grid_of(&canvas);
        assert_eq!(grid.size, 25, "v2 size");
        assert_eq!(read_format(&image_of(&canvas), &grid), Some((EcLevel::L, 5)));
    }

    #[test]
    fn recovers_format_via_second_copy() {
        let mut canvas = barcode_fixtures::encode_qr_pattern(1, 1, 3);
        // Wipe copy 1 (its modules around the top-left finder) entirely.
        let m = 4u32;
        let quiet = 6 * m;
        for (mx, my) in COPY1 {
            let x = quiet + mx * m;
            let y = quiet + my * m;
            canvas.clear(x, y, m, m);
        }
        let grid = grid_of(&canvas);
        assert_eq!(read_format(&image_of(&canvas), &grid), Some((EcLevel::L, 3)));
    }

    #[test]
    fn corrects_a_few_corrupt_format_bits() {
        // BCH(15,5) corrects up to 3 errors per copy: one flipped module
        // per copy must still come back.
        let mut canvas = barcode_fixtures::encode_qr_pattern(1, 1, 3);
        let m = 4u32;
        let quiet = 6 * m;
        let (x, y) = (quiet + 8 * m, quiet + 0 * m); // copy 1, bit 0
        canvas.flip(x + 2, y + 2);
        let (x, y) = (quiet + (21 - 1) * m, quiet + 8 * m); // copy 2, bit 0
        canvas.flip(x + 2, y + 2);
        let grid = grid_of(&canvas);
        assert_eq!(
            read_format(&image_of(&canvas), &grid),
            Some((EcLevel::L, 3)),
            "1 flipped bit per copy is corrected"
        );
    }

    #[test]
    fn rejects_when_both_copies_are_corrupt() {
        // 4+ bit errors per copy exceed the correction capacity.
        let mut canvas = barcode_fixtures::encode_qr_pattern(1, 1, 3);
        let m = 4u32;
        let quiet = 6 * m;
        for k in 0..4usize {
            let (mx, my) = COPY1[k];
            for (dx, dy) in [(1u32, 1u32), (1, 2), (2, 1), (2, 2)] {
                canvas.flip(quiet + mx * m + dx, quiet + my * m + dy);
            }
            let (mx, my) = copy2(21)[k];
            for (dx, dy) in [(1u32, 1u32), (1, 2), (2, 1), (2, 2)] {
                canvas.flip(quiet + mx * m + dx, quiet + my * m + dy);
            }
        }
        // The true v1 grid, built from geometry (grid_of needs a valid
        // format read, which is exactly what this corruption defeats).
        let grid = Grid { size: 21, tl: (38.0, 38.0), step_x: (4.0, 0.0), step_y: (0.0, 4.0), h: None };
        assert_eq!(read_format(&image_of(&canvas), &grid), None);
    }

    #[test]
    fn dark_module_is_inplace() {
        let canvas = barcode_fixtures::encode_qr_pattern(1, 0, 0);
        let grid = grid_of(&canvas);
        assert!(grid.sample(&image_of(&canvas), 8, 13), "dark module (8, n-8)");
    }
}
