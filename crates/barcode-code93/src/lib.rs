//! Clean-room Code 93 decoder (ANSI/AIM BC5; used by Canada Post).
//!
//! Structure: start (6 runs, value 47) + data chars + check C + check K
//! + stop (value 47) + termination bar (1 module). Every character is
//! 9 modules wide with 3 bars and 3 spaces (element widths 1-4), so the
//! unit is re-estimated per character from its own 6 runs (sum/9).
//! Two mandatory check digits: C = mod-47 weighted sum of the data
//! (weights 1..20 from the right), K = same over data + C (weights
//! 1..15). Verified against the published worked example TEST93 ->
//! C=41 '+', K=6.

use std::collections::HashMap;

use barcode_core::{BitImage, DecodeResult, Point, Symbology, SymbologyDecoder};

/// Element widths (bar, space, bar, space, bar, space) of values 0-47.
/// Value 47 is the start/stop character (binary 101011110). Transcribed
/// from the AIM spec table and cross-checked between two published
/// tables (Wikipedia, BarCodeIsland), each row validated against its
/// binary column.
#[rustfmt::skip]
const PATTERNS: [[u8; 6]; 48] = [
    [1,3,1,1,1,2], [1,1,1,2,1,3], [1,1,1,3,1,2], [1,1,1,4,1,1], [1,2,1,1,1,3], [1,2,1,2,1,2],
    [1,2,1,3,1,1], [1,1,1,1,1,4], [1,3,1,2,1,1], [1,4,1,1,1,1], [2,1,1,1,1,3], [2,1,1,2,1,2],
    [2,1,1,3,1,1], [2,2,1,1,1,2], [2,2,1,2,1,1], [2,3,1,1,1,1], [1,1,2,1,1,3], [1,1,2,2,1,2],
    [1,1,2,3,1,1], [1,2,2,1,1,2], [1,3,2,1,1,1], [1,1,1,1,2,3], [1,1,1,2,2,2], [1,1,1,3,2,1],
    [1,2,1,1,2,2], [1,3,1,1,2,1], [2,1,2,1,1,2], [2,1,2,2,1,1], [2,1,1,1,2,2], [2,1,1,2,2,1],
    [2,2,1,1,2,1], [2,2,2,1,1,1], [1,1,2,1,2,2], [1,1,2,2,2,1], [1,2,2,1,2,1], [1,2,3,1,1,1],
    [1,2,1,1,3,1], [3,1,1,1,1,2], [3,1,1,2,1,1], [3,2,1,1,1,1], [1,1,2,1,3,1], [1,1,3,1,2,1],
    [2,1,1,1,3,1], [1,2,1,2,2,1], [3,1,2,1,1,1], [3,1,1,1,2,1], [1,2,2,2,1,1], [1,1,1,1,4,1],
];
const STOP: [u8; 6] = [1, 1, 1, 1, 4, 1];
const START_STOP: u8 = 47;

struct Candidate {
    text: String,
    x_start: u32,
    x_end: u32,
}

