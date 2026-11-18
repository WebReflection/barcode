//! Clean-room Code 39 decoder (ISO/IEC 16388).
//!
//! Each character is 9 elements (5 bars, 4 spaces, bar/space alternating)
//! with exactly 3 wide; characters are separated by a narrow intercharacter
//! gap; '*'-framed; bidirectional. The wide/narrow ratio is 2.0..3.0, and
//! binarization dilates ink by ~1px relative to spaces, so elements are
//! classified per type (bars vs bars, spaces vs spaces) against each type's
//! own min/max midline.
//!
//! Majority-vote across rows with early exit. The optional mod-43 check
//! character is not yet verified (a future `verify_check_char` flag).

use std::collections::HashMap;

use barcode_core::{BitImage, DecodeResult, Point, Symbology, SymbologyDecoder};

/// Character -> wide element indices (element 0 = first bar; even = bar,
/// odd = space). Independent transcription; see crates/barcode-fixtures for
/// the element-string form. If a real-world symbol disagrees, the '*' row
/// is the only one with published ambiguity (candidates "146" vs "168").
const TABLE: [(&str, &str); 44] = [
    ("0", "346"), ("1", "038"), ("2", "238"), ("3", "023"), ("4", "348"), ("5", "034"),
    ("6", "234"), ("7", "368"), ("8", "036"), ("9", "236"), ("A", "058"), ("B", "258"),
    ("C", "025"), ("D", "458"), ("E", "045"), ("F", "245"), ("G", "568"), ("H", "056"),
    ("I", "256"), ("J", "456"), ("K", "078"), ("L", "278"), ("M", "027"), ("N", "478"),
    ("O", "047"), ("P", "247"), ("Q", "678"), ("R", "067"), ("S", "267"), ("T", "467"),
    ("U", "018"), ("V", "128"), ("W", "012"), ("X", "148"), ("Y", "014"), ("Z", "124"),
    ("-", "168"), (".", "016"), (" ", "126"), ("$", "135"), ("/", "137"), ("+", "157"),
    ("%", "357"), ("*", "146"),
];

struct Candidate {
    text: String,
    x_start: u32,
    x_end: u32,
}

#[derive(Debug)]
pub struct Code39Decoder {
    /// Rows that must agree on the same text before it is reported.
    min_rows: usize,
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    votes: HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl Code39Decoder {
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

    /// Match the 9-element character at `start`. Returns the char, the
    /// narrow-space width (for gap checks) and the element count consumed.
    fn match_char(&self, start: usize) -> Option<(char, u32)> {
        const N_ELEM: usize = 9;
        if self.runs.len() < start + N_ELEM {
            return None;
        }
        let lens: [u32; N_ELEM] = std::array::from_fn(|k| self.runs[start + k].1);
        let bar_lo = lens[0].min(lens[2]).min(lens[4]).min(lens[6]).min(lens[8]);
        let bar_hi = lens[0].max(lens[2]).max(lens[4]).max(lens[6]).max(lens[8]);
        let sp_lo = lens[1].min(lens[3]).min(lens[5]).min(lens[7]);
        let sp_hi = lens[1].max(lens[3]).max(lens[5]).max(lens[7]);

        // Every Code 39 char has >= 1 wide space; reject implausible scales.
        if sp_lo == 0 || sp_hi < sp_lo * 2 || sp_hi > sp_lo * 5 {
            return None;
        }
        let bar_mid = (bar_lo + bar_hi) as f32 / 2.0;
        let sp_mid = (sp_lo + sp_hi) as f32 / 2.0;

        let mut key = String::with_capacity(3);
        for (k, &len) in lens.iter().enumerate() {
            let mid = if k % 2 == 0 { bar_mid } else { sp_mid };
            if len as f32 > mid {
                key.push(char::from(b'0' + k as u8));
            }
        }
        let ch = TABLE.iter().find(|(_, idx)| *idx == key).map(|(c, _)| c.chars().next().unwrap());
        ch.map(|c| (c, sp_lo))
    }

    /// Decode from run index `start`; requires a leading '*' and a closing '*'.
    fn try_from(&self, start: usize) -> Option<Candidate> {
        let mut j = start;
        let mut text = String::new();
        let mut first = true;
        loop {
            let (ch, sp_lo) = self.match_char(j)?;
            if first {
                // The start sentinel is consumed, never part of the payload.
                if ch != '*' {
                    return None;
                }
                first = false;
            } else if ch == '*' {
                let x_start = self.offsets[start];
                let x_end =
                    self.offsets[j] + self.runs[j..].iter().take(9).map(|r| r.1).sum::<u32>();
                return if text.is_empty() {
                    None
                } else {
                    Some(Candidate { text, x_start, x_end })
                };
            } else {
                text.push(ch);
            }
            j += 9;
            // Intercharacter gap: a narrow background run.
            if j < self.runs.len() && !self.runs[j].0 && self.runs[j].1 <= sp_lo * 3 / 2 {
                j += 1;
            }
            if j + 9 > self.runs.len() {
                return None;
            }
        }
    }

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        for y in 0..image.height {
            self.scan_row(image, y);
            if self.runs.len() < 11 {
                continue;
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
            symbology: Symbology::Code39,
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

impl SymbologyDecoder for Code39Decoder {
    fn symbology(&self) -> Symbology {
        Symbology::Code39
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        self.decode_impl(image).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_canvas(canvas: &barcode_fixtures::Canvas, min_rows: usize) -> Option<String> {
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = Code39Decoder::new(min_rows);
        d.decode(&img).first().map(|r| r.text.clone())
    }

    #[test]
    fn table_has_three_wide_each() {
        for (_, idx) in TABLE {
            assert_eq!(idx.len(), 3, "{idx}");
        }
    }

    #[test]
    fn round_trip_basic() {
        for text in ["GMMNDR78E17A271Q", "0404804131807", "ABC-123"] {
            for (narrow, ratio) in [(2u32, 2.0f32), (3, 2.5), (2, 3.0), (4, 2.5)] {
                let canvas = barcode_fixtures::encode_code39(text, narrow, ratio, 30);
                assert_eq!(
                    decode_canvas(&canvas, 2).as_deref(),
                    Some(text),
                    "narrow={narrow} ratio={ratio} text={text}"
                );
            }
        }
    }

    #[test]
    fn round_trip_all_chars() {
        // '*' is the reserved start/stop sentinel and cannot appear in data.
        let all: String =
            TABLE.iter().map(|(c, _)| *c).filter(|c| *c != "*").collect();
        let canvas = barcode_fixtures::encode_code39(&all, 3, 2.5, 30);
        assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some(all.as_str()));
    }
}
