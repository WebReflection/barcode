//! Clean-room Code 128 decoder (ISO/IEC 15417; covers GS1-128 payloads).
//!
//! Structure: start (6 runs, value 103/104/105 = set A/B/C) + data chars
//! (6 runs each, widths 1-4 modules summing to 11) + check char (mod 103)
//! + stop (7 runs: 2,3,3,1,1,1,2). The unit is re-estimated per character
//! from that character's own 6 run widths (sum/11), so no global calibration
//! is needed. The check digit is mandatory — noise and mid-symbol starts
//! survive it only with probability ~1/103.

use std::collections::HashMap;

use barcode_core::{BitImage, DecodeResult, Point, Symbology, SymbologyDecoder};

/// Element widths (bar, space, bar, space, bar, space) of code values 0-106,
/// transcribed from the ISO/IEC 15417 pattern table. Value 106 is the stop's
/// first six elements; the stop appends a 2-module final bar.
#[rustfmt::skip]
const PATTERNS: [[u8; 6]; 107] = [
    [2,1,2,2,2,2], [2,2,2,1,2,2], [2,2,2,2,2,1], [1,2,1,2,2,3], [1,2,1,3,2,2],
    [1,3,1,2,2,2], [1,2,2,2,1,3], [1,2,2,3,1,2], [1,3,2,2,1,2], [2,2,1,2,1,3],
    [2,2,1,3,1,2], [2,3,1,2,1,2], [1,1,2,2,3,2], [1,2,2,1,3,2], [1,2,2,2,3,1],
    [1,1,3,2,2,2], [1,2,3,1,2,2], [1,2,3,2,2,1], [2,2,3,2,1,1], [2,2,1,1,3,2],
    [2,2,1,2,3,1], [2,1,3,2,1,2], [2,2,3,1,1,2], [3,1,2,1,3,1], [3,1,1,2,2,2],
    [3,2,1,1,2,2], [3,2,1,2,2,1], [3,1,2,2,1,2], [3,2,2,1,1,2], [3,2,2,2,1,1],
    [2,1,2,1,2,3], [2,1,2,3,2,1], [2,3,2,1,2,1], [1,1,1,3,2,3], [1,3,1,1,2,3],
    [1,3,1,3,2,1], [1,1,2,3,1,3], [1,3,2,1,1,3], [1,3,2,3,1,1], [2,1,1,3,1,3],
    [2,3,1,1,1,3], [2,3,1,3,1,1], [1,1,2,1,3,3], [1,1,2,3,3,1], [1,3,2,1,3,1],
    [1,1,3,1,2,3], [1,1,3,3,2,1], [1,3,3,1,2,1], [3,1,3,1,2,1], [2,1,1,3,3,1],
    [2,3,1,1,3,1], [2,1,3,1,1,3], [2,1,3,3,1,1], [2,1,3,1,3,1], [3,1,1,1,2,3],
    [3,1,1,3,2,1], [3,3,1,1,2,1], [3,1,2,1,1,3], [3,1,2,3,1,1], [3,3,2,1,1,1],
    [3,1,4,1,1,1], [2,2,1,4,1,1], [4,3,1,1,1,1], [1,1,1,2,2,4], [1,1,1,4,2,2],
    [1,2,1,1,2,4], [1,2,1,4,2,1], [1,4,1,1,2,2], [1,4,1,2,2,1], [1,1,2,2,1,4],
    [1,1,2,4,1,2], [1,2,2,1,1,4], [1,2,2,4,1,1], [1,4,2,1,1,2], [1,4,2,2,1,1],
    [2,4,1,2,1,1], [2,2,1,1,1,4], [4,1,3,1,1,1], [2,4,1,1,1,2], [1,3,4,1,1,1],
    [1,1,1,2,4,2], [1,2,1,1,4,2], [1,2,1,2,4,1], [1,1,4,2,1,2], [1,2,4,1,1,2],
    [1,2,4,2,1,1], [4,1,1,2,1,2], [4,2,1,1,1,2], [4,2,1,2,1,1], [2,1,2,1,4,1],
    [2,1,4,1,2,1], [4,1,2,1,2,1], [1,1,1,1,4,3], [1,1,1,3,4,1], [1,3,1,1,4,1],
    [1,1,4,1,1,3], [1,1,4,3,1,1], [4,1,1,1,1,3], [4,1,1,3,1,1], [1,1,3,1,4,1],
    [1,1,4,1,3,1], [3,1,1,1,4,1], [4,1,1,1,3,1], [2,1,1,4,1,2], [2,1,1,2,1,4],
    [2,1,1,2,3,2], [2,3,3,1,1,1],
];

