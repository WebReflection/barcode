//! Codeword placement (ISO/IEC 16022 annex M): the diagonal sweep that
//! assigns each successive bit of the codeword stream to a module of the
//! concatenated mapping matrix.
//!
//! The sweep walks upward/downward diagonals in 8-module "utah" corner
//! shapes, with four special corner cases and the wrap rule
//! `col += 4 − ((ncol+4) mod 8)` / `row += 4 − ((nrow+4) mod 8)` when a
//! shape runs off the left/top edge. It applies to the CONCATENATED
//! mapping matrix (data regions stacked, separators removed) — see
//! tables.rs. Validated against libdmtx for all 30 sizes.

use crate::tables::SymbolSpec;

/// The 8-module corner shape relative to its anchor, in placement order
/// (the anchor itself receives the last bit of the group).
const UTAH: [(isize, isize); 8] = [
    (-2, -2),
    (-2, -1),
    (-1, -2),
    (-1, -1),
    (-1, 0),
    (0, -2),
    (0, -1),
    (0, 0),
];

/// Visit order of every mapping-matrix cell for a symbol spec:
/// `positions[bit_index] = (mapping_row, mapping_col)`.
///
/// Every mapping cell is visited exactly once; for sizes whose cell count
/// is not a multiple of 8 (12x12, 16x16, 20x20, 24x24) the last few
/// visited cells carry no stream bit and render as the fixed pattern.
pub fn mapping_positions(spec: &SymbolSpec) -> Vec<(u16, u16)> {
    let nrow = spec.map_rows();
    let ncol = spec.map_cols();
    // Wrap shifts from the annex M formula (can be negative: 4−((n+4)%8)
    // takes values in {−2, 0, 2, 4}).
    let shift_r = 4isize - ((nrow + 4) % 8) as isize; // col shift when running off the top
    let shift_c = 4isize - ((ncol + 4) % 8) as isize; // row shift when running off the left

    let mut assigned = vec![false; nrow * ncol];
    let mut positions = Vec::with_capacity(nrow * ncol);

    #[inline]
    fn place(
        assigned: &mut [bool],
        positions: &mut Vec<(u16, u16)>,
        nrow: usize,
        ncol: usize,
        shift_r: isize,
        shift_c: isize,
        row: isize,
        col: isize,
    ) {
        let mut row = row;
        let mut col = col;
        if row < 0 {
            row += nrow as isize;
            col += shift_r;
        }
        if col < 0 {
            col += ncol as isize;
            row += shift_c;
        }
        debug_assert!((0..nrow as isize).contains(&row) && (0..ncol as isize).contains(&col));
        assigned[row as usize * ncol + col as usize] = true;
        positions.push((row as u16, col as u16));
    }

    macro_rules! group {
        ($row:expr, $col:expr) => {
            for &(dr, dc) in UTAH.iter() {
                place(
                    &mut assigned,
                    &mut positions,
                    nrow,
                    ncol,
                    shift_r,
                    shift_c,
                    $row + dr,
                    $col + dc,
                );
            }
        };
    }

    // The four special corner cases, each an 8-module group.
    macro_rules! corner1 {
        () => {
            for &(r, c) in [
                (nrow as isize - 1, 0),
                (nrow as isize - 1, 1),
                (nrow as isize - 1, 2),
                (0, ncol as isize - 2),
                (0, ncol as isize - 1),
                (1, ncol as isize - 1),
                (2, ncol as isize - 1),
                (3, ncol as isize - 1),
            ]
            .iter()
            {
                place(&mut assigned, &mut positions, nrow, ncol, shift_r, shift_c, r, c);
            }
        };
    }
    macro_rules! corner2 {
        () => {
            for &(r, c) in [
                (nrow as isize - 3, 0),
                (nrow as isize - 2, 0),
                (nrow as isize - 1, 0),
                (0, ncol as isize - 4),
                (0, ncol as isize - 3),
                (0, ncol as isize - 2),
                (0, ncol as isize - 1),
                (1, ncol as isize - 1),
            ]
            .iter()
            {
                place(&mut assigned, &mut positions, nrow, ncol, shift_r, shift_c, r, c);
            }
        };
    }
    macro_rules! corner3 {
        () => {
            for &(r, c) in [
                (nrow as isize - 3, 0),
                (nrow as isize - 2, 0),
                (nrow as isize - 1, 0),
                (0, ncol as isize - 2),
                (0, ncol as isize - 1),
                (1, ncol as isize - 1),
                (2, ncol as isize - 1),
                (3, ncol as isize - 1),
            ]
            .iter()
            {
                place(&mut assigned, &mut positions, nrow, ncol, shift_r, shift_c, r, c);
            }
        };
    }
    macro_rules! corner4 {
        () => {
            for &(r, c) in [
                (nrow as isize - 1, 0),
                (nrow as isize - 1, ncol as isize - 1),
                (0, ncol as isize - 3),
                (0, ncol as isize - 2),
                (0, ncol as isize - 1),
                (1, ncol as isize - 3),
                (1, ncol as isize - 2),
                (1, ncol as isize - 1),
            ]
            .iter()
            {
                place(&mut assigned, &mut positions, nrow, ncol, shift_r, shift_c, r, c);
            }
        };
    }

    let mut row: isize = 4;
    let mut col: isize = 0;
    loop {
        if row == nrow as isize && col == 0 {
            corner1!();
        }
        if row == nrow as isize - 2 && col == 0 && ncol % 4 != 0 {
            corner2!();
        }
        if row == nrow as isize - 2 && col == 0 && ncol % 8 == 4 {
            corner3!();
        }
        if row == nrow as isize + 4 && col == 2 && ncol % 8 == 0 {
            corner4!();
        }
        // sweep upward diagonally
        loop {
            if row < nrow as isize && col >= 0 && !assigned[row as usize * ncol + col as usize] {
                group!(row, col);
            }
            row -= 2;
            col += 2;
            if !(row >= 0 && col < ncol as isize) {
                break;
            }
        }
        row += 1;
        col += 3;
        // sweep downward diagonally
        loop {
            if row >= 0 && col < ncol as isize && !assigned[row as usize * ncol + col as usize] {
                group!(row, col);
            }
            row += 2;
            col -= 2;
            if !(row < nrow as isize && col >= 0) {
                break;
            }
        }
        row += 3;
        col += 1;
        if !(row < nrow as isize || col < ncol as isize) {
            break;
        }
    }

    debug_assert_eq!(
        positions.len(),
        8 * spec.codewords(),
        "sweep must visit exactly the stream-bit cells"
    );
    positions
}

