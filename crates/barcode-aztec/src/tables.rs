//! Aztec symbol characteristics (ISO/IEC 24778), empirically pinned
//! against zint for all 36 sizes (see docs/STATUS.md aztec blocks).
//!
//! Two facts every consumer must know:
//! - Sizes 19/23/27 are AMBIGUOUS: compact L2-L4 and full L1-L3 share
//!   them. [`spec_for`] takes the compact flag, which the detector
//!   determines from the bullseye diameter (9 vs 13).
//! - `total` codewords and `skip` leading bits assume no bit stuffing;
//!   the real stream length depends on stuffed codewords, so decoders
//!   consume cells (with destuffing) rather than counting codewords.
//!   `total` is the unstuffed upper bound used to validate the table.

/// Characteristics of one symbol size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    /// zint size index, 1..=36 (compact 1-4, full 5-36).
    pub vers: u8,
    pub rows: u16,
    pub cols: u16,
    /// Layer count L.
    pub layers: u8,
    /// Compact (smaller 9x9 bullseye, 11x11 core) vs full (13/15).
    pub compact: bool,
    /// Codeword width in bits: 6/8/10/12 by layer count.
    pub bits: u8,
    /// Total codeword slots for the unstuffed stream (cap_bits / bits).
    pub total: u16,
    /// Leading stream bits to skip (cap_bits mod bits); the encoder
    /// prepends zeros there.
    pub skip: u8,
}

impl Spec {
    /// Bullseye side length (the alternating-ring finder): 9 compact, 13 full.
    pub fn bullseye(&self) -> u16 {
        if self.compact {
            9
        } else {
            13
        }
    }

    /// Core side length (bullseye + one ring holding the mode message):
    /// 11 compact, 15 full.
    pub fn core(&self) -> u16 {
        self.bullseye() + 2
    }

    /// Usable bits for the codeword stream: (88+16L)·L compact,
    /// (112+16L)·L full — verified to equal the data-region cell count
    /// minus reference-grid cells for every size.
    pub fn cap_bits(&self) -> u32 {
        let l = self.layers as u32;
        if self.compact {
            (88 + 16 * l) * l
        } else {
            (112 + 16 * l) * l
        }
    }
}

