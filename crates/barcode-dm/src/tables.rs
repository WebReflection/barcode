//! ECC 200 symbol characteristics: sizes, capacity split, RS block count
//! and the data-region grid.
//!
//! Every entry is empirically validated against libdmtx/zint ground truth
//! (see tools/dmtx_placement_solve.py; 144x144 via tools/dmtx_144_derive.py).
//!
//! The inner area ((rows−2)×(cols−2)) is tiled with data regions of
//! s_r×s_c separated by 2-module lines (one clock track, one solid L).
//! The codeword placement runs on the CONCATENATED mapping matrix
//! (mr = kr·s_r rows, mc = kc·s_c cols) — a 14×14 region holds 24.5
//! codewords, which is impossible; the concatenated 28×28 holds exactly
//! 98. Rendering maps mapping (r, c) to symbol
//! (1 + r + 2·(r / s_r), 1 + c + 2·(c / s_c)).

/// One symbol size's full specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolSpec {
    /// Total symbol size in modules, including the finder/clock border.
    pub rows: usize,
    pub cols: usize,
    /// Data / ecc codeword counts (data + ecc = total capacity).
    pub data: usize,
    pub ecc: usize,
    /// Reed-Solomon block count (uniform nn = ecc / blocks).
    pub blocks: usize,
    /// Data-region grid: kr × kc regions of s_r × s_c.
    pub kr: usize,
    pub kc: usize,
    pub s_r: usize,
    pub s_c: usize,
}

impl SymbolSpec {
    /// Concatenated mapping-matrix dimensions (the placement grid).
    pub fn map_rows(&self) -> usize {
        self.kr * self.s_r
    }

    pub fn map_cols(&self) -> usize {
        self.kc * self.s_c
    }

    /// Total codeword slots = mapping cells / 8 (exact for every size).
    pub fn codewords(&self) -> usize {
        self.data + self.ecc
    }

    /// ecc codewords per block (uniform across blocks in ECC 200).
    pub fn nn(&self) -> usize {
        self.ecc / self.blocks
    }

    /// Symbol coords for a mapping-matrix coord.
    pub fn to_symbol(&self, r: usize, c: usize) -> (usize, usize) {
        (1 + r + 2 * (r / self.s_r), 1 + c + 2 * (c / self.s_c))
    }
}

const fn spec(
    rows: usize,
    cols: usize,
    data: usize,
    ecc: usize,
    blocks: usize,
    kr: usize,
    kc: usize,
    s_r: usize,
    s_c: usize,
) -> SymbolSpec {
    SymbolSpec {
        rows,
        cols,
        data,
        ecc,
        blocks,
        kr,
        kc,
        s_r,
        s_c,
    }
}

/// All symbol sizes indexed by (rows, cols).
pub const SYMBOLS: &[SymbolSpec] = &[
    // squares, single region
    spec(10, 10, 3, 5, 1, 1, 1, 8, 8),
    spec(12, 12, 5, 7, 1, 1, 1, 10, 10),
    spec(14, 14, 8, 10, 1, 1, 1, 12, 12),
    spec(16, 16, 12, 12, 1, 1, 1, 14, 14),
    spec(18, 18, 18, 14, 1, 1, 1, 16, 16),
    spec(20, 20, 22, 18, 1, 1, 1, 18, 18),
    spec(22, 22, 30, 20, 1, 1, 1, 20, 20),
    spec(24, 24, 36, 24, 1, 1, 1, 22, 22),
    spec(26, 26, 44, 28, 1, 1, 1, 24, 24),
    // squares, 2x2 regions
    spec(32, 32, 62, 36, 1, 2, 2, 14, 14),
    spec(36, 36, 86, 42, 1, 2, 2, 16, 16),
    spec(40, 40, 114, 48, 1, 2, 2, 18, 18),
    spec(44, 44, 144, 56, 1, 2, 2, 20, 20),
    spec(48, 48, 174, 68, 1, 2, 2, 22, 22),
    spec(52, 52, 204, 84, 2, 2, 2, 24, 24),
    // squares, 4x4 regions
    spec(64, 64, 280, 112, 2, 4, 4, 14, 14),
    spec(72, 72, 368, 144, 4, 4, 4, 16, 16),
    spec(80, 80, 456, 192, 4, 4, 4, 18, 18),
    spec(88, 88, 576, 224, 4, 4, 4, 20, 20),
    spec(96, 96, 696, 272, 4, 4, 4, 22, 22),
    spec(104, 104, 816, 336, 6, 4, 4, 24, 24),
    // squares, 6x6 regions
    spec(120, 120, 1050, 408, 6, 6, 6, 18, 18),
    spec(132, 132, 1304, 496, 8, 6, 6, 20, 20),
    spec(144, 144, 1558, 620, 10, 6, 6, 22, 22),
    // rectangles (DMRE)
    spec(8, 18, 5, 7, 1, 1, 1, 6, 16),
    spec(8, 32, 10, 11, 1, 1, 2, 6, 14),
    spec(12, 26, 16, 14, 1, 1, 1, 10, 24),
    spec(12, 36, 22, 18, 1, 1, 2, 10, 16),
    spec(16, 36, 32, 24, 1, 1, 2, 14, 16),
    spec(16, 48, 49, 28, 1, 1, 2, 14, 22),
];

/// Look up a symbol spec by its module dimensions.
pub fn spec_for(rows: usize, cols: usize) -> Option<&'static SymbolSpec> {
    SYMBOLS.iter().find(|s| s.rows == rows && s.cols == cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_matches_mapping_cells() {
        for s in SYMBOLS {
            // region tiling exactly accounts for the inner area
            assert_eq!(
                (s.rows - 2) - 2 * (s.kr - 1),
                s.kr * s.s_r,
                "rows {}",
                s.rows
            );
            assert_eq!(
                (s.cols - 2) - 2 * (s.kc - 1),
                s.kc * s.s_c,
                "cols {}",
                s.cols
            );
            // mapping cells minus the stream bits = the fixed-pattern leftover
            let leftover = s.map_rows() * s.map_cols() - 8 * s.codewords();
            assert!(
                leftover == 0 || leftover == 2 || leftover == 4,
                "leftover {} for {}x{}",
                leftover,
                s.rows,
                s.cols
            );
            assert_eq!(s.ecc % s.blocks, 0, "uniform nn for {}x{}", s.rows, s.cols);
        }
    }

    #[test]
    fn lookup_works() {
        assert_eq!(spec_for(10, 10).unwrap().data, 3);
        assert_eq!(spec_for(144, 144).unwrap().blocks, 10);
        assert_eq!(spec_for(8, 32).unwrap().map_cols(), 28);
        assert!(spec_for(11, 11).is_none());
    }
}