/// Symbol-space visit order: `positions[bit_index] = (symbol_row, symbol_col)`.
pub fn stream_positions(spec: &SymbolSpec) -> Vec<(u16, u16)> {
    mapping_positions(spec)
        .into_iter()
        .map(|(r, c)| {
            let (sr, sc) = spec.to_symbol(r as usize, c as usize);
            (sr as u16, sc as u16)
        })
        .collect()
}

/// Read the codeword stream from a binarized symbol matrix
/// (`matrix[row * cols + col]` = dark). Returns `n_codewords` bytes,
/// MSB-first per codeword. The caller supplies the symbol's orientation —
/// the matrix must be upright (L finder left+bottom, clock top+right).
pub fn read_codewords(
    matrix: &[u8],
    _rows: usize,
    cols: usize,
    spec: &SymbolSpec,
    n_codewords: usize,
) -> Vec<u8> {
    let positions = stream_positions(spec);
    debug_assert!(positions.len() >= n_codewords * 8);
    let mut out = Vec::with_capacity(n_codewords);
    for k in 0..n_codewords {
        let mut v = 0u8;
        for j in 0..8 {
            let (r, c) = positions[8 * k + j];
            if matrix[r as usize * cols + c as usize] != 0 {
                v |= 1 << (7 - j);
            }
        }
        out.push(v);
    }
    out
}

/// Stream codewords whose 8 modules include at least one uncertain module.
/// `uncertain` is indexed like the sampled symbol matrix
/// (`symbol_row * spec.cols + symbol_col`), so a codeword with one
/// ambiguous bit is a known erasure (one spare parity symbol) instead of
/// an unknown error (two).
pub fn codeword_erasures(spec: &SymbolSpec, uncertain: &[u8]) -> Vec<usize> {
    let positions = stream_positions(spec);
    debug_assert_eq!(uncertain.len(), spec.rows * spec.cols);
    (0..spec.codewords())
        .filter(|&k| {
            (0..8).any(|j| {
                let (r, c) = positions[8 * k + j];
                uncertain[r as usize * spec.cols + c as usize] != 0
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_every_cell_exactly_once() {
        for spec in crate::tables::SYMBOLS {
            let pos = mapping_positions(spec);
            let (mr, mc) = (spec.map_rows(), spec.map_cols());
            // exactly the stream-bit cells are visited; the rest render as
            // the fixed pattern (12x12/16x16/20x20/24x24)
            assert_eq!(pos.len(), 8 * spec.codewords(), "{}x{}", spec.rows, spec.cols);
            let mut seen = vec![false; mr * mc];
            for &(r, c) in &pos {
                assert!(!seen[r as usize * mc + c as usize], "{}x{}", spec.rows, spec.cols);
                seen[r as usize * mc + c as usize] = true;
            }
        }
    }

    #[test]
    fn positions_are_symbol_coords_in_range() {
        for spec in crate::tables::SYMBOLS {
            for (r, c) in stream_positions(spec) {
                assert!((1..spec.rows - 1).contains(&(r as usize)));
                assert!((1..spec.cols - 1).contains(&(c as usize)));
            }
        }
    }
}
