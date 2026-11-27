//! Clean-room Codabar decoder (ANSI/AIM BC3-1995; aka NW-7, Rationalized
//! Codabar). Library barcodes, blood-bank forms, FedEx airbills.
//!
//! Structure: each character is 7 elements — 4 bars then 3 spaces,
//! interleaved B S B S B S B — classified narrow/wide (ratio 2.25-3.0),
//! separated from the next character by one narrow space. The symbol is
//! START + data + STOP where start/stop are A-D; their reversed element
//! patterns are invalid, which makes the direction unambiguous. No
//! mandatory check digit (the symbology is self-checking).
//!
//! Classification is per-character: the 7 element widths are sorted and
//! split either 5/2 (chars with 2 wide elements: digits, -, $) or 4/3
//! (chars with 3 wide: : / . + and A-D); exactly one split shows a clean
//! gap. This is blur-tolerant in the same way the ITF decoder is —
//! erosion and dilation shift widths but not counts.

use std::collections::HashMap;

use barcode_core::{
    decode_linear_single, BitImage, DecodeResult, LinearDecoder, Point, RowRuns, Symbology,
    SymbologyDecoder,
};

/// (bars, spaces) wide-bit patterns per character, transcribed from the
/// published Codabar table. Bars are a 4-bit pattern B1..B4, spaces a
/// 3-bit pattern S1..S3; the element sequence is B1 S1 B2 S2 B3 S3 B4.
/// A-D are the start/stop characters.
#[rustfmt::skip]
const CHARS: [(char, [u8; 4], [u8; 3]); 20] = [
    ('0', [0, 0, 0, 1], [0, 0, 1]),
    ('1', [0, 0, 1, 0], [0, 0, 1]),
    ('4', [0, 1, 0, 0], [0, 0, 1]),
    ('5', [1, 0, 0, 0], [0, 0, 1]),
    ('2', [0, 0, 0, 1], [0, 1, 0]),
    ('-', [0, 0, 1, 0], [0, 1, 0]),
    ('$', [0, 1, 0, 0], [0, 1, 0]),
    ('9', [1, 0, 0, 0], [0, 1, 0]),
    ('6', [0, 0, 0, 1], [1, 0, 0]),
    ('7', [0, 0, 1, 0], [1, 0, 0]),
    ('8', [0, 1, 0, 0], [1, 0, 0]),
    ('3', [1, 0, 0, 0], [1, 0, 0]),
    ('C', [0, 0, 0, 1], [0, 1, 1]),
    ('D', [0, 0, 1, 0], [0, 1, 1]),
    ('A', [0, 1, 0, 0], [0, 1, 1]),
    ('B', [0, 0, 0, 1], [1, 1, 0]),
    ('.', [1, 1, 1, 0], [0, 0, 0]),
    ('/', [1, 1, 0, 1], [0, 0, 0]),
    (':', [1, 0, 1, 1], [0, 0, 0]),
    ('+', [0, 1, 1, 1], [0, 0, 0]),
];

fn is_start_stop(c: char) -> bool {
    matches!(c, 'A' | 'B' | 'C' | 'D')
}

struct Candidate {
    text: String,
    x_start: u32,
    x_end: u32,
}