const SIZES: [Spec; 36] = [
    Spec { vers: 1, rows: 15, cols: 15, layers: 1, compact: true, bits: 6, total: 17, skip: 2 },
    Spec { vers: 2, rows: 19, cols: 19, layers: 2, compact: true, bits: 6, total: 40, skip: 0 },
    Spec { vers: 3, rows: 23, cols: 23, layers: 3, compact: true, bits: 8, total: 51, skip: 0 },
    Spec { vers: 4, rows: 27, cols: 27, layers: 4, compact: true, bits: 8, total: 76, skip: 0 },
    Spec { vers: 5, rows: 19, cols: 19, layers: 1, compact: false, bits: 6, total: 21, skip: 2 },
    Spec { vers: 6, rows: 23, cols: 23, layers: 2, compact: false, bits: 6, total: 48, skip: 0 },
    Spec { vers: 7, rows: 27, cols: 27, layers: 3, compact: false, bits: 8, total: 60, skip: 0 },
    Spec { vers: 8, rows: 31, cols: 31, layers: 4, compact: false, bits: 8, total: 88, skip: 0 },
    Spec { vers: 9, rows: 37, cols: 37, layers: 5, compact: false, bits: 8, total: 120, skip: 0 },
    Spec { vers: 10, rows: 41, cols: 41, layers: 6, compact: false, bits: 8, total: 156, skip: 0 },
    Spec { vers: 11, rows: 45, cols: 45, layers: 7, compact: false, bits: 8, total: 196, skip: 0 },
    Spec { vers: 12, rows: 49, cols: 49, layers: 8, compact: false, bits: 8, total: 240, skip: 0 },
    Spec { vers: 13, rows: 53, cols: 53, layers: 9, compact: false, bits: 10, total: 230, skip: 4 },
    Spec { vers: 14, rows: 57, cols: 57, layers: 10, compact: false, bits: 10, total: 272, skip: 0 },
    Spec { vers: 15, rows: 61, cols: 61, layers: 11, compact: false, bits: 10, total: 316, skip: 8 },
    Spec { vers: 16, rows: 67, cols: 67, layers: 12, compact: false, bits: 10, total: 364, skip: 8 },
    Spec { vers: 17, rows: 71, cols: 71, layers: 13, compact: false, bits: 10, total: 416, skip: 0 },
    Spec { vers: 18, rows: 75, cols: 75, layers: 14, compact: false, bits: 10, total: 470, skip: 4 },
    Spec { vers: 19, rows: 79, cols: 79, layers: 15, compact: false, bits: 10, total: 528, skip: 0 },
    Spec { vers: 20, rows: 83, cols: 83, layers: 16, compact: false, bits: 10, total: 588, skip: 8 },
    Spec { vers: 21, rows: 87, cols: 87, layers: 17, compact: false, bits: 10, total: 652, skip: 8 },
    Spec { vers: 22, rows: 91, cols: 91, layers: 18, compact: false, bits: 10, total: 720, skip: 0 },
    Spec { vers: 23, rows: 95, cols: 95, layers: 19, compact: false, bits: 10, total: 790, skip: 4 },
    Spec { vers: 24, rows: 101, cols: 101, layers: 20, compact: false, bits: 10, total: 864, skip: 0 },
    Spec { vers: 25, rows: 105, cols: 105, layers: 21, compact: false, bits: 10, total: 940, skip: 8 },
    Spec { vers: 26, rows: 109, cols: 109, layers: 22, compact: false, bits: 10, total: 1020, skip: 8 },
    Spec { vers: 27, rows: 113, cols: 113, layers: 23, compact: false, bits: 12, total: 920, skip: 0 },
    Spec { vers: 28, rows: 117, cols: 117, layers: 24, compact: false, bits: 12, total: 992, skip: 0 },
    Spec { vers: 29, rows: 121, cols: 121, layers: 25, compact: false, bits: 12, total: 1066, skip: 8 },
    Spec { vers: 30, rows: 125, cols: 125, layers: 26, compact: false, bits: 12, total: 1144, skip: 0 },
    Spec { vers: 31, rows: 131, cols: 131, layers: 27, compact: false, bits: 12, total: 1224, skip: 0 },
    Spec { vers: 32, rows: 135, cols: 135, layers: 28, compact: false, bits: 12, total: 1306, skip: 8 },
    Spec { vers: 33, rows: 139, cols: 139, layers: 29, compact: false, bits: 12, total: 1392, skip: 0 },
    Spec { vers: 34, rows: 143, cols: 143, layers: 30, compact: false, bits: 12, total: 1480, skip: 0 },
    Spec { vers: 35, rows: 147, cols: 147, layers: 31, compact: false, bits: 12, total: 1570, skip: 8 },
    Spec { vers: 36, rows: 151, cols: 151, layers: 32, compact: false, bits: 12, total: 1664, skip: 0 },
];

/// Codeword width in bits for a layer count (transitions at L = 3, 9, 23).
pub fn bits_for_layers(layers: u8) -> u8 {
    if layers <= 2 {
        6
    } else if layers <= 8 {
        8
    } else if layers <= 22 {
        10
    } else {
        12
    }
}

/// Look up a symbol spec by measured side length. Sizes 19/23/27 exist in
/// both compact and full form — pass the `compact` flag from the measured
/// bullseye diameter. Returns None for non-square or unknown sizes.
pub fn spec_for(rows: u16, cols: u16, compact: bool) -> Option<&'static Spec> {
    if rows != cols {
        return None;
    }
    SIZES.iter().find(|s| s.rows == rows && s.cols == cols && s.compact == compact)
}