const STOP7: [u8; 7] = [2, 3, 3, 1, 1, 1, 2];

/// Closest-pattern tolerance (module units, squared for the averages):
/// a character's six elements must average within 0.55 modules² and no
/// single element may be off by more than 1.4 modules. Clean renders are
/// exact; the receipt's merged/eroded bars land around 0.1–0.3.
const CHAR_AVG_VARIANCE: f32 = 0.55;
const STOP_AVG_VARIANCE: f32 = 0.6;
const ELEMENT_VARIANCE: f32 = 1.4;

/// `PATTERNS` transposed for the SIMD closest-pattern search: element-major,
/// padded to a multiple of the 4-lane vector. The padding patterns are
/// unreachable (100 modules per element) and the index bound rejects them.
#[cfg(target_arch = "wasm32")]
const NPAD: usize = 108;

#[cfg(target_arch = "wasm32")]
const PATTERN_COLS: [[f32; NPAD]; 6] = {
    let mut t = [[0.0f32; NPAD]; 6];
    let mut p = 0;
    while p < PATTERNS.len() {
        let mut i = 0;
        while i < 6 {
            t[i][p] = PATTERNS[p][i] as f32;
            i += 1;
        }
        p += 1;
    }
    let mut i = 0;
    while i < 6 {
        t[i][107] = 100.0;
        i += 1;
    }
    t
};

/// The fuzzy fallback of [`Code128Decoder::char_value`]: closest table
/// pattern by mean squared module error. SIMD is opted in at this function
/// only (see the barcode-core mirror note): a global `simd128` target
/// feature also re-vectorizes the branchy scan loops, measured slower.
#[inline]
fn closest_pattern(raw: &[f32; 6]) -> Option<(f32, u8)> {
    #[cfg(target_arch = "wasm32")]
    {
        // The SDK's browser floor is WebGPU-capable engines, all of which
        // ship wasm SIMD; the opt-in is local to this function.
        closest_pattern_simd(raw)
    }
    #[cfg(not(target_arch = "wasm32"))]
    closest_pattern_scalar(raw)
}

#[cfg(not(target_arch = "wasm32"))]
fn closest_pattern_scalar(raw: &[f32; 6]) -> Option<(f32, u8)> {
    let mut best: Option<(f32, u8)> = None; // (mean squared error, value)
    for (v, p) in PATTERNS.iter().enumerate() {
        let mut err = 0.0f32;
        let mut worst = 0.0f32;
        for i in 0..6 {
            let d = raw[i] - p[i] as f32;
            err += d * d;
            worst = worst.max(d.abs());
        }
        let mean = err / 6.0;
        if mean <= CHAR_AVG_VARIANCE
            && worst <= ELEMENT_VARIANCE
            && best.map_or(true, |(b, _)| mean < b)
        {
            best = Some((mean, v as u8));
        }
    }
    best
}

#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
fn closest_pattern_simd(raw: &[f32; 6]) -> Option<(f32, u8)> {
    use core::arch::wasm32::*;
    let mut best: Option<(f32, u8)> = None;
    let mut g = 0;
    while g < NPAD {
        let mut err = f32x4_splat(0.0);
        let mut worst = f32x4_splat(0.0);
        for i in 0..6 {
            // SAFETY: g is a multiple of 4 and g+3 < NPAD.
            let p = unsafe {
                core::ptr::read_unaligned(PATTERN_COLS[i].as_ptr().add(g) as *const v128)
            };
            let d = f32x4_sub(f32x4_splat(raw[i]), p);
            err = f32x4_add(err, f32x4_mul(d, d));
            worst = f32x4_max(worst, f32x4_abs(d));
        }
        let mean = f32x4_mul(err, f32x4_splat(1.0 / 6.0));
        let mut ms = [0.0f32; 4];
        let mut ws = [0.0f32; 4];
        // SAFETY: both arrays are exactly one 16-byte vector.
        unsafe {
            core::ptr::write_unaligned(ms.as_mut_ptr() as *mut v128, mean);
            core::ptr::write_unaligned(ws.as_mut_ptr() as *mut v128, worst);
        }
        for lane in 0..4 {
            let v = g + lane;
            if v >= PATTERNS.len() {
                break;
            }
            let m = ms[lane];
            if m <= CHAR_AVG_VARIANCE
                && ws[lane] <= ELEMENT_VARIANCE
                && best.map_or(true, |(b, _)| m < b)
            {
                best = Some((m, v as u8));
            }
        }
        g += 4;
    }
    best
}

