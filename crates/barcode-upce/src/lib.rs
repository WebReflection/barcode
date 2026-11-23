//! Clean-room UPC-E decoder (GS1 / UPC-A zero-compression symbology).
//!
//! Structure: left guard 101 + 6 digits (7 modules each, L patterns for
//! odd parity and G patterns for even, exactly as EAN-13's left side) +
//! center guard 01010 + trailing bar 1 — 51 modules, 33 runs, the symbol
//! ends with ink. The number system (0/1) and the UPC-A check digit are
//! encoded in the parity pattern of the six digits; the printed digits
//! are the zero-compressed form of a UPC-A.
//!
//! Decoding returns the full 12-digit UPC-A expansion (the GTIN), which
//! is lossless: the parity gives NS + check, the compression case gives
//! the digit layout.

use std::collections::HashMap;

use barcode_core::{
    decode_linear_single, BitImage, DecodeResult, LinearDecoder, Point, RowRuns, Symbology,
    SymbologyDecoder,
};

/// L (odd) digit patterns — identical to EAN-13's left odd set.
const L_CODES: [&str; 10] = [
    "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
    "0110111", "0001011",
];
/// G (even) digit patterns — identical to EAN-13's left even set.
const G_CODES: [&str; 10] = [
    "0100111", "0110011", "0011011", "0100001", "0011101", "0111001", "0000101", "0010001",
    "0001001", "0010111",
];

/// Parity patterns by UPC-A check digit. E = even (G), O = odd (L).
/// NS 0 column and NS 1 column (the NS 1 pattern is the reverse of the
/// NS 0 pattern for the same check digit).
#[rustfmt::skip]
const PARITY_NS0: [&str; 10] = [
    "EEEOOO", "EEOEOO", "EEOOEO", "EEOOOE", "EOEEOO",
    "EOOEEO", "EOOOEE", "EOEOEO", "EOEOOE", "EOOEOE",
];
#[rustfmt::skip]
const PARITY_NS1: [&str; 10] = [
    "OOOEEE", "OOEOEE", "OOEEOE", "OOEEEO", "OEOOEE",
    "OEEOOE", "OEEEOO", "OEOEOE", "OEOEEO", "OEEOEO",
];

const GUARDS: [usize; 9] = [0, 1, 2, 27, 28, 29, 30, 31, 32];

const fn seg_of(s: &str) -> u8 {
    let bytes = s.as_bytes();
    let mut v = 0u8;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'1' {
            v |= 1 << i;
        }
        i += 1;
    }
    v
}

const fn build_bits(table: &[&str; 10]) -> [u8; 10] {
    let mut out = [0u8; 10];
    let mut i = 0;
    while i < 10 {
        out[i] = seg_of(table[i]);
        i += 1;
    }
    out
}

const L_BITS: [u8; 10] = build_bits(&L_CODES);
const G_BITS: [u8; 10] = build_bits(&G_CODES);

struct Candidate {
    text: [u8; 12],
    x_start: u32,
    x_end: u32,
}

#[derive(Debug)]
pub struct UpceDecoder {
    min_rows: usize,
    votes: HashMap<[u8; 12], (usize, u32, u32, u32, u32)>,
}