/// All specs, e.g. for exhaustive search over candidate sizes.
pub fn all_specs() -> &'static [Spec] {
    &SIZES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_table_matches_zint_enumeration() {
        // (vers, size, layers, compact, bits, total, skip) from the probe
        // that decoded the mode message of a real zint symbol at every
        // size (all mode RS checks passed).
        let expect: [(u8, u16, u8, bool, u8, u16, u8); 36] = [
            (1, 15, 1, true, 6, 17, 2),
            (2, 19, 2, true, 6, 40, 0),
            (3, 23, 3, true, 8, 51, 0),
            (4, 27, 4, true, 8, 76, 0),
            (5, 19, 1, false, 6, 21, 2),
            (6, 23, 2, false, 6, 48, 0),
            (7, 27, 3, false, 8, 60, 0),
            (8, 31, 4, false, 8, 88, 0),
            (9, 37, 5, false, 8, 120, 0),
            (10, 41, 6, false, 8, 156, 0),
            (11, 45, 7, false, 8, 196, 0),
            (12, 49, 8, false, 8, 240, 0),
            (13, 53, 9, false, 10, 230, 4),
            (14, 57, 10, false, 10, 272, 0),
            (15, 61, 11, false, 10, 316, 8),
            (16, 67, 12, false, 10, 364, 8),
            (17, 71, 13, false, 10, 416, 0),
            (18, 75, 14, false, 10, 470, 4),
            (19, 79, 15, false, 10, 528, 0),
            (20, 83, 16, false, 10, 588, 8),
            (21, 87, 17, false, 10, 652, 8),
            (22, 91, 18, false, 10, 720, 0),
            (23, 95, 19, false, 10, 790, 4),
            (24, 101, 20, false, 10, 864, 0),
            (25, 105, 21, false, 10, 940, 8),
            (26, 109, 22, false, 10, 1020, 8),
            (27, 113, 23, false, 12, 920, 0),
            (28, 117, 24, false, 12, 992, 0),
            (29, 121, 25, false, 12, 1066, 8),
            (30, 125, 26, false, 12, 1144, 0),
            (31, 131, 27, false, 12, 1224, 0),
            (32, 135, 28, false, 12, 1306, 8),
            (33, 139, 29, false, 12, 1392, 0),
            (34, 143, 30, false, 12, 1480, 0),
            (35, 147, 31, false, 12, 1570, 8),
            (36, 151, 32, false, 12, 1664, 0),
        ];
        for (v, size, layers, compact, bits, total, skip) in expect {
            let s = spec_for(size, size, compact).unwrap();
            assert_eq!(s.vers, v, "vers for {size} compact={compact}");
            assert_eq!(s.layers, layers);
            assert_eq!(s.bits, bits);
            assert_eq!(s.total, total);
            assert_eq!(s.skip, skip);
            assert_eq!(s.cap_bits() / s.bits as u32, total as u32);
            assert_eq!((s.cap_bits() % s.bits as u32) as u8, skip);
        }
    }

    #[test]
    fn ambiguous_sizes_resolve_by_compact_flag() {
        for size in [19u16, 23, 27] {
            assert!(spec_for(size, size, true).is_some());
            assert!(spec_for(size, size, false).is_some());
            let (c, f) = (spec_for(size, size, true).unwrap(), spec_for(size, size, false).unwrap());
            assert!(c.layers != f.layers);
            assert_eq!(c.bullseye(), 9);
            assert_eq!(f.bullseye(), 13);
        }
        // unique sizes
        assert!(spec_for(15, 15, false).is_none());
        assert!(spec_for(31, 31, true).is_none());
        assert!(spec_for(12, 36, true).is_none()); // non-square (DM shape)
    }

    #[test]
    fn core_and_bullseye_dimensions() {
        let s1 = spec_for(15, 15, true).unwrap();
        assert_eq!((s1.bullseye(), s1.core()), (9, 11));
        let s36 = spec_for(151, 151, false).unwrap();
        assert_eq!((s36.bullseye(), s36.core()), (13, 15));
    }

    /// Codeword width transitions at L = 3, 9, 23.
    #[test]
    fn bits_by_layer_count() {
        for s in all_specs() {
            let want = if s.layers <= 2 {
                6
            } else if s.layers <= 8 {
                8
            } else if s.layers <= 22 {
                10
            } else {
                12
            };
            assert_eq!(s.bits, want, "vers {}", s.vers);
        }
    }
}