struct Candidate {
    text: String,
    x_start: u32,
    x_end: u32,
    /// Every element matched its table value by exact rounding — the
    /// pre-tolerance behaviour. Fuzzy candidates are a strict fallback.
    exact: bool,
    /// Sum of the per-element mean-squared pattern errors of every
    /// character (and the stop). The true sequence fits the table much
    /// better than a misaligned decode that merely passes the checksum.
    score: f32,
}

#[derive(Debug)]
pub struct Code128Decoder {
    min_rows: usize,
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    votes: HashMap<String, (usize, usize, u32, u32, u32, u32, f32)>,
}

impl Code128Decoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            runs: Vec::with_capacity(512),
            offsets: Vec::with_capacity(512),
            votes: HashMap::new(),
        }
    }

    fn scan_row(&mut self, image: &BitImage, y: u32) {
        self.runs.clear();
        self.offsets.clear();
        let mut cur = false;
        let mut n = 0u32;
        let mut run_start = 0u32;
        let mut x = 0u32;
        while x < image.width {
            let ink = image.ink(x, y);
            if ink == cur {
                n += 1;
            } else {
                self.runs.push((cur, std::mem::replace(&mut n, 1)));
                self.offsets.push(run_start);
                run_start = x;
                cur = ink;
            }
            x += 1;
        }
        if n > 0 {
            self.runs.push((cur, n));
            self.offsets.push(image.width - n);
        }
    }

    /// Widths of runs[k..k+6] decoded against the pattern table using that
    /// character's own unit (6 widths sum to 11 modules).
    ///
    /// Exact rounding first (clean renders hit this on every row); on a
    /// ±1-module miss the *closest* pattern is accepted when the whole
    /// character stays within `CHAR_AVG_VARIANCE` (mean squared module
    /// error) and no single element is off by more than
    /// `ELEMENT_VARIANCE`. This is what a degraded print needs: one
    /// widened bar must not invalidate an otherwise certain character.
    /// The mandatory mod-103 check digit (and the scanner's `min_rows`
    /// corroboration) is the false-positive gate.
    fn char_value(&self, k: usize) -> Option<(u8, f32, bool, f32)> {
        if k + 6 > self.runs.len() {
            return None;
        }
        let sum: u32 = (0..6).map(|i| self.runs[k + i].1).sum();
        let unit = sum as f32 / 11.0;
        if unit < 1.0 {
            return None;
        }
        let mut raw = [0f32; 6];
        let mut exact = true;
        for i in 0..6 {
            raw[i] = self.runs[k + i].1 as f32 / unit;
            let m = raw[i].round();
            if !(1.0..=4.0).contains(&m) {
                exact = false;
            }
        }
        if exact {
            let w = raw.map(|v| v.round() as u8);
            if let Some(v) = PATTERNS.iter().position(|p| *p == w) {
                return Some((v as u8, unit, true, 0.0));
            }
        }
        // Cheap junk-window reject: a valid element is 1-4 modules, so a
        // window with an element outside 0.5-4.5 cannot be a character
        // under any tolerance. Junk-rich still frames scan thousands of
        // windows; without this the closest-pattern search costs 1.7s of
        // wall time (dm_deadline_probe).
        if raw.iter().any(|&v| v < 0.5 || v > 4.5) {
            return None;
        }
        closest_pattern(&raw).map(|(err, v)| (v, unit, false, err))
    }

    /// The 7-run stop: 2,3,3,1,1,1,2 modules. Same closest-pattern
    /// tolerance as a character — the stop's shape is the end-of-symbol
    /// confirmation, and a ±1 module smear on the receipt must not lose an
    /// otherwise complete read.
    ///
    /// The stop need NOT be the row's last ink run: a receipt has printed
    /// text and rules beside the barcode, so on the winning quarter-turned
    /// column the symbol ends mid-row (the field photo that motivated the
    /// tolerance). The mod-103 check digit plus the start/stop structure
    /// and the scanner's `min_rows` corroboration keep mid-row starts
    /// from inventing payloads.
    /// `(0 = no stop, 1 = tolerant stop, 2 = bit-exact stop)` plus the
    /// mean-squared pattern error.
    fn stop_at_impl(&self, k: usize) -> (u8, f32) {
        if k + 7 > self.runs.len() {
            return (0, 0.0);
        }
        let sum: u32 = (0..7).map(|i| self.runs[k + i].1).sum();
        let unit = sum as f32 / 13.0;
        if unit < 1.0 {
            return (0, 0.0);
        }
        let mut err = 0.0f32;
        let mut exact = true;
        for (i, &m) in STOP7.iter().enumerate() {
            // Even i are bars, odd i are spaces.
            if self.runs[k + i].0 != (i % 2 == 0) {
                return (0, 0.0); // bar/space alternation broken
            }
            let raw = self.runs[k + i].1 as f32 / unit;
            let d = raw - m as f32;
            if d.abs() > ELEMENT_VARIANCE {
                return (0, 0.0);
            }
            if raw.round() as u8 != m {
                exact = false;
            }
            err += d * d;
        }
        let mean = err / 7.0;
        if mean > STOP_AVG_VARIANCE {
            return (0, 0.0);
        }
        (if exact { 2 } else { 1 }, mean)
    }

    /// Decode a symbol whose first bar run is at `start`.
    fn try_from(&self, start: usize) -> Option<Candidate> {
        let (first, _, mut exact, first_err) = self.char_value(start)?;
        if !matches!(first, 103..=105) {
            return None;
        }
        let _set = match first {
            103 => Set::A,
            104 => Set::B,
            _ => Set::C,
        };
        let mut values: Vec<u8> = vec![first];
        let mut score = first_err;
        let mut k = start + 6;
        // A stop match is only the end of the symbol when the check digit
        // validates. But a *tolerant* stop can also match a window inside
        // the data; with a 1-in-103 checksum coincidence that would end an
        // otherwise clean read early ("ROTATED-CODE128" -> "ROTATED-").
        // Exact stops win immediately; a tolerant stop is only a fallback
        // for the same start (degraded prints never round exactly).
        // A tolerant stop can also match a window inside the data. Keep
        // the LONGEST valid read from this start: a damaged start can
        // produce an early 1-2 codeword stop (the checksum is only 1/103),
        // and that must not shadow the full symbol. A genuine short
        // symbol has exactly one validating stop, so nothing changes.
        let mut fallback: Option<(Candidate, usize)> = None;
        loop {
            if values.len() >= 2 {
                let (stop, stop_err) = self.stop_at_impl(k);
                if stop != 0 {
                    let check = *values.last().unwrap();
                    let data = &values[1..values.len() - 1];
                    // ISO/IEC 15417 allows a single data codeword: do not
                    // reject valid short symbols. The mod-103 check plus
                    // the scanner's `min_rows` corroboration gate noise.
                    if !data.is_empty() && check_valid(first, data, check) {
                        let cand = Candidate {
                            text: build_text(first, data),
                            x_start: self.offsets[start],
                            x_end: self.offsets[k + 6] + self.runs[k + 6].1,
                            exact: exact && stop == 2,
                            score: score + stop_err,
                        };
                        if cand.exact {
                            return Some(cand);
                        }
                        if fallback.as_ref().map_or(true, |(_, len)| data.len() > *len) {
                            fallback = Some((cand, data.len()));
                        }
                    }
                }
            }
            let Some((v, _, this_exact, err)) = self.char_value(k) else {
                return fallback.map(|(c, _)| c);
            };
            exact &= this_exact;
            score += err;
            values.push(v);
            k += 6;
            // No fixed codeword maximum: every character consumes six runs
            // and `char_value` returns None when the row runs out, so the
            // scan is already bounded by the image. The old `> 30` cap
            // silently truncated valid long GS1-128 payloads.
        }
    }

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        for y in 0..image.height {
            self.scan_row(image, y);
            if self.runs.len() < 25 {
                continue; // minimum: start + 1 data + check + stop
            }
            for start in 0..self.runs.len() {
                if !self.runs[start].0 {
                    continue;
                }
                let Some(c) = self.try_from(start) else { continue };
                let (count, exact, x0, x1, y0, y1, score) = {
                    let e = self
                        .votes
                        .entry(c.text.clone())
                        .or_insert((0, 0, c.x_start, c.x_end, y, y, f32::MAX));
                    e.0 += 1;
                    if c.exact {
                        e.1 += 1;
                    }
                    e.2 = e.2.min(c.x_start);
                    e.3 = e.3.max(c.x_end);
                    e.4 = e.4.min(y);
                    e.5 = e.5.max(y);
                    e.6 = e.6.min(c.score);
                    *e
                };
                // A bit-exact read confirms as fast as it always did; a
                // tolerant (fuzzy) one is not allowed to preempt an exact
                // read that a later row may still produce — on the field
                // receipt a systematically misread band beat the exact
                // band on row count, with a checksum coincidence.
                if count >= self.min_rows && exact >= self.min_rows {
                    if std::env::var("CODE128_DBG").is_ok() {
                        eprintln!(
                            "CODE128 early {0:?} rows={count} exact={exact} score={score:.3} x={x0}..{x1} y={y0}..{y1}",
                            c.text
                        );
                    }
                    return Some(self.finish(&c.text, x0, x1, y0, y1));
                }
            }
        }
        if std::env::var("CODE128_DBG").is_ok() {
            for (t, v) in &self.votes {
                eprintln!(
                    "CODE128 vote {t:?} rows={} exact={} score={:.3} x={}..{} y={}..{}",
                    v.0, v.1, v.6, v.2, v.3, v.4, v.5
                );
            }
        }
        // Prefer an exact candidate (the pre-tolerance standard); fuzzy is
        // a strict fallback: the best-fitting sequence wins, not the most
        // rows. A systematic misread band can outnumber clean rows while
        // fitting the pattern table far worse; the checksum alone cannot
        // separate two 1-in-103 coincidences.
        let mut pick: Option<(&String, &(usize, usize, u32, u32, u32, u32, f32))> = None;
        for (t, v) in &self.votes {
            if v.0 >= self.min_rows && v.1 >= self.min_rows {
                let better = pick.map_or(true, |(_, p)| (v.1, v.0) > (p.1, p.0));
                if better {
                    pick = Some((t, v));
                }
            }
        }
        if pick.is_none() {
            for (t, v) in &self.votes {
                if v.0 >= self.min_rows {
                    let better = pick.map_or(true, |(_, p)| v.6 < p.6);
                    if better {
                        pick = Some((t, v));
                    }
                }
            }
        }
        pick.map(|(t, v)| self.finish(t, v.2, v.3, v.4, v.5))
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::Code128,
            text: text.to_string(),
            raw: text.as_bytes().to_vec(),
            position: [
                Point { x: x0 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y1 as f32 },
                Point { x: x0 as f32, y: y1 as f32 },
            ],
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Set {
    A,
    B,
    C,
}