impl UpceDecoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            votes: HashMap::new(),
        }
    }

    fn try_at(row: &RowRuns, start: usize) -> Option<Candidate> {
        let runs = &row.runs;
        if runs.len() < start + 33 {
            return None;
        }
        if !runs[start].0 {
            return None; // symbols begin with a bar
        }
        let total = row.prefix[start + 33] - row.prefix[start];
        let n_mod = total as f32 / 51.0;
        if !(0.8..=64.0).contains(&n_mod) {
            return None;
        }
        // Exact guard envelope across f in [0.8, 1.2]: rejects >99% of junk
        // windows before entering the 11-scale quantize loop.
        let lo_env = 0.43 * n_mod;
        let hi_env = 1.75 * n_mod;
        for &g in &GUARDS {
            let len = runs[start + g].1 as f32;
            if len < lo_env || len > hi_env {
                return None;
            }
        }

        for f in [1.0f32, 0.92, 1.08, 0.85, 1.15, 0.8, 1.2, 0.95, 1.05, 0.9, 1.1] {
            let n = n_mod * f;
            let lo_g = 0.54 * n;
            let hi_g = 1.46 * n;
            let mut g_ok = true;
            for &g in &GUARDS {
                let len = runs[start + g].1 as f32;
                if len < lo_g || len > hi_g {
                    g_ok = false;
                    break;
                }
            }
            if !g_ok {
                continue;
            }
            let Some(mods) = Self::quantize(row, start, n) else {
                continue;
            };
            if !Self::guards_ok(&mods) {
                continue;
            }
            let Some(digits) = Self::decode_digits(row, start, &mods) else {
                continue;
            };
            let x_start = row.offsets[start];
            let x_end = row.offsets[start + 32] + runs[start + 32].1;
            return Some(Candidate { text: digits, x_start, x_end });
        }
        None
    }

    /// Guards: the opening bar-space-bar and the six 1-module runs of the
    /// center guard + trailing bar.
    fn guards_ok(mods: &[u8; 33]) -> bool {
        GUARDS.iter().all(|&i| mods[i] == 1)
    }

    fn quantize(row: &RowRuns, start: usize, n: f32) -> Option<[u8; 33]> {
        let mut mods = [0u8; 33];
        let mut sum = 0u32;
        let inv_n = 1.0 / n;
        for k in 0..33 {
            let m = row.runs[start + k].1 as f32 * inv_n;
            let r = m.round();
            if r < 1.0 || r > 4.0 || (m - r).abs() > 0.45 {
                return None;
            }
            mods[k] = r as u8;
            sum += r as u32;
        }
        if sum != 51 {
            return None;
        }
        Some(mods)
    }

    fn decode_digits(row: &RowRuns, start: usize, mods: &[u8; 33]) -> Option<[u8; 12]> {
        let mut bits: u128 = 0;
        let mut pos = 0usize;
        for k in 0..33 {
            for _ in 0..mods[k] {
                if row.runs[start + k].0 {
                    bits |= 1 << pos;
                }
                pos += 1;
            }
        }

        // Six digits: L match = odd parity, G match = even parity.
        let mut parity = [0u8; 6];
        let mut compressed = [0u8; 6];
        for d in 0..6 {
            let off = 3 + d * 7;
            let seg = ((bits >> off) & 0x7f) as u8;
            let li = L_BITS.iter().position(|&b| b == seg);
            let gi = G_BITS.iter().position(|&b| b == seg);
            match (li, gi) {
                (Some(li), None) => {
                    compressed[d] = li as u8;
                    parity[d] = b'O';
                }
                (None, Some(gi)) => {
                    compressed[d] = gi as u8;
                    parity[d] = b'E';
                }
                _ => return None, // ambiguous or unmatched
            }
        }

        // Parity pattern -> (number system, UPC-A check digit).
        let (ns, check) = if let Some(i) = PARITY_NS0.iter().position(|s| s.as_bytes() == parity.as_slice()) {
            (0u8, i as u8)
        } else if let Some(i) = PARITY_NS1.iter().position(|s| s.as_bytes() == parity.as_slice()) {
            (1u8, i as u8)
        } else {
            return None;
        };

        let mut digits = [0u8; 12];
        digits[0] = ns;
        expand(&compressed, &mut digits);
        digits[11] = check;

        // Defense in depth: the expanded UPC-A must have a consistent check.
        if !upca_check_valid(&digits) {
            return None;
        }
        Some(digits)
    }

    fn finish(&self, digits: [u8; 12], x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        let text: String = digits.iter().map(|d| (b'0' + d) as char).collect();
        DecodeResult {
            symbology: Symbology::UPCE,
            raw: text.as_bytes().to_vec(),
            text,
            position: [
                Point { x: x0 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y1 as f32 },
                Point { x: x0 as f32, y: y1 as f32 },
            ],
        }
    }
}

impl LinearDecoder for UpceDecoder {
    fn reset(&mut self) {
        self.votes.clear();
    }

    fn scan_line(&mut self, row: &RowRuns, y: u32) -> Option<DecodeResult> {
        let runs = &row.runs;
        if runs.len() < 34 {
            return None; // leading paper run + 33 symbol runs
        }
        debug_assert!(!runs[0].0);
        for start in (1..=(runs.len() - 33)).step_by(2) {
            let l0 = runs[start].1;
            let l1 = runs[start + 1].1;
            let l2 = runs[start + 2].1;
            let lo = l0.min(l1).min(l2);
            let hi = l0.max(l1).max(l2);
            if lo == 0 || hi > lo * 2 {
                continue;
            }
            let Some(c) = Self::try_at(row, start) else { continue };
            let (count, x0, x1, y0, y1) = {
                let e = self.votes.entry(c.text).or_insert((0, c.x_start, c.x_end, y, y));
                e.0 += 1;
                e.1 = e.1.min(c.x_start);
                e.2 = e.2.max(c.x_end);
                e.3 = e.3.min(y);
                e.4 = e.4.max(y);
                *e
            };
            if count >= self.min_rows {
                return Some(self.finish(c.text, x0, x1, y0, y1));
            }
        }
        None
    }

    fn finish_votes(&self) -> Option<DecodeResult> {
        self.votes
            .iter()
            .max_by_key(|(_, v)| v.0)
            .filter(|(_, v)| v.0 >= self.min_rows)
            .map(|(t, v)| self.finish(*t, v.1, v.2, v.3, v.4))
    }

    fn vote_count(&self) -> usize {
        if self.votes.is_empty() { 0 } else { self.votes.values().map(|v| v.0).sum() }
    }
}

