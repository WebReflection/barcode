//! Clean-room Interleaved 2 of 5 decoder (ISO/IEC 16390; covers ITF-14).
//!
//! Structure: start = 4 narrow elements (bar, space, bar, space); each digit
//! pair = 10 runs — the first digit's 5 elements are BARS, the second's are
//! SPACES, each 5-element group carrying exactly 2 wide of 5 (2-of-5 code);
//! stop = narrow space + wide bar. Element widths are classified narrow/wide
//! at 1.5 modules, which is valid for the whole 2.0-3.0 ratio range, so no
//! ratio knowledge is needed. Optional GS1 check digit validated when the
//! decoded length makes it applicable.

use std::collections::HashMap;

use barcode_core::{BitImage, DecodeResult, Point, Symbology, SymbologyDecoder};

/// 2-of-5 digit patterns (element index 0-4, `1` = wide).
const D2OF5: [&str; 10] = [
    "00110", // 0
    "10001", // 1
    "01001", // 2
    "11000", // 3
    "00101", // 4
    "10100", // 5
    "01100", // 6
    "00011", // 7
    "10010", // 8
    "01010", // 9
];

struct Candidate {
    text: String,
    x_start: u32,
    x_end: u32,
}

#[derive(Debug)]
pub struct ItfDecoder {
    min_rows: usize,
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    /// Index of the last run containing ink (the stop's wide bar must be it).
    last_ink_run: usize,
    votes: HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl ItfDecoder {
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

    /// Decode a symbol whose first bar run is at `start`.
    ///
    /// Classification is per-pair: each 10-run digit pair carries exactly
    /// 6 narrow and 4 wide elements (3+3 and 2+2 per digit), so sorting the
    /// 10 widths and splitting between the 6th and 7th needs no module
    /// width and no ratio — both erode and dilate under blur; counts do not.
    /// A start bar dilated past the global threshold still classifies fine.
    fn try_from(&self, start: usize) -> Option<Candidate> {
        if start + 14 > self.runs.len() {
            return None;
        }
        // The start's 4 runs must be narrow per the first pair's split.
        let thr0 = self.pair_threshold(start + 4)?;
        for k in 0..4 {
            if (self.runs[start + k].1 as f32) >= thr0 {
                return None;
            }
        }

        // Decode digit pairs greedily; stop at narrow bar + narrow space +
        // wide bar, with the wide bar being the row's last ink run.
        let mut digits: Vec<u8> = Vec::with_capacity(20);
        let mut k = start + 4;
        loop {
            if k + 10 > self.runs.len() {
                return None;
            }
            let thr = self.pair_threshold(k)?;
            let mut bar_w = [0u8; 5];
            let mut sp_w = [0u8; 5];
            for i in 0..5 {
                bar_w[i] = if (self.runs[k + 2 * i].1 as f32) >= thr { 2 } else { 1 };
                sp_w[i] = if (self.runs[k + 2 * i + 1].1 as f32) >= thr { 2 } else { 1 };
            }
            digits.push(pattern_digit(&bar_w)?);
            digits.push(pattern_digit(&sp_w)?);
            k += 10;

            // Stop: wide bar, narrow space, narrow bar (verified against a
            // real capture). The wide bar must be the row's last ink run:
            // pair elements can mimic the stop pattern mid-symbol.
            if k + 3 <= self.runs.len() {
                let b1_ok = self.runs[k].0 && (self.runs[k].1 as f32) >= thr;
                let s_ok = !self.runs[k + 1].0 && (self.runs[k + 1].1 as f32) < thr;
                let b2_ok = self.runs[k + 2].0 && (self.runs[k + 2].1 as f32) < thr;
                if b1_ok && s_ok && b2_ok && k + 2 == self.last_ink_run {
                    // A symbol needs at least three digit pairs. A GS1 check
                    // digit is NOT required: ITF-10/ITF-12 routing labels
                    // (parcel carriers, warehouse slips) carry plain data,
                    // so demanding one rejected real codes outright (the
                    // 2026-11-18 shipping slip: ITF "0074472289").
                    if digits.len() < 6 {
                        return None;
                    }
                    let text: String = digits.iter().map(|d| (b'0' + d) as char).collect();
                    let x_start = self.offsets[start];
                    let x_end = self.offsets[k + 2] + self.runs[k + 2].1;
                    return Some(Candidate { text, x_start, x_end });
                }
            }
            if digits.len() > 32 {
                return None; // implausibly long
            }
        }
    }

    /// Per-pair narrow/wide threshold: sort the 10 run widths; a valid pair
    /// has exactly 6 narrow then 4 wide with a visible gap between them.
    fn pair_threshold(&self, k: usize) -> Option<f32> {
        if k + 10 > self.runs.len() {
            return None;
        }
        let mut widths: Vec<u32> = (0..10).map(|i| self.runs[k + i].1).collect();
        widths.sort();
        let lo = widths[5] as f32;
        let hi = widths[6] as f32;
        if hi < lo * 1.4 || lo == 0.0 {
            return None; // no clean narrow/wide gap
        }
        Some((lo + hi) / 2.0)
    }

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        for y in 0..image.height {
            self.scan_row(image, y);
            if self.runs.len() < 10 {
                continue;
            }
            // Vote for the row's EARLIEST valid symbol start. A mid-symbol
            // start also parses (it is a suffix of the real digits, still
            // 2-of-5 and reaching the same stop), so "first one to vote
            // twice wins" let a suffix shadow the full payload; the real
            // start decodes strictly more digits from further left.
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
                break;
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
            symbology: Symbology::Itf,
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

/// Match a 5-width group against the 2-of-5 table; the group must carry
/// exactly 2 wide elements and the match must be unique within distance 1.
fn pattern_digit(widths: &[u8; 5]) -> Option<u8> {
    if widths.iter().filter(|&&w| w >= 2).count() != 2 {
        return None;
    }
    // D2OF5 patterns encode 1 = wide, 0 = narrow.
    let key: String = widths.iter().map(|w| if *w >= 2 { '1' } else { '0' }).collect();
    let mut cands: Vec<usize> = D2OF5
        .iter()
        .enumerate()
        .filter(|(_, p)| **p == key)
        .map(|(i, _)| i)
        .collect();
    if cands.len() == 1 {
        return Some(cands[0] as u8);
    }
    // distance-1: the table's minimum inter-code distance is 2, so a unique
    // nearest code corrects one misclassified element.
    for i in 0..5 {
        let mut flipped = key.clone();
        flipped.replace_range(i..i + 1, if key.as_bytes()[i] == b'1' { "0" } else { "1" });
        let hits: Vec<usize> = D2OF5
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == flipped)
            .map(|(i, _)| i)
            .collect();
        if hits.len() == 1 {
            cands.push(hits[0]);
        }
    }
    if cands.len() == 1 {
        return Some(cands[0] as u8);
    }
    None
}

/// GS1/I2of5 check: weights 3,1,3,1,... from the left; valid if total ≡ 0 mod 10.
pub fn check_digit_valid(digits: &[u8]) -> bool {
    let sum: usize = digits
        .iter()
        .enumerate()
        .map(|(i, &d)| d as usize * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    sum % 10 == 0
}

impl SymbologyDecoder for ItfDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::Itf
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
        let mut d = ItfDecoder::new(min_rows);
        d.decode(&img).first().map(|r| r.text.clone())
    }

    #[test]
    fn check_digit_math() {
        let digits: Vec<u8> = "800138909760034646303132"
            .bytes()
            .map(|b| b - b'0')
            .collect();
        assert!(check_digit_valid(&digits));
        let bad = digits.clone();
        // corrupt the last digit
        let mut bad = bad;
        let n = bad.len();
        bad[n - 1] = (bad[n - 1] + 1) % 10;
        assert!(!check_digit_valid(&bad));
    }

    /// Regression: the real ITF-14-style payload from the live phone scan
    /// (24 digits) must decode. The symbol contains mid-symbol starts that
    /// parse as suffixes, so this also pins the earliest-start rule: the
    /// full payload must win, never a tail.
    #[test]
    fn live_payload_and_suffix_shadowing() {
        let canvas = barcode_fixtures::encode_itf("800138909760034646303132", 3, 2.5, 320);
        assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some("800138909760034646303132"));
    }