#[derive(Debug)]
pub struct CodabarDecoder {
    min_rows: usize,
    votes: HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl CodabarDecoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            votes: HashMap::new(),
        }
    }

    /// Classify one character's 7 element widths (runs[k..k+7]) into a
    /// narrow/wide bit pattern. Returns `(bars, spaces, widest_narrow)` on
    /// a clean split so `try_from` can check the inter-character gap without
    /// sorting the 7 runs a second time.
    fn classify(runs: &[(bool, u32)], k: usize) -> Option<([u8; 4], [u8; 3], u32)> {
        if k + 7 > runs.len() {
            return None;
        }
        // Runs strictly alternate, so checking runs[k].0 proves bar/space alternation.
        if !runs[k].0 {
            return None;
        }
        let mut widths = [0u32; 7];
        for (i, w) in widths.iter_mut().enumerate() {
            *w = runs[k + i].1;
        }
        widths.sort_unstable();
        let splits = [(5usize, 2usize), (4usize, 3usize)];
        let mut result = None;
        let mut best_gap = 0.0f32;
        for &(first_wide, _n_wide) in &splits {
            let lo = widths[first_wide - 1] as f32;
            let hi = widths[first_wide] as f32;
            if lo == 0.0 || hi < lo * 1.4 {
                continue;
            }
            let gap = hi / lo;
            if gap > best_gap {
                best_gap = gap;
                let thr = (lo + hi) / 2.0;
                let mut bars = [0u8; 4];
                let mut spaces = [0u8; 3];
                for i in 0..4 {
                    bars[i] = (runs[k + 2 * i].1 as f32 >= thr) as u8;
                }
                for i in 0..3 {
                    spaces[i] = (runs[k + 2 * i + 1].1 as f32 >= thr) as u8;
                }
                result = Some((bars, spaces, widths[first_wide - 1]));
            }
        }
        result
    }

    fn lookup(bars: [u8; 4], spaces: [u8; 3]) -> Option<char> {
        CHARS
            .iter()
            .find(|(_, b, s)| *b == bars && *s == spaces)
            .map(|(c, _, _)| *c)
    }

    /// Decode a symbol whose first bar run is at `start`.
    fn try_from(row: &RowRuns, start: usize) -> Option<Candidate> {
        let runs = &row.runs;
        if start + 7 > runs.len() {
            return None;
        }
        // Exact pre-filter for Codabar start chars (A-D):
        // - b0 is ALWAYS narrow, and 1 of {b1, b2, b3} is wide (ratio >= 1.4)
        // - s1 (middle space) is ALWAYS wide, and 1 of {s0, s2} is narrow (ratio >= 1.4)
        let b0 = runs[start].1;
        let b1 = runs[start + 2].1;
        let b2 = runs[start + 4].1;
        let b3 = runs[start + 6].1;
        let b_hi = b1.max(b2).max(b3);
        let s0 = runs[start + 1].1;
        let s1 = runs[start + 3].1;
        let s2 = runs[start + 5].1;
        let s_lo = s0.min(s2);
        if b0 == 0 || s_lo == 0 || b_hi * 5 < b0 * 7 || s1 * 5 < s_lo * 7 {
            return None;
        }
        let (bars, spaces, start_w4) = Self::classify(runs, start)?;
        let first = Self::lookup(bars, spaces)?;
        if !is_start_stop(first) {
            return None;
        }
        // Left quiet zone: flush left edge or >= 5 narrow modules before start.
        if start > 1 && (runs[start - 1].0 || (runs[start - 1].1 as f32) < start_w4 as f32 * 5.0) {
            return None;
        }
        let mut k = start + 8;
        let mut text = String::with_capacity(24);
        loop {
            let (bars, spaces, w4) = Self::classify(runs, k)?;
            let c = Self::lookup(bars, spaces)?;
            if is_start_stop(c) {
                if text.is_empty() {
                    return None;
                }
                let quiet_right = k + 6 == row.last_ink_run
                    || runs
                        .get(k + 7)
                        .map(|r| !r.0 && (r.1 as f32) >= w4 as f32 * 5.0)
                        .unwrap_or(true);
                if !quiet_right {
                    return None;
                }
                let x_start = row.offsets[start];
                let x_end = row.offsets[k + 6] + runs[k + 6].1;
                return Some(Candidate {
                    text,
                    x_start,
                    x_end,
                });
            }
            text.push(c);
            if k + 8 > runs.len() {
                return None;
            }
            let gap = runs[k + 7];
            if gap.0 {
                return None;
            }
            if (gap.1 as f32) >= w4 as f32 * 1.4 {
                return None;
            }
            k += 8;
            if text.len() > 48 {
                return None;
            }
        }
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::Codabar,
            rows: 0,
            cols: 0,
            text: text.to_string(),
            raw: text.as_bytes().to_vec(),
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

impl LinearDecoder for CodabarDecoder {
    fn reset(&mut self) {
        self.votes.clear();
    }

    fn scan_line(&mut self, row: &RowRuns, y: u32) -> Option<DecodeResult> {
        let runs = &row.runs;
        if runs.len() < 16 {
            return None;
        }
        debug_assert!(!runs[0].0);
        for start in (1..runs.len()).step_by(2) {
            let Some(c) = Self::try_from(row, start) else {
                continue;
            };
            if c.text.len() < 4 {
                continue;
            }
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
        None
    }

    fn finish_votes(&self) -> Option<DecodeResult> {
        self.votes
            .iter()
            .max_by_key(|(_, v)| v.0)
            .filter(|(_, v)| v.0 >= self.min_rows)
            .map(|(t, v)| self.finish(t, v.1, v.2, v.3, v.4))
    }

    fn vote_count(&self) -> usize {
        if self.votes.is_empty() {
            0
        } else {
            self.votes.values().map(|v| v.0).sum()
        }
    }
}

impl SymbologyDecoder for CodabarDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::Codabar
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        decode_linear_single(self, image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_table_is_well_formed() {
        // All 20 patterns distinct; digits/-/$ have 1 wide bar + 1 wide
        // space; :/.+ have 3 wide bars, no wide spaces; A-D have 1 wide
        // bar + 2 adjacent wide spaces.
        let mut seen = Vec::new();
        for (c, bars, spaces) in CHARS {
            let key = (bars, spaces);
            assert!(!seen.contains(&key), "duplicate pattern for {c}");
            seen.push(key);
            let nw = bars.iter().filter(|&&b| b == 1).count();
            let sw = spaces.iter().filter(|&&s| s == 1).count();
            match c {
                '0'..='9' | '-' | '$' => {
                    assert_eq!((nw, sw), (1, 1), "{c}");
                }
                ':' | '/' | '.' | '+' => {
                    assert_eq!((nw, sw), (3, 0), "{c}");
                }
                'A' | 'B' | 'C' | 'D' => {
                    assert_eq!((nw, sw), (1, 2), "{c}");
                    let s = spaces;
                    assert!(
                        s == [0, 1, 1] || s == [1, 1, 0],
                        "{c} wide spaces must be adjacent"
                    );
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(seen.len(), 20);
    }

    fn decode_canvas(canvas: &barcode_fixtures::Canvas, min_rows: usize) -> Option<String> {
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = CodabarDecoder::new(min_rows);
        d.decode(&img).first().map(|r| r.text.clone())
    }

    #[test]
    fn round_trip_digits() {
        for narrow in [3u32, 4] {
            for ratio in [2.25f32, 2.5, 3.0] {
                let canvas = barcode_fixtures::encode_codabar("40156", narrow, ratio, 30);
                assert_eq!(
                    decode_canvas(&canvas, 2).as_deref(),
                    Some("40156"),
                    "narrow {narrow} ratio {ratio}"
                );
            }
        }
    }

    #[test]
    fn round_trip_specials() {
        let canvas = barcode_fixtures::encode_codabar("12.5-99/3+4:5$", 4, 2.5, 30);
        assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some("12.5-99/3+4:5$"));
    }

    #[test]
    fn rejects_broken_stop() {
        let canvas = barcode_fixtures::encode_codabar("12345", 4, 2.5, 30);
        let mut canvas = canvas;
        // wipe the final bar of the stop character
        let x0 = canvas.width - 10 * 4 - 4;
        canvas.clear(x0, 0, 4, 30);
        assert_eq!(decode_canvas(&canvas, 2), None);
    }

    #[test]
    fn ink_beside_symbol_does_not_hide_stop() {
        let mut canvas = barcode_fixtures::encode_codabar("40156", 4, 2.5, 30);
        // Simulate label borders printed outside both quiet zones.
        canvas.fill(2, 0, 4, 30);
        let border_x = canvas.width - 6;
        canvas.fill(border_x, 0, 4, 30);
        assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some("40156"));
    }
}
