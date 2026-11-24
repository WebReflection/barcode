//! Mode message decoding from the core ring (ISO/IEC 24778; see
//! docs/STATUS.md aztec A3/A4 blocks).
//!
//! The ring around the bullseye carries a fixed-pattern corner plus, on
//! each side, the mode message: 7 reference dashes per side (compact) or
//! 10 (full, split by the corner tick), read as 4-bit nibbles. The message
//! is `L−1` and `D−1` packed big-endian (2 nibbles compact: 2+6 bits;
//! 4 nibbles full: 5+11 bits) followed by 5 (compact) / 6 (full) GF(16)
//! check nibbles.

use crate::gf::field_for;
use crate::rs::rs_encode;
use crate::tables::Spec;

/// Decode the mode message. `matrix[r * cols + c]` nonzero = dark.
/// Returns `(layers, data_codewords)`, or None if the check nibbles
/// disagree (wrong size guess or damaged core).
pub fn read_mode(matrix: &[u8], spec: &Spec) -> Option<(u8, u16)> {
    let size = spec.rows as usize;
    let c = size / 2;
    let half = if spec.compact { 5 } else { 7 };
    let (lo, hi) = (c - half, c + half);

    // Core ring perimeter, clockwise from the top-left corner (probe
    // `read_mode`; indices into it select the message bits per side).
    let mut ring: Vec<(usize, usize)> = Vec::with_capacity(8 * half);
    for x in lo..=hi {
        ring.push((lo, x));
    }
    for y in lo + 1..=hi {
        ring.push((y, hi));
    }
    for x in (lo..hi).rev() {
        ring.push((hi, x));
    }
    for y in (lo + 1..hi).rev() {
        ring.push((y, lo));
    }

    // Message bit indices per side: compact skips the corner + 1 dash
    // (k+2..k+9); full skips the corner + 1, takes 5, skips the corner
    // tick, takes 5 more (k+2..k+7, k+8..k+13).
    let mut bits = Vec::new();
    for k in if spec.compact {
        [0usize, 10, 20, 30]
    } else {
        [0, 14, 28, 42]
    } {
        let idx: Box<dyn Iterator<Item = usize>> = if spec.compact {
            Box::new(k + 2..k + 9)
        } else {
            Box::new((k + 2..k + 7).chain(k + 8..k + 13))
        };
        for i in idx {
            let (r, cc) = ring[i];
            bits.push((matrix[r * size + cc] != 0) as u8);
        }
    }
    let nibs: Vec<u16> = bits
        .chunks_exact(4)
        .map(|ch| ch.iter().fold(0u16, |a, &b| (a << 1) | b as u16))
        .collect();

    let (lw, ncheck) = if spec.compact {
        (2usize, 5usize)
    } else {
        (4, 6)
    };
    let gf = field_for(4);
    let expect = rs_encode(&gf, &nibs[..lw], ncheck);
    if expect != nibs[lw..lw + ncheck] {
        return None;
    }

    let mut mw = 0u32;
    for &n in &nibs[..lw] {
        mw = (mw << 4) | n as u32;
    }
    let ld = if spec.compact { 6 } else { 11 };
    let layers = ((mw >> ld) as u8) + 1;
    let data_cw = ((mw & ((1 << ld) - 1)) as u16) + 1;
    // The detector's size guess must agree with the decoded layer count.
    (layers == spec.layers).then_some((layers, data_cw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::spec_for;

    /// '#' = dark. 'B' at vers 2 (19x19), zint oracle; its mode word must
    /// decode to L=2, D=1.
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
        "#.#.##.#####.#.#..#",
        "...#.#.......####..",
        ".##..##########....",
        "##.....#...#....#..",
        ".#...#..##..####...",
        "#.######..#.###.#.#",
        "#.##...#####..#...#",
        "..####..#.#.##..##.",
    ];

    fn matrix(rows: &[&str]) -> Vec<u8> {
        rows.iter()
            .flat_map(|r| r.bytes().map(|b| (b == b'#') as u8))
            .collect()
    }

    #[test]
    fn mode_of_zint_symbol() {
        let spec = spec_for(19, 19, true).unwrap();
        assert_eq!(read_mode(&matrix(B_V2), spec), Some((2, 1)));
    }

    #[test]
    fn corrupted_mode_rejected() {
        let spec = spec_for(19, 19, true).unwrap();
        let mut m = matrix(B_V2);
        // ring[2] = (4, 6): the second message bit of the top side
        m[4 * 19 + 6] ^= 1;
        assert_eq!(read_mode(&m, spec), None);
    }

    #[test]
    fn wrong_size_guess_rejected() {
        // L=2 decoded but the caller claimed vers 1 (L=1).
        let spec = spec_for(15, 15, true).unwrap();
        let m = matrix(B_V2);
        assert_eq!(read_mode(&m, spec), None);
    }
}