/// mod-103 checksum: (start + Σ i·value_i) mod 103 over the data values.
fn check_valid(start: u8, data: &[u8], check: u8) -> bool {
    let mut sum = start as usize;
    for (i, &v) in data.iter().enumerate() {
        sum += (i + 1) * v as usize;
    }
    sum % 103 == check as usize
}

/// Render data values to text through the code-set state machine.
/// FNC1 becomes GS (0x1D); FNC2/FNC3/FNC4 are dropped.
fn build_text(start: u8, data: &[u8]) -> String {
    let mut set = match start {
        103 => Set::A,
        104 => Set::B,
        _ => Set::C,
    };
    let mut shift: Option<Set> = None;
    let mut out = String::with_capacity(data.len() * 2);
    for &v in data {
        let s = shift.take().unwrap_or(set);
        match (s, v) {
            // code-set switches / shifts / function codes
            (Set::A, 99) | (Set::B, 99) => set = Set::C,
            (Set::A, 100) | (Set::C, 100) => set = Set::B,
            (Set::B, 101) | (Set::C, 101) => set = Set::A,
            (Set::A, 98) => shift = Some(Set::B),
            (Set::B, 98) => shift = Some(Set::A),
            (_, 102) => out.push('\u{1d}'), // FNC1 -> GS
            // FNC2 (97), FNC3 (96): no text (set C has no function codes —
            // its 96/97 are digit pairs). FNC4 (100 in B, 101 in A): dropped.
            (Set::B, 100) | (Set::A, 101) | (Set::A, 96 | 97) | (Set::B, 96 | 97) => {}
            // data
            (Set::A, v) if v <= 95 => {
                out.push(if v < 64 { (v + 32) as char } else { (v - 64) as char })
            }
            (Set::A, _) => {}
            (Set::B, v) => out.push((v + 32) as char),
            (Set::C, v) => {
                out.push((b'0' + v / 10) as char);
                out.push((b'0' + v % 10) as char);
            }
        }
    }
    out
}

