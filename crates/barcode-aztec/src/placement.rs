//! Codeword placement: the data stream spirals counterclockwise around the
//! core in a 2-bit-thick layer, outermost ring first (ISO/IEC 24778; see
//! docs/STATUS.md aztec A4 blocks).
//!
//! Per ring (bounds `lo..=hi` in the unshifted frame): left strip downward,
//! bottom strip rightward, right strip upward, top strip leftward. Each
//! strip owns its starting corner, dominoes run perpendicular to the travel
//! direction with the OUTER cell first, and the path ends just above the
//! core's top-left corner. Full symbols carry a reference grid every 16th
//! row/column (final coordinates); those cells render as fixed dashes and
//! are skipped, while the remaining cells are pushed outward — the probe
//! (`tools/aztec_placement_probe.py`) verified `path.len() == cap_bits()`
//! for all 36 sizes.
//!
//! Bit convention: stream bit 1 = dark module (spec: "1 bits are printed as
//! black squares"), so `read_bits` copies the binarized matrix directly.

use crate::tables::Spec;

/// Visit order of every stream-bit cell: `cells[bit_index] = (row, col)`.
/// The length equals `spec.cap_bits()`; grid cells (full symbols) are
/// skipped, so the returned coordinates are final symbol coordinates.
pub fn data_path(spec: &Spec) -> Vec<(u16, u16)> {
    let layers = spec.layers as usize;
    let size = spec.rows as usize;
    let u_size = spec.core() as usize + 4 * layers; // unshifted frame side
    let u_c = u_size / 2;
    let c = size / 2;

    // Reference-grid line coordinates (final coords), center excluded.
    let inserts: Vec<usize> = if spec.compact {
        vec![]
    } else {
        let start = c % 16;
        (0..)
            .map(move |k| start + 16 * k)
            .take_while(|&v| v < size)
            .filter(|&v| v != c)
            .collect()
    };

    // Map an unshifted coordinate to its final position: coordinates at or
    // after each inserted grid line shift outward by one.
    let m = |u: usize| -> usize {
        let mut s = u;
        for &ins in &inserts {
            if s >= ins {
                s += 1;
            }
        }
        s
    };
    // Grid cells render as dashes and carry no stream bit.
    let keep =
        |y: usize, x: usize| -> bool { spec.compact || (y % 16 != c % 16 && x % 16 != c % 16) };

    let mut cells = Vec::with_capacity(spec.cap_bits() as usize);
    let push = |cells: &mut Vec<(u16, u16)>, y: usize, x: usize| {
        let (fy, fx) = (m(y), m(x));
        if keep(fy, fx) {
            cells.push((fy as u16, fx as u16));
        }
    };

    for i in 0..layers {
        let half = u_c - 2 * i;
        let lo = u_c - half;
        let hi = u_c + half;
        for y in lo..hi - 1 {
            for x in [lo, lo + 1] {
                push(&mut cells, y, x); // left strip, top -> bottom
            }
        }
        for x in lo..hi - 1 {
            for y in [hi, hi - 1] {
                push(&mut cells, y, x); // bottom strip, left -> right
            }
        }
        for y in (lo + 2..=hi).rev() {
            for x in [hi, hi - 1] {
                push(&mut cells, y, x); // right strip, bottom -> top
            }
        }
        for x in (lo + 2..=hi).rev() {
            for y in [lo, lo + 1] {
                push(&mut cells, y, x); // top strip, right -> left
            }
        }
    }
    debug_assert_eq!(cells.len() as u32, spec.cap_bits(), "vers {}", spec.vers);
    cells
}

/// Read the stream bits along the placement path. `matrix[row * cols + col]`
/// nonzero = dark = bit 1. Returns exactly `spec.cap_bits()` bits.
pub fn read_bits(matrix: &[u8], spec: &Spec) -> Vec<u8> {
    let cols = spec.cols as usize;
    data_path(spec)
        .into_iter()
        .map(|(r, c)| (matrix[r as usize * cols + c as usize] != 0) as u8)
        .collect()
}

/// `n` little-end-ordered b-bit windows starting at bit `start`:
/// `out[k] = bits[start + k*b .. start + (k+1)*b]` MSB-first.
pub fn windows(bits: &[u8], start: usize, n: usize, b: usize) -> Vec<u16> {
    (0..n)
        .map(|k| {
            let mut v = 0u16;
            for &bit in &bits[start + k * b..start + (k + 1) * b] {
                v = (v << 1) | bit as u16;
            }
            v
        })
        .collect()
}

/// Parse `n_cw` stuffed data codewords from `bits[start:]` (probe
/// `parse_data`, RS-validated against zint). A codeword is its first b−1
/// bits plus one more; when those b−1 bits are all equal — and it is not
/// the final codeword, which carries the padding raw — the next bit is a
/// complementary stuff bit and is skipped. Returns the values and the end
/// position, or None if the stream is too short or a stuff bit is missing.
pub fn parse_stuffed(
    bits: &[u8],
    start: usize,
    b: usize,
    n_cw: usize,
) -> Option<(Vec<u16>, usize)> {
    parse_stuffed_impl(bits, None, start, b, n_cw).map(|(v, p, _)| (v, p))
}

/// As [`parse_stuffed`], also reporting which codewords consumed an
/// uncertain bit (`uncertain` is aligned with `bits`). Stuffing makes the
/// bit→codeword map position-dependent, so this is computed during the
/// parse rather than from fixed windows.
pub fn parse_stuffed_erasures(
    bits: &[u8],
    uncertain: &[u8],
    start: usize,
    b: usize,
    n_cw: usize,
) -> Option<(Vec<u16>, usize, Vec<usize>)> {
    parse_stuffed_impl(bits, Some(uncertain), start, b, n_cw)
}