#[derive(Debug)]
pub struct Code93Decoder {
    min_rows: usize,
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    last_ink_run: usize,
    votes: HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl Code93Decoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            runs: Vec::with_capacity(512),
            offsets: Vec::with_capacity(512),
            last_ink_run: 0,
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
        self.last_ink_run = 0;
        if n > 0 {
            self.runs.push((cur, n));
            self.offsets.push(image.width - n);
        }
        for (i, r) in self.runs.iter().enumerate() {
            if r.0 {
                self.last_ink_run = i;
            }
        }
    }

    /// Decode runs[k..k+6] against the pattern table using that
    /// character's own unit (6 widths sum to 9 modules).
    fn char_value(&self, k: usize) -> Option<(u8, f32)> {
        if k + 6 > self.runs.len() {
            return None;
        }
        let sum: u32 = (0..6).map(|i| self.runs[k + i].1).sum();
        let unit = sum as f32 / 9.0;
        if unit < 1.0 {
            return None;
        }
        let mut w = [0u8; 6];
        for i in 0..6 {
            let raw = self.runs[k + i].1 as f32 / unit;
            let m = raw.round();
            if !(1.0..=4.0).contains(&m) {
                return None;
            }
            w[i] = m as u8;
        }
        let v = PATTERNS.iter().position(|p| *p == w)? as u8;
        Some((v, unit))
    }

    /// Stop: the start/stop pattern followed by a 1-module termination
    /// bar which must be the row's last ink run.
    fn stop_at(&self, k: usize) -> bool {
        if k + 7 > self.runs.len() || k + 6 != self.last_ink_run {
            return false;
        }
        let sum: u32 = (0..6).map(|i| self.runs[k + i].1).sum();
        let unit = sum as f32 / 9.0;
        if unit < 1.0 {
            return false;
        }
        for (i, &m) in STOP.iter().enumerate() {
            // Even i are bars, odd i are spaces.
            if self.runs[k + i].0 != (i % 2 == 0) {
                return false;
            }
            let raw = self.runs[k + i].1 as f32 / unit;
            if (raw.round() as u8) != m {
                return false;
            }
        }
        let term = self.runs[k + 6];
        term.0 && (term.1 as f32 / unit).round() == 1.0
    }

    /// Decode a symbol whose first bar run is at `start`.
    fn try_from(&self, start: usize) -> Option<Candidate> {
        let (first, _) = self.char_value(start)?;
        if first != START_STOP {
            return None;
        }
        let mut values: Vec<u8> = Vec::with_capacity(24);
        let mut k = start + 6;
        loop {
            if values.len() >= 4 && self.stop_at(k) {
                // values = [start?, data..., C, K]; start is NOT in values.
                let k_expect = *values.last().unwrap();
                let c_expect = values[values.len() - 2];
                let data = &values[..values.len() - 2];
                if data.is_empty()
                    || c_check(data) != c_expect
                    || k_check(data, c_expect) != k_expect
                {
                    return None;
                }
                let text = build_text(data);
                let x_start = self.offsets[start];
                let x_end = self.offsets[k + 6] + self.runs[k + 6].1;
                return Some(Candidate { text, x_start, x_end });
            }
            let (v, _) = self.char_value(k)?;
            if v == START_STOP && !self.stop_at(k) {
                // A start/stop pattern in the data can only be the stop.
                return None;
            }
            values.push(v);
            k += 6;
            if values.len() > 60 {
                return None; // implausibly long
            }
        }
    }

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        for y in 0..image.height {
            self.scan_row(image, y);
            if self.runs.len() < 26 {
                continue; // minimum: start + 1 data + C + K + stop + term bar
            }
            for start in 0..self.runs.len() {
                if !self.runs[start].0 {
                    continue;
                }
                let Some(c) = self.try_from(start) else { continue };
                let (count, x0, x1, y0, y1) = {
                    let e = self
                        .votes
                        .entry(c.text.clone())
                        .or_insert((0, c.x_start, c.x_end, y, y));
                    e.0 += 1;
                    e.1 = e.1.min(c.x_start);
                    e.2 = e.2.max(c.x_end);
                    e.3 = e.3.min(y);
                    e.4 = e.4.max(y);
                    *e
                };
                if count >= self.min_rows {
                    return Some(self.finish(&c.text, x0, x1, y0, y1));
                }
            }
        }
        self.votes
            .iter()
            .max_by_key(|(_, v)| v.0)
            .filter(|(_, v)| v.0 >= self.min_rows)
            .map(|(t, v)| self.finish(t, v.1, v.2, v.3, v.4))
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::Code93,
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

/// C check: mod-47 weighted sum of the data values, weights 1..20
/// starting at 1 for the right-most data character.
fn c_check(data: &[u8]) -> u8 {
    let sum: usize = data
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &v)| v as usize * (i % 20 + 1))
        .sum();
    (sum % 47) as u8
}