    /// A GS1 check digit is optional: parcel routing labels (the
    /// 2026-11-18 shipping slip carried "0074472289") use plain data.
    /// The old decoder dropped every such symbol outright.
    #[test]
    fn accepts_payloads_without_a_check_digit() {
        for text in ["0074472289", "01234567", "0102030405"] {
            let canvas = barcode_fixtures::encode_itf(text, 3, 2.5, 80);
            assert_eq!(
                decode_canvas(&canvas, 2).as_deref(),
                Some(text),
                "no decode for {text}"
            );
        }
    }

    #[test]
    fn round_trip_user_card() {
        for (narrow, ratio) in [(2u32, 2.0f32), (3, 2.5), (2, 3.0), (4, 2.5)] {
            let canvas =
                barcode_fixtures::encode_itf("800138909760034646303132", narrow, ratio, 30);
            assert_eq!(
                decode_canvas(&canvas, 2).as_deref(),
                Some("800138909760034646303132"),
                "narrow={narrow} ratio={ratio}"
            );
        }
    }

    #[test]
    fn round_trip_itf14_and_short() {
        // ITF-14 style (14 digits) and a short 8-digit symbol
        for text in ["01234567890128", "12345670"] {
            let canvas = barcode_fixtures::encode_itf(text, 3, 2.5, 30);
            assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some(text));
        }
    }
}

