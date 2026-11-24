//! Clean-room EAN-8 decoder (ISO/IEC 15420). EAN-13's little sibling:
//! 4 digits on the left (all L patterns), 4 on the right (all R), no
//! parity-encoded first digit. 67 modules = 3 + 4*7 + 5 + 4*7 + 3,
//! 43 runs. Check digit weights 3,1,3,1,3,1,3 over the first 7 digits.

use std::collections::HashMap;

use barcode_core::{
    decode_linear_single, BitImage, DecodeResult, LinearDecoder, Point, RowRuns, Symbology,
    SymbologyDecoder,
};

/// L (odd) digit patterns, module bit per char position.
const L_CODES: [&str; 10] = [
    "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
    "0110111", "0001011",
];
/// R digit patterns.
const R_CODES: [&str; 10] = [
    "1110010", "1100110", "1101100", "1000010", "1011100", "1001110", "1010000", "1000100",
    "1001000", "1110100",
];

const GUARDS: [usize; 11] = [0, 1, 2, 19, 20, 21, 22, 23, 40, 41, 42];

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

const fn pattern_runs(pattern: &str) -> [u8; 4] {
    let bytes = pattern.as_bytes();
    let mut out = [0u8; 4];
    let mut k = 0usize;
    let mut cur = bytes[0];
    let mut n = 0u8;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == cur {
            n += 1;
        } else {
            out[k] = n;
            k += 1;
            cur = bytes[i];
            n = 1;
        }
        i += 1;
    }
    out[k] = n;
    out
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

const fn build_runs(table: &[&str; 10]) -> [[u8; 4]; 10] {
    let mut out = [[0u8; 4]; 10];
    let mut i = 0;
    while i < 10 {
        out[i] = pattern_runs(table[i]);
        i += 1;
    }
    out
}

const L_BITS: [u8; 10] = build_bits(&L_CODES);
const R_BITS: [u8; 10] = build_bits(&R_CODES);
const L_RUNS: [[u8; 4]; 10] = build_runs(&L_CODES);
const R_RUNS: [[u8; 4]; 10] = build_runs(&R_CODES);

struct Candidate {
    text: [u8; 8],
    x_start: u32,
    x_end: u32,
}

#[derive(Debug)]
pub struct Ean8Decoder {
    min_rows: usize,
    votes: HashMap<[u8; 8], (usize, u32, u32, u32, u32)>,
}