/// K check: same over data + C, weights 1..15 with the C character
/// itself weighted 1.
fn k_check(data: &[u8], c: u8) -> u8 {
    let sum: usize = data
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &v)| v as usize * ((i + 1) % 15 + 1))
        .sum::<usize>()
        + c as usize;
    (sum % 47) as u8
}

fn basic_char(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        10..=35 => (b'A' + v - 10) as char,
        36 => '-',
        37 => '.',
        38 => ' ',
        39 => '$',
        40 => '/',
        41 => '+',
        42 => '%',
        // 43-46 are the shift prefixes; their own glyphs are "$%/+".
        43 => '$',
        44 => '%',
        45 => '/',
        46 => '+',
        _ => '?',
    }
}

/// Full ASCII pair (shift 43-46 + basic value) -> ASCII character.
/// Transcribed from the Extended Code 93 table.
fn pair_ascii(shift: u8, base: u8) -> Option<char> {
    let b = basic_char(base);
    let c = match shift {
        43 => match b {
            // ($)A..=Z carry U+0001..=U+001A, not the shifted letters.
            'A'..='Z' => (b as u32 - 'A' as u32) + 1,
            _ => return None,
        },
        44 => match b {
            'A'..='E' => 27 + (b as u32 - 'A' as u32),
            'F'..='J' => 59 + (b as u32 - 'F' as u32),
            'K'..='O' => 91 + (b as u32 - 'K' as u32),
            'P'..='S' => 123 + (b as u32 - 'P' as u32),
            'T' | 'X' | 'Y' | 'Z' => 127,
            'U' => 0,
            'V' => 64,
            'W' => 96,
            _ => return None,
        },
        45 => match b {
            'A'..='C' => 33 + (b as u32 - 'A' as u32),
            'F'..='J' => 38 + (b as u32 - 'F' as u32),
            'L' => 44,
            'Z' => 58,
            _ => return None,
        },
        46 => match b {
            'A'..='Z' => 97 + (b as u32 - 'A' as u32),
            _ => return None,
        },
        _ => return None,
    };
    char::from_u32(c)
}

/// Render data values to text, resolving Full ASCII shift pairs.
fn build_text(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let v = data[i];
        if (43..=46).contains(&v) && i + 1 < data.len() && data[i + 1] <= 42 {
            match pair_ascii(v, data[i + 1]) {
                Some(c) => out.push(c),
                None => {
                    out.push(basic_char(v));
                    out.push(basic_char(data[i + 1]));
                }
            }
            i += 2;
        } else {
            out.push(basic_char(v));
            i += 1;
        }
    }
    out
}

