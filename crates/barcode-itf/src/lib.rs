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

use barcode_core::{
    decode_linear_single, BitImage, DecodeResult, LinearDecoder, Point, RowRuns, Symbology,
    SymbologyDecoder,
};

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

/// The same table as 5-bit masks (bit i set = element i is wide), derived
/// at compile time so `pattern_digit` never allocates.
const D2OF5_BITS: [u8; 10] = {
    let mut out = [0u8; 10];
    let mut i = 0;
    while i < D2OF5.len() {
        let p = D2OF5[i].as_bytes();
        let mut mask = 0u8;
        let mut j = 0;
        while j < p.len() {
            if p[j] == b'1' {
                mask |= 1 << j;
            }
            j += 1;
        }
        out[i] = mask;
        i += 1;
    }
    out
};

struct Candidate {
    text: String,
    x_start: u32,
    x_end: u32,
}

#[derive(Debug)]
pub struct ItfDecoder {
    min_rows: usize,
    votes: HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl ItfDecoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            votes: HashMap::new(),
        }
    }

    /// Decode a symbol whose first bar run is at `start`.
    fn try_from(row: &RowRuns, start: usize) -> Option<Candidate> {
        let runs = &row.runs;
        if start + 14 > runs.len() {
            return None;
        }
        // Exact pre-filter before sorting 10 runs: every start run must be
        // < thr0, and the first digit pair has 4 wide runs >= hi > thr0, so
        // at least 4 of runs[start+4..start+14] must exceed max(start runs).
        let s_max = runs[start]
            .1
            .max(runs[start + 1].1)
            .max(runs[start + 2].1)
            .max(runs[start + 3].1);
        let mut wider = 0u32;
        for i in 4..14 {
            if runs[start + i].1 > s_max {
                wider += 1;
            }
        }
        if wider < 4 {
            return None;
        }
        // The start's 4 runs must be narrow per the first pair's split.
        let (thr0, _) = Self::pair_threshold(runs, start + 4)?;
        for k in 0..4 {
            if (runs[start + k].1 as f32) >= thr0 {
                return None;
            }
        }

        let mut digits: Vec<u8> = Vec::with_capacity(20);
        let mut k = start + 4;
        loop {
            if k + 10 > runs.len() {
                return None;
            }
            let (thr, narrow) = Self::pair_threshold(runs, k)?;
            let mut bar_w = [0u8; 5];
            let mut sp_w = [0u8; 5];
            for i in 0..5 {
                bar_w[i] = if (runs[k + 2 * i].1 as f32) >= thr {
                    2
                } else {
                    1
                };
                sp_w[i] = if (runs[k + 2 * i + 1].1 as f32) >= thr {
                    2
                } else {
                    1
                };
            }
            digits.push(pattern_digit(&bar_w)?);
            digits.push(pattern_digit(&sp_w)?);
            k += 10;

            if k + 3 <= runs.len() {
                let b1_ok = runs[k].0 && (runs[k].1 as f32) >= thr;
                let s_ok = !runs[k + 1].0 && (runs[k + 1].1 as f32) < thr;
                let b2_ok = runs[k + 2].0 && (runs[k + 2].1 as f32) < thr;
                let quiet = runs
                    .get(k + 3)
                    .map(|r| !r.0 && (r.1 as f32) >= 5.0 * narrow)
                    .unwrap_or(true);
                if b1_ok && s_ok && b2_ok && quiet {
                    if digits.len() < 6 {
                        return None;
                    }
                    let text: String = digits.iter().map(|d| (b'0' + d) as char).collect();
                    let x_start = row.offsets[start];
                    let x_end = row.offsets[k + 2] + runs[k + 2].1;
                    return Some(Candidate {
                        text,
                        x_start,
                        x_end,
                    });
                }
            }
            if digits.len() > 32 {
                return None;
            }
        }
    }

    /// Per-pair narrow/wide threshold: sort the 10 run widths; a valid pair
    /// has exactly 6 narrow then 4 wide with a visible gap between them.
    fn pair_threshold(runs: &[(bool, u32)], k: usize) -> Option<(f32, f32)> {
        if k + 10 > runs.len() {
            return None;
        }
        let mut widths = [0u32; 10];
        for (i, w) in widths.iter_mut().enumerate() {
            *w = runs[k + i].1;
        }
        widths.sort_unstable();
        let lo = widths[5] as f32;
        let hi = widths[6] as f32;
        if hi < lo * 1.4 || lo == 0.0 {
            return None;
        }
        Some(((lo + hi) / 2.0, lo))
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::Itf,
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

impl LinearDecoder for ItfDecoder {
    fn reset(&mut self) {
        self.votes.clear();
    }

    fn scan_line(&mut self, row: &RowRuns, y: u32) -> Option<DecodeResult> {
        let runs = &row.runs;
        if runs.len() < 10 {
            return None;
        }
        debug_assert!(!runs[0].0);
        for start in (1..runs.len()).step_by(2) {
            let Some(c) = Self::try_from(row, start) else {
                continue;
            };
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

/// Match a 5-width group against the 2-of-5 table; the group must carry
/// exactly 2 wide elements and the match must be unique within distance 1.
fn pattern_digit(widths: &[u8; 5]) -> Option<u8> {
    if widths.iter().filter(|&&w| w >= 2).count() != 2 {
        return None;
    }
    // D2OF5 patterns encode 1 = wide, 0 = narrow.
    let mut key = 0u8;
    for (i, &w) in widths.iter().enumerate() {
        if w >= 2 {
            key |= 1 << i;
        }
    }
    if let Some(i) = D2OF5_BITS.iter().position(|&p| p == key) {
        return Some(i as u8);
    }
    // distance-1: the table's minimum inter-code distance is 2, so a unique
    // nearest code corrects one misclassified element.
    let mut found: Option<u8> = None;
    for i in 0..5 {
        if let Some(j) = D2OF5_BITS.iter().position(|&p| p == key ^ (1 << i)) {
            if found.is_some() {
                return None;
            }
            found = Some(j as u8);
        }
    }
    found
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
        decode_linear_single(self, image)
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
        assert_eq!(
            decode_canvas(&canvas, 2).as_deref(),
            Some("800138909760034646303132")
        );
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

    /// A label border, printed rule or seal often shares the barcode's
    /// rows. The stop is identified by the quiet zone after it, not by
    /// being the row's last ink run: the old rule rejected every real
    /// capture with anything printed beside the symbol (Dompé pharma
    /// box, ITF-10 "0122508903").
    #[test]
    fn ink_beside_the_symbol_does_not_hide_the_stop() {
        let mut canvas = barcode_fixtures::encode_itf("0122508903", 3, 2.5, 120);
        // A 2px border column after the encoder's 10-module quiet zone.
        canvas.fill(canvas.width - 2, 0, 2, canvas.height);
        assert_eq!(decode_canvas(&canvas, 2).as_deref(), Some("0122508903"));
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
        let mut row = RowRuns::with_capacity(256);
        row.scan_row(&img, 0);
        eprintln!("n runs={} width={}", row.runs.len(), canvas.width);
        eprintln!("runs: {:?}", row.runs.iter().take(24).collect::<Vec<_>>());
        for start in 0..8 {
            if !row.runs[start].0 {
                eprintln!("start={start}: bg");
                continue;
            }
            if start + 34 > row.runs.len() {
                eprintln!("start={start}: too short");
                continue;
            }
            let mut boot: Vec<u32> = (0..34).map(|k| row.runs[start + k].1).collect();
            boot.sort();
            let thr = (boot[21] + boot[22]) as f32 / 2.0;
            eprintln!(
                "start={start}: boot[20..24]={:?} thr={thr:.1} startruns={:?}",
                &boot[20..24],
                (0..4).map(|k| row.runs[start + k].1).collect::<Vec<_>>()
            );
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
        let mut row = RowRuns::with_capacity(256);
        row.scan_row(&img, 0);
        eprintln!("n={} width={}", row.runs.len(), canvas.width);
        for start in 1..3 {
            let r = ItfDecoder::try_from(&row, start);
            eprintln!("try_from({start}) = {:?}", r.as_ref().map(|c| &c.text));
        }
        let res = d.decode(&img);
        eprintln!(
            "decode: {:?}",
            res.iter().map(|r| &r.text).collect::<Vec<_>>()
        );
    }
}