impl Ean8Decoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            votes: HashMap::new(),
        }
    }

    /// Try to decode a symbol starting at run `start`.
    fn try_at(row: &RowRuns, start: usize) -> Option<Candidate> {
        let runs = &row.runs;
        if runs.len() < start + 43 {
            return None;
        }
        if !runs[start].0 {
            return None; // symbols begin with a bar
        }
        let total = row.prefix[start + 43] - row.prefix[start];
        let n_mod = total as f32 / 67.0;
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

        for f in [
            1.0f32, 0.92, 1.08, 0.85, 1.15, 0.8, 1.2, 0.95, 1.05, 0.9, 1.1,
        ] {
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
            let x_end = x_start + total;
            return Some(Candidate {
                text: digits,
                x_start,
                x_end,
            });
        }
        None
    }

    fn try_variance(row: &RowRuns, start: usize) -> Option<Candidate> {
        let runs = &row.runs;
        if runs.len() < start + 43 {
            return None;
        }
        let g = [runs[start].1, runs[start + 1].1, runs[start + 2].1];
        let s = (g[0] + g[1] + g[2]) as f32 / 3.0;
        if s < 0.7 || g.iter().any(|&v| (v as f32 - s).abs() > s * 0.7) {
            return None;
        }
        // Symmetric right quiet zone (>= 3 modules, matching the left gate):
        // without it, periodic woven/grid texture (qr-hard-negative) can
        // fit 43 noisy runs and pass EAN-8's 1-in-10 check digit on 2 rows.
        if let Some(&(ink, w)) = runs.get(start + 43) {
            if ink || (w as f32) < s * 3.0 {
                return None;
            }
        }
        let max_g = s * 2.29;
        for &idx in &GUARDS[3..] {
            if (runs[start + idx].1 as f32) > max_g {
                return None;
            }
        }
        for sf in [1.0f32, 0.9, 1.1, 0.8, 1.2, 0.85, 1.15] {
            let scale = s * sf;
            // Center guard (5 runs) and end guard (3 runs) are 1 module each.
            let guards_ok = GUARDS[3..]
                .iter()
                .all(|&idx| (runs[start + idx].1 as f32 - scale).abs() <= scale * 0.9);
            if !guards_ok {
                continue;
            }
            let mut digits = [0u8; 8];
            let mut ok = true;
            for d in 0..4 {
                let i = start + 3 + d * 4;
                let druns = [runs[i].1, runs[i + 1].1, runs[i + 2].1, runs[i + 3].1];
                match best_digit(druns, scale, &L_RUNS) {
                    Some((dd, _)) => digits[d] = dd,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            for d in 0..4 {
                let i = start + 24 + d * 4;
                let druns = [runs[i].1, runs[i + 1].1, runs[i + 2].1, runs[i + 3].1];
                match best_digit(druns, scale, &R_RUNS) {
                    Some((dd, _)) => digits[4 + d] = dd,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            if !check_digit_valid(&digits) {
                continue;
            }
            let total = row.prefix[start + 43] - row.prefix[start];
            return Some(Candidate {
                text: digits,
                x_start: row.offsets[start],
                x_end: row.offsets[start] + total,
            });
        }
        None
    }

    /// Start/center/end guard runs must all be single modules.
    /// Run indices: left guard 0-2, center 19-23, right 40-42.
    fn guards_ok(mods: &[u8; 43]) -> bool {
        GUARDS.iter().all(|&i| mods[i] == 1)
    }

    /// Quantize the 43 runs at module width `n`.
    fn quantize(row: &RowRuns, start: usize, n: f32) -> Option<[u8; 43]> {
        let mut mods = [0u8; 43];
        let mut sum = 0u32;
        let inv_n = 1.0 / n;
        for k in 0..43 {
            let m = row.runs[start + k].1 as f32 * inv_n;
            let r = m.round();
            if r < 1.0 || r > 4.0 || (m - r).abs() > 0.45 {
                return None;
            }
            mods[k] = r as u8;
            sum += r as u32;
        }
        if sum != 67 {
            return None;
        }
        Some(mods)
    }

    fn decode_digits(row: &RowRuns, start: usize, mods: &[u8; 43]) -> Option<[u8; 8]> {
        // Expand to 67 module bits.
        let mut bits: u128 = 0;
        let mut pos = 0usize;
        for k in 0..43 {
            for _ in 0..mods[k] {
                if row.runs[start + k].0 {
                    bits |= 1 << pos;
                }
                pos += 1;
            }
        }

        let mut digits = [0u8; 8];
        for d in 0..4 {
            let off = 3 + d * 7;
            let seg = ((bits >> off) & 0x7f) as u8;
            let li = L_BITS
                .iter()
                .position(|&b| b == seg)
                .map(|i| i as u8)
                .or_else(|| {
                    let mut found: Option<u8> = None;
                    for (i, &b) in L_BITS.iter().enumerate() {
                        if dist1(seg, b) <= 1 {
                            if found.is_some() {
                                return None;
                            }
                            found = Some(i as u8);
                        }
                    }
                    found
                })?;
            digits[d] = li;
        }
        for d in 0..4 {
            let off = 36 + d * 7;
            let seg = ((bits >> off) & 0x7f) as u8;
            let ri = R_BITS
                .iter()
                .position(|&b| b == seg)
                .map(|i| i as u8)
                .or_else(|| {
                    let mut found: Option<u8> = None;
                    for (i, &b) in R_BITS.iter().enumerate() {
                        if dist1(seg, b) <= 1 {
                            if found.is_some() {
                                return None;
                            }
                            found = Some(i as u8);
                        }
                    }
                    found
                })?;
            digits[4 + d] = ri;
        }

        if !check_digit_valid(&digits) {
            return None;
        }
        Some(digits)
    }

    fn finish(&self, digits: [u8; 8], x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        let text: String = digits.iter().map(|d| (b'0' + d) as char).collect();
        DecodeResult {
            symbology: Symbology::EAN8,
            raw: text.as_bytes().to_vec(),
            text,
            position: [
                Point {
                    x: x0 as f32,
                    y: y0 as f32,
                },
                Point {
                    x: x1 as f32,
                    y: y0 as f32,
                },
                Point {
                    x: x1 as f32,
                    y: y1 as f32,
                },
                Point {
                    x: x0 as f32,
                    y: y1 as f32,
                },
            ],
        }
    }
}

impl LinearDecoder for Ean8Decoder {
    fn reset(&mut self) {
        self.votes.clear();
    }

    fn scan_line(&mut self, row: &RowRuns, y: u32) -> Option<DecodeResult> {
        let runs = &row.runs;
        if runs.len() < 44 {
            return None; // leading paper run + 43 symbol runs
        }
        debug_assert!(!runs[0].0);
        for start in (1..=(runs.len() - 43)).step_by(2) {
            let l0 = runs[start].1;
            let l1 = runs[start + 1].1;
            let l2 = runs[start + 2].1;
            let lo = l0.min(l1).min(l2);
            let hi = l0.max(l1).max(l2);
            if lo == 0 || hi > lo * 2 {
                continue;
            }
            let Some(c) = Self::try_at(row, start) else {
                continue;
            };
            let e = self
                .votes
                .entry(c.text)
                .or_insert((0, c.x_start, c.x_end, y, y));
            // Every row votes; `finish_votes` arbitrates by plurality. An
            // early return here would trust the first row that decodes.
            e.0 += 1;
            e.1 = e.1.min(c.x_start);
            e.2 = e.2.max(c.x_end);
            e.3 = e.3.min(y);
            e.4 = e.4.max(y);
        }
        for start in (1..=(runs.len() - 43)).step_by(2) {
            let s = (runs[start].1 + runs[start + 1].1 + runs[start + 2].1) as f32 / 3.0;
            if (runs[start - 1].1 as f32) < s * 3.0 {
                continue;
            }
            let Some(c) = Self::try_variance(row, start) else {
                continue;
            };
            let e = self
                .votes
                .entry(c.text)
                .or_insert((0, c.x_start, c.x_end, y, y));
            // Every row votes; `finish_votes` arbitrates by plurality. An
            // early return here would trust the first row that decodes.
            e.0 += 1;
            e.1 = e.1.min(c.x_start);
            e.2 = e.2.max(c.x_end);
            e.3 = e.3.min(y);
            e.4 = e.4.max(y);
            break;
        }
        None
    }

    fn finish_votes(&self) -> Option<DecodeResult> {
        barcode_core::plurality(&self.votes, |v| v.0, self.min_rows)
            .map(|(t, v)| self.finish(*t, v.1, v.2, v.3, v.4))
    }

    fn vote_count(&self) -> usize {
        if self.votes.is_empty() {
            0
        } else {
            self.votes.values().map(|v| v.0).sum()
        }
    }
}

impl SymbologyDecoder for Ean8Decoder {
    fn symbology(&self) -> Symbology {
        Symbology::EAN8
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        decode_linear_single(self, image)
    }
}

/// Local-variance tolerance for the per-digit fallback: the mean absolute
/// deviation of a digit's 4 runs from the expected 7-module pattern, in
/// modules. One misplaced module boundary is 1/7 ≈ 0.14.
const VARIANCE_TOL: f32 = 0.5;

/// Closest digit in `table` under the local variance, if it is within
/// `VARIANCE_TOL`.
fn best_digit(runs: [u32; 4], scale: f32, table: &[[u8; 4]; 10]) -> Option<(u8, f32)> {
    let total = runs.iter().sum::<u32>().max(1) as f32;
    let inv_total = 1.0 / total;
    let mut best: Option<(u8, f32)> = None;
    for (d, exp) in table.iter().enumerate() {
        let var = ((runs[0] as f32 - exp[0] as f32 * scale).abs()
            + (runs[1] as f32 - exp[1] as f32 * scale).abs()
            + (runs[2] as f32 - exp[2] as f32 * scale).abs()
            + (runs[3] as f32 - exp[3] as f32 * scale).abs())
            * inv_total;
        if var <= VARIANCE_TOL && best.map_or(true, |(_, bv)| var < bv) {
            best = Some((d as u8, var));
        }
    }
    best
}

/// Hamming distance between two 7-bit segments.
fn dist1(a: u8, b: u8) -> u8 {
    (a ^ b).count_ones() as u8
}

/// EAN-8 check digit: weights 3,1,3,1,3,1,3 over the first 7 digits.
pub fn check_digit_valid(digits: &[u8; 8]) -> bool {
    let mut sum = 0usize;
    for (i, &d) in digits.iter().enumerate().take(7) {
        sum += d as usize * if i % 2 == 0 { 3 } else { 1 };
    }
    (10 - sum % 10) % 10 == digits[7] as usize
}

/// Compute the check digit for the first 7 digits.
pub fn check_digit(digits: &[u8; 7]) -> u8 {
    let mut sum = 0usize;
    for (i, &d) in digits.iter().enumerate() {
        sum += d as usize * if i % 2 == 0 { 3 } else { 1 };
    }
    ((10 - sum % 10) % 10) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_canvas(canvas: &barcode_fixtures::Canvas, min_rows: usize) -> Option<String> {
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = Ean8Decoder::new(min_rows);
        d.decode(&img).first().map(|r| r.text.clone())
    }

    #[test]
    fn check_digit_examples() {
        // 96385074 is the classic published EAN-8 example
        assert_eq!(check_digit(&[9, 6, 3, 8, 5, 0, 7]), 4);
        assert!(check_digit_valid(&[9, 6, 3, 8, 5, 0, 7, 4]));
        assert!(!check_digit_valid(&[9, 6, 3, 8, 5, 0, 7, 0]));
    }

    #[test]
    fn round_trip_synthetic_symbols() {
        let cases: [&str; 4] = ["96385074", "24056731", "73127727", "00123457"];
        for case in cases {
            for narrow in [3u32, 4, 5] {
                let canvas = barcode_fixtures::encode_ean8(case, narrow, 24, 9 * narrow);
                assert_eq!(
                    decode_canvas(&canvas, 2).as_deref(),
                    Some(case),
                    "narrow={narrow} case={case}"
                );
            }
        }
    }

    #[test]
    fn rejects_bad_check_digit() {
        let canvas = barcode_fixtures::encode_ean8_raw("96385070", 4, 24, 9 * 4);
        assert_eq!(decode_canvas(&canvas, 2), None);
    }
}