/// Zero-compression expansion: the 6 printed digits + the case given by
/// the last digit. Writes digits 1-10 of the UPC-A (NS at 0 and check at
/// 11 are set by the caller).
fn expand(u: &[u8; 6], digits: &mut [u8; 12]) {
    let last = u[5];
    match last {
        0..=2 => {
            // mfr = u1 u2 last 0 0; product = 0 0 u3 u4 u5
            digits[1] = u[0];
            digits[2] = u[1];
            digits[3] = last;
            digits[4] = 0;
            digits[5] = 0;
            digits[6] = 0;
            digits[7] = 0;
            digits[8] = u[2];
            digits[9] = u[3];
            digits[10] = u[4];
        }
        3 => {
            // mfr = u1 u2 u3 0 0; product = 0 0 0 u4 u5
            digits[1] = u[0];
            digits[2] = u[1];
            digits[3] = u[2];
            digits[4] = 0;
            digits[5] = 0;
            digits[6] = 0;
            digits[7] = 0;
            digits[8] = 0;
            digits[9] = u[3];
            digits[10] = u[4];
        }
        4 => {
            // mfr = u1 u2 u3 u4 0; product = 0 0 0 0 u5
            digits[1] = u[0];
            digits[2] = u[1];
            digits[3] = u[2];
            digits[4] = u[3];
            digits[5] = 0;
            digits[6] = 0;
            digits[7] = 0;
            digits[8] = 0;
            digits[9] = 0;
            digits[10] = u[4];
        }
        5..=9 => {
            // mfr = u1..u5; product = 0 0 0 0 u6
            for i in 0..5 {
                digits[1 + i] = u[i];
            }
            digits[6] = 0;
            digits[7] = 0;
            digits[8] = 0;
            digits[9] = 0;
            digits[10] = u[5];
        }
        _ => unreachable!(),
    }
}

/// UPC-A check digit validity (weights 1,3,1,3,... over digits 0-10).
fn upca_check_valid(digits: &[u8; 12]) -> bool {
    let mut sum = 0usize;
    for (i, &d) in digits.iter().enumerate().take(11) {
        sum += d as usize * if i % 2 == 0 { 3 } else { 1 };
    }
    (10 - sum % 10) % 10 == digits[11] as usize
}

impl SymbologyDecoder for UpceDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::UPCE
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        decode_linear_single(self, image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_tables_are_complete_and_distinct() {
        let mut all = Vec::new();
        for p in PARITY_NS0 {
            all.push(p);
        }
        for p in PARITY_NS1 {
            all.push(p);
        }
        assert_eq!(all.len(), 20);
        assert_eq!(all.iter().collect::<std::collections::HashSet<_>>().len(), 20);
    }

    #[test]
    fn expansion_rules() {
        // 042100005264 -> 425261 (M=42100 ends in "100" -> case u6=1)
        let mut d = [0u8; 12];
        d[0] = 0;
        d[11] = 4;
        expand(&[4, 2, 5, 2, 6, 1], &mut d);
        let text: String = d.iter().map(|x| (b'0' + x) as char).collect();
        assert_eq!(text, "042100005264");
        // 065100004327 -> 654321 (case u6=1, NS 0)
        let mut d = [0u8; 12];
        d[0] = 0;
        d[11] = 7;
        expand(&[6, 5, 4, 3, 2, 1], &mut d);
        let text: String = d.iter().map(|x| (b'0' + x) as char).collect();
        assert_eq!(text, "065100004327");
    }

    fn decode_canvas(canvas: &barcode_fixtures::Canvas, min_rows: usize) -> Option<String> {
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = UpceDecoder::new(min_rows);
        d.decode(&img).first().map(|r| r.text.clone())
    }

    #[test]
    fn round_trip_synthetic_symbols() {
        // (UPC-A, UPC-E) pairs across all compression cases
        let cases: [&str; 6] = [
            "042100005264", // ABHIJ1 (the published example)
            "012000007897", // ABHIJ0
            "012200007895", // ABHIJ2
            "012300000895", // AB3IJ3
            "012400000894", // AB4IJ3
            "012911000055", // ABCDE5
        ];
        for case in cases {
            for narrow in [3u32, 4, 5] {
                let canvas = barcode_fixtures::encode_upce(case, narrow, 24, 9 * narrow);
                assert_eq!(
                    decode_canvas(&canvas, 2).as_deref(),
                    Some(case),
                    "narrow={narrow} case={case}"
                );
            }
        }
    }

    #[test]
    fn ground_truth_module_string() {
        // The published encoding of UPC-E 425261 (from UPC-A 042100005264):
        // 101 0011101 0010011 0111001 0011011 0101111 0011001 01010 1
        let canvas = barcode_fixtures::encode_upce("042100005264", 1, 1, 0);
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut s = String::new();
        for x in 0..canvas.width {
            s.push(if img.ink(x, 0) { '1' } else { '0' });
        }
        assert_eq!(
            s,
            "101001110100100110111001001101101011110011001010101",
            "module string mismatch: {s}"
        );
    }
}
