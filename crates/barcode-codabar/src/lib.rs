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

use barcode_core::{BitImage, DecodeResult, Point, Symbology, SymbologyDecoder};

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
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    last_ink_run: usize,
    votes: HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl CodabarDecoder {
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

    /// Classify one character's 7 element widths (runs[k..k+7]) into a
    /// narrow/wide bit pattern. Returns (bars, spaces) on a clean split.
    fn classify(&self, k: usize) -> Option<([u8; 4], [u8; 3])> {
        if k + 7 > self.runs.len() {
            return None;
        }
        // Bars and spaces must alternate starting with a bar.
        for i in 0..7 {
            if self.runs[k + i].0 != (i % 2 == 0) {
                return None;
            }
        }
        let mut widths: Vec<u32> = (0..7).map(|i| self.runs[k + i].1).collect();
        widths.sort();
        // 2-wide split: six narrow then two wide. 3-wide: four then three.
        let splits = [
            (5usize, 2usize), // index of first wide, count of wide
            (4usize, 3usize),
        ];
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
                    bars[i] = (self.runs[k + 2 * i].1 as f32 >= thr) as u8;
                }
                for i in 0..3 {
                    spaces[i] = (self.runs[k + 2 * i + 1].1 as f32 >= thr) as u8;
                }
                result = Some((bars, spaces));
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
    fn try_from(&self, start: usize) -> Option<Candidate> {
        // First char must be a start/stop character.
        let (bars, spaces) = self.classify(start)?;
        let first = Self::lookup(bars, spaces)?;
        if !is_start_stop(first) {
            return None;
        }
        // Skip the start's 7 elements plus the inter-character space.
        let mut k = start + 8;
        let mut text = String::with_capacity(24);
        loop {
            let (bars, spaces) = self.classify(k)?;
            let c = Self::lookup(bars, spaces)?;
            if is_start_stop(c) {
                // The stop character must end the row's ink.
                if self.last_ink_run != k + 6 || text.is_empty() {
                    return None;
                }
                let x_start = self.offsets[start];
                let x_end = self.offsets[k + 6] + self.runs[k + 6].1;
                return Some(Candidate { text, x_start, x_end });
            }
            text.push(c);
            // Inter-character narrow space, then the next char's first bar.
            if k + 8 > self.runs.len() {
                return None;
            }
            let gap = self.runs[k + 7];
            if gap.0 {
                return None; // expected a space
            }
            // Narrow vs this char's own wide threshold: reuse the widest
            // narrow element of the char just decoded as an upper bound.
            let mut widths: Vec<u32> = (0..7).map(|i| self.runs[k + i].1).collect();
            widths.sort();
            if (gap.1 as f32) >= widths[4] as f32 * 1.4 {
                return None; // gap too wide for an inter-character space
            }
            k += 8;
            if text.len() > 48 {
                return None;
            }
        }
    }

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        for y in 0..image.height {
            self.scan_row(image, y);
            if self.runs.len() < 16 {
                continue; // start + 1 data + stop + quiets
            }
            for start in 0..self.runs.len() {
                if !self.runs[start].0 {
                    continue;
                }
                let Some(c) = self.try_from(start) else { continue };
                // Scanning hygiene, not spec: a payload under 4 characters
                // is noise. Loose start/stop pairs on random texture produce
                // 1-2 character junk that otherwise fires onResults and
                // stops the scan before real symbologies get their frame.
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
        }
        self.votes
            .iter()
            .max_by_key(|(_, v)| v.0)
            .filter(|(_, v)| v.0 >= self.min_rows)
            .map(|(t, v)| self.finish(t, v.1, v.2, v.3, v.4))
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::Codabar,
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

impl SymbologyDecoder for CodabarDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::Codabar
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        self.decode_impl(image).into_iter().collect()
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
}