fn parse_stuffed_impl(
    bits: &[u8],
    uncertain: Option<&[u8]>,
    start: usize,
    b: usize,
    n_cw: usize,
) -> Option<(Vec<u16>, usize, Vec<usize>)> {
    let mut pos = start;
    let mut out = Vec::with_capacity(n_cw);
    let mut erased: Vec<usize> = Vec::new();
    for i in 0..n_cw {
        if pos + b > bits.len() {
            return None;
        }
        let from = pos;
        let run = &bits[pos..pos + b - 1];
        pos += b - 1;
        let stuffed = i + 1 < n_cw && run.iter().all(|&x| x == run[0]);
        if stuffed {
            if pos >= bits.len() || bits[pos] == run[0] {
                return None; // stuff bit must be complementary
            }
            pos += 1;
        }
        if pos >= bits.len() {
            return None;
        }
        let mut v = 0u16;
        for &bit in run {
            v = (v << 1) | bit as u16;
        }
        v = (v << 1) | bits[pos] as u16;
        pos += 1;
        if let Some(unc) = uncertain {
            if unc[from..pos].iter().any(|&u| u != 0) {
                erased.push(i);
            }
        }
        out.push(v);
    }
    Some((out, pos, erased))
}

/// Indices of the `n` b-bit windows starting at `start` whose bits include
/// an uncertain one. `uncertain` is aligned with `bits`.
pub fn window_erasures(uncertain: &[u8], start: usize, n: usize, b: usize) -> Vec<usize> {
    if uncertain.is_empty() {
        return Vec::new();
    }
    (0..n)
        .filter(|&k| {
            uncertain[start + k * b..start + (k + 1) * b]
                .iter()
                .any(|&u| u != 0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::{all_specs, spec_for};

    /// GOLDEN: path length == cap_bits for every size, and every cell is a
    /// distinct in-bounds coordinate (probe-verified for all 36 sizes).
    #[test]
    fn path_covers_cap_bits_distinct_cells() {
        for spec in all_specs() {
            let path = data_path(spec);
            assert_eq!(path.len() as u32, spec.cap_bits(), "vers {}", spec.vers);
            let mut seen = vec![false; spec.rows as usize * spec.cols as usize];
            for &(r, c) in &path {
                assert!(r < spec.rows && c < spec.cols, "vers {} oob", spec.vers);
                assert!(!seen[r as usize * spec.cols as usize + c as usize]);
                seen[r as usize * spec.cols as usize + c as usize] = true;
            }
        }
    }

    /// The spiral starts at the outer top-left and ends one row below,
    /// two columns right of it ("directly above the core's top-left
    /// corner", reading toward the core).
    #[test]
    fn spiral_start_and_end() {
        let s = spec_for(19, 19, true).unwrap();
        let p = data_path(s);
        assert_eq!(p[0], (0, 0));
        assert_eq!(p[1], (0, 1));
        // ends just above the innermost ring's top-left corner
        assert_eq!(&p[p.len() - 2..], &[(2, 4), (3, 4)]);

        // Full symbol: grid rows/cols are skipped, outer cells pushed out.
        let f = spec_for(37, 37, false).unwrap(); // first size with grid
        let p = data_path(f);
        assert_eq!(p[0], (0, 0));
        // grid lines at final coords 2 and 34 (18 = center, part of core)
        assert!(p
            .iter()
            .all(|&(r, c)| ![2, 18, 34].contains(&r) && ![2, 18, 34].contains(&c)));
    }

    /// Reference grid: full symbols skip every 16th row/col (mod center).
    #[test]
    fn grid_cells_excluded() {
        let f = spec_for(49, 49, false).unwrap();
        let c = 49 / 2;
        let path = data_path(f);
        for &(r, ck) in &path {
            let on_grid = (r as usize % 16 == c % 16 && r as usize != c)
                || (ck as usize % 16 == c % 16 && ck as usize != c);
            assert!(!on_grid, "grid cell ({r},{ck}) in path");
        }
    }

    /// Window extraction is MSB-first per codeword.
    #[test]
    fn windows_msb_first() {
        let bits = [0, 0, 0, 1, 0, 1, 1, 1, 1, 0, 0, 0];
        assert_eq!(windows(&bits, 0, 2, 6), vec![0b000101, 0b111000]);
    }

    /// Stuffed parse: the probe-verified 'x\\x7f' vers 1 stream parses to
    /// its true values (see tests/zint_golden.rs for the full matrix).
    #[test]
    fn parse_stuffed_end_to_end() {
        let spec = spec_for(15, 15, true).unwrap();
        let matrix = golden_matrix_x7f();
        let bits = read_bits(&matrix, spec);
        let (vals, pos) = parse_stuffed(&bits, 2, 6, 4).unwrap();
        assert_eq!(vals, vec![57, 39, 46, 62]);
        assert_eq!(pos, 104 - 13 * 6); // lands exactly on the check region
    }

    // '#' = dark. 'x\x7f' at vers 1, zint oracle (see docs/STATUS.md).
    fn golden_matrix_x7f() -> Vec<u8> {
        let rows = [
            "..##..#..###.##",
            "#####.#..#.####",
            "#.##......#.#..",
            ".#############.",
            "#.##.......##..",
            ".###.#####.###.",
            "##.#.#...#.##.#",
            "#..#.#.#.#.##.#",
            "##.#.#...#.#..#",
            "#.##.#####.###.",
            "####.......####",
            "##.###########.",
            "#.....#.#....##",
            "#.####..#.###..",
            "#....#.#.#.##.#",
        ];
        rows.iter()
            .flat_map(|r| r.bytes().map(|b| (b == b'#') as u8))
            .collect()
    }
}