impl SymbologyDecoder for Code128Decoder {
    fn symbology(&self) -> Symbology {
        Symbology::Code128
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        self.decode_impl(image).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_table_is_well_formed() {
        for (i, p) in PATTERNS.iter().enumerate() {
            assert_eq!(p.iter().sum::<u8>(), 11, "pattern {i} sums to 11");
            assert!(p.iter().all(|&w| (1..=4).contains(&w)), "pattern {i} widths 1-4");
        }
        // all distinct
        for i in 0..PATTERNS.len() {
            for j in i + 1..PATTERNS.len() {
                assert_ne!(PATTERNS[i], PATTERNS[j], "patterns {i} and {j} identical");
            }
        }
        assert_eq!(STOP7.iter().sum::<u8>(), 13);
    }

    fn decode_canvas(canvas: &barcode_fixtures::Canvas, min_rows: usize) -> Option<String> {
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = Code128Decoder::new(min_rows);
        d.decode(&img).first().map(|r| r.text.clone())
    }

    #[test]
    fn round_trip_code_c_digits() {
        for module in [2u32, 3, 4] {
            let canvas = barcode_fixtures::encode_code128("80013890976003464630", module, 30);
            assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some("80013890976003464630"));
        }
    }

    #[test]
    fn round_trip_code_b_alnum() {
        let canvas = barcode_fixtures::encode_code128("AB12-x9", 3, 30);
        assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some("AB12-x9"));
    }

    #[test]
    fn long_payload_round_trip() {
        for text in ["ROTATED-CODE128", "Code 128!"] {
            let canvas = barcode_fixtures::encode_code128(text, 4, 120);
            assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some(text), "{text}");
        }
        // Longer than the old 30-value cap (~28 data codewords).
        let long = "A".repeat(64);
        let canvas = barcode_fixtures::encode_code128(&long, 3, 30);
        assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some(long.as_str()));
    }

    #[test]
    fn tolerant_widths_recover_a_smeared_bar() {
        // Module 6, so a 4px erosion is 0.67 modules — enough to flip the
        // start bar's rounding (2 -> 1) and defeat the exact table lookup,
        // while the closest-pattern path still reads it. This is the field
        // receipt's damage model (bars widened/eroded by the print).
        let clean = barcode_fixtures::encode_code128("AB12-x9", 6, 30);
        assert_eq!(decode_canvas(&clean, 2).as_deref(), Some("AB12-x9"));

        let mut worn = barcode_fixtures::encode_code128("AB12-x9", 6, 30);
        let quiet = 10 * 6;
        // The start value 104's first element is a 2-module bar.
        worn.clear(quiet + 2 * 6 - 4, 0, 4, 30);
        assert_eq!(decode_canvas(&worn, 2).as_deref(), Some("AB12-x9"));
    }

    #[test]
    fn rejects_broken_stop() {
        // Wipe the stop pattern's final bar across all rows: the symbol must
        // not decode at all.
        let canvas = barcode_fixtures::encode_code128("12345678", 3, 30);
        let mut canvas = canvas;
        let x0 = canvas.width - 10 * 3 - 2 * 3; // end of trailing quiet - final bar
        canvas.clear(x0, 0, 2 * 3, 30);
        assert_eq!(decode_canvas(&canvas, 2), None);
    }
}