impl SymbologyDecoder for Code93Decoder {
    fn symbology(&self) -> Symbology {
        Symbology::Code93
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
            assert_eq!(p.iter().sum::<u8>(), 9, "pattern {i} sums to 9");
            assert!(p.iter().all(|&w| (1..=4).contains(&w)), "pattern {i} widths 1-4");
            // exactly 3 bars (even indices) whose widths pair with spaces:
            // bars and spaces must alternate as (bar, space) x3
        }
        let mut sorted = PATTERNS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 48, "all patterns distinct");
        assert_eq!(STOP, PATTERNS[47]);
    }

    /// Ground truth from the published worked example: TEST93 encodes to
    /// these exact per-character binaries, C = '+' (41), K = '6' (6).
    #[test]
    fn matches_published_worked_example() {
        let expect = [
            // start/stop + T E S T 9 3 + + ... per BarCodeIsland example
            "101011110", // start
            "110100110", // T
            "110010010", // E
            "110101100", // S
            "110100110", // T
            "100001010", // 9
            "101000010", // 3
            "101110110", // + (C = 41)
            "100100010", // 6 (K = 6)
            "101011110", // stop
        ];
        for (v, bin) in expect.iter().enumerate() {
            let want: [u8; 6] = match v {
                0 | 9 => STOP,
                1 => PATTERNS[29], // T
                2 => PATTERNS[14], // E
                3 => PATTERNS[28], // S
                4 => PATTERNS[29], // T
                5 => PATTERNS[9],  // 9
                6 => PATTERNS[3],  // 3
                7 => PATTERNS[41], // +
                8 => PATTERNS[6],  // 6
                _ => unreachable!(),
            };
            // binary -> widths
            let mut w = vec![];
            let mut cur = true;
            let mut n = 0;
            for c in bin.chars() {
                let ink = c == '1';
                if ink == cur {
                    n += 1;
                } else {
                    w.push(n);
                    cur = ink;
                    n = 1;
                }
            }
            w.push(n);
            assert_eq!(w, want, "pattern {v} binary {bin}");
        }
        // and the check digits
        let data: [u8; 6] = [29, 14, 28, 29, 9, 3];
        assert_eq!(c_check(&data), 41);
        assert_eq!(k_check(&data, 41), 6);
    }

    fn decode_canvas(canvas: &barcode_fixtures::Canvas, min_rows: usize) -> Option<String> {
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = Code93Decoder::new(min_rows);
        d.decode(&img).first().map(|r| r.text.clone())
    }

    #[test]
    fn round_trip_basic() {
        for module in [2u32, 3, 4] {
            let canvas = barcode_fixtures::encode_code93("TEST93", module, 30);
            assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some("TEST93"));
        }
    }

    #[test]
    fn round_trip_full_ascii() {
        // lowercase letters and punctuation need shift pairs
        let canvas = barcode_fixtures::encode_code93("Invoice-A1 (net 42)", 3, 30);
        assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some("Invoice-A1 (net 42)"));
    }

    /// The ($)A..=Z pair carries U+0001..=U+001A. The decoder used to map
    /// those to the shifted letters ('B'..='['), so control-byte payloads
    /// read back as garbage — the round-trip was broken, not the encoder.
    #[test]
    fn round_trip_control_bytes() {
        for text in ["\u{00}", "\u{01}", "\u{1a}", "\u{01}\u{1a}AB", "\u{0a}\u{0d}"] {
            let canvas = barcode_fixtures::encode_code93(text, 4, 30);
            assert_eq!(
                decode_canvas(&canvas, 2).as_deref(),
                Some(text),
                "{text:?}"
            );
        }
    }

    #[test]
    fn rejects_broken_termination_bar() {
        let canvas = barcode_fixtures::encode_code93("12345", 3, 30);
        let mut canvas = canvas;
        // wipe the final 1-module termination bar (just before trailing quiet)
        let x0 = canvas.width - 10 * 3 - 3;
        canvas.clear(x0, 0, 3, 30);
        assert_eq!(decode_canvas(&canvas, 2), None);
    }
}

#[cfg(test)]
mod dbg {
    use super::*;
    use barcode_core::BitImage;

    #[test]
    fn debug_trace() {
        let canvas = barcode_fixtures::encode_code93("TEST93", 3, 30);
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = Code93Decoder::new(1);
        d.scan_row(&img, 15);
        eprintln!("runs={} last_ink={}", d.runs.len(), d.last_ink_run);
        let (first, _) = d.char_value(1).unwrap();
        eprintln!("first={first}");
        let mut values = vec![];
        let mut k = 7usize;
        for _step in 0..14 {
            if values.len() >= 4 && d.stop_at(k) {
                eprintln!("STOP at k={k}");
                break;
            }
            match d.char_value(k) {
                Some((v, u)) => { eprintln!("k={k} v={v} unit={u:.2}"); values.push(v); }
                None => { eprintln!("k={k} char_value=None"); break; }
            }
            k += 6;
        }
        eprintln!("values={values:?}");
        if values.len() >= 6 {
            let data = &values[..values.len() - 2];
            eprintln!("c={} k={}", c_check(data), k_check(data, c_check(data)));
        }
    }
}