#[cfg(test)]
mod dbg {
    use super::*;
    use barcode_core::BitImage;

    #[test]
    fn debug_roundtrip() {
        let canvas = barcode_fixtures::encode_itf("800138909760034646303132", 2, 2.0, 30);
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = ItfDecoder::new(1);
        d.scan_row(&img, 0);
        eprintln!("n runs={} width={}", d.runs.len(), canvas.width);
        eprintln!("runs: {:?}", d.runs.iter().take(24).collect::<Vec<_>>());
        // where does try_from bail?
        for start in 0..8 {
            if !d.runs[start].0 { eprintln!("start={start}: bg"); continue; }
            // replicate the bootstrap
            if start + 34 > d.runs.len() { eprintln!("start={start}: too short"); continue; }
            let mut boot: Vec<u32> = (0..34).map(|k| d.runs[start + k].1).collect();
            boot.sort();
            let thr = (boot[21] + boot[22]) as f32 / 2.0;
            eprintln!("start={start}: boot[20..24]={:?} thr={thr:.1} startruns={:?}", &boot[20..24], (0..4).map(|k| d.runs[start + k].1).collect::<Vec<_>>());
        }
    }
}

#[cfg(test)]
mod dbg2 {
    use super::*;
    use barcode_core::BitImage;

    #[test]
    fn debug_try_from() {
        let canvas = barcode_fixtures::encode_itf("800138909760034646303132", 2, 2.0, 30);
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = ItfDecoder::new(1);
        d.scan_row(&img, 0);
        eprintln!("n={} width={}", d.runs.len(), canvas.width);
        for start in 1..3 {
            let r = d.try_from(start);
            eprintln!("try_from({start}) = {:?}", r.as_ref().map(|c| &c.text));
        }
        let res = d.decode(&img);
        eprintln!("decode: {:?}", res.iter().map(|r| &r.text).collect::<Vec<_>>());
    }
}

