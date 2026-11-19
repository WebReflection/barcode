//! GS1 DataBar Expanded (RSS Expanded) decoder.
//!
//! Bit layouts and the symbol-character table were measured from the zint
//! oracle (`tools/databar_char_table.py`, notes/databar-recon.md). The
//! symbol has a variable number of 17-module data characters separated by
//! 15-module blocks, a check character before the 15-module start pattern,
//! and four encodation methods:
//!
//! * `1`    — AI (01) + other AIs (40-bit GTIN body, then general purpose)
//! * `00`   — any AIs (general purpose only)
//! * `0100` — AI (01) + (3103): 40-bit body + 15-bit weight (0.001 kg)
//! * `0101` — AI (01) + (3202)/(3203): 40-bit body + 15-bit weight
//! * `01100`/`01101` — AI (01) + (392x)/(393x) price + general purpose
//! * `0111xxx` — AI (01) + (310x)/(320x) + date
//!
//! The check character is not validated: it carries the mod-211 checksum
//! and symbol length, but every data character is independently checked
//! against the measured width table and the field parser is exact.

use barcode_core::{BitImage, DecodeResult, Point, Symbology, SymbologyDecoder};
use std::sync::OnceLock;

use super::expanded_ai::AI_LENGTHS;
use super::expanded_tables::EXP_WIDTHS;

/// FNC1 separator used inside a decoded general-purpose element string.
const GS: char = '\u{1d}';
/// 16-bit date value that means "no date" in the weight+date methods.
const NO_DATE: u32 = 38_400;
/// ISO/IEC 646 punctuation in code order 232..=252.
const ISO646_PUNCT: &[u8] = b"!\"%&'()*+,-./:;<=>?_ ";
/// The alphanumeric punctuation in code order 58..=62.
const ALNUM_PUNCT: &[u8] = b"*,-./";

/// One row of a symbol: runs alternate by construction.
type Run = (bool, u32);

#[derive(Debug)]
pub struct DataBarExpandedDecoder {
    min_rows: usize,
    runs: Vec<Run>,
    offsets: Vec<u32>,
    votes: std::collections::HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl DataBarExpandedDecoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            runs: Vec::with_capacity(256),
            offsets: Vec::with_capacity(256),
            votes: std::collections::HashMap::new(),
        }
    }

    fn scan_row(&mut self, image: &BitImage, y: u32) {
        self.runs.clear();
        self.offsets.clear();
        let mut cur = false;
        let mut n = 0u32;
        let mut run_start = 0u32;
        for x in 0..image.width {
            let ink = image.ink(x, y);
            if ink == cur {
                n += 1;
            } else {
                self.runs.push((cur, std::mem::replace(&mut n, 1)));
                self.offsets.push(run_start);
                run_start = x;
                cur = ink;
            }
        }
        if n > 0 {
            self.runs.push((cur, n));
            self.offsets.push(image.width - n);
        }
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::DataBarExpanded,
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

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        for y in 0..image.height {
            self.scan_row(image, y);
            for start in 0..self.runs.len() {
                let Some((text, x_start, x_end)) = self.try_symbol(start) else {
                    continue;
                };
                let (count, x0, x1, y0, y1) = {
                    let e = self
                        .votes
                        .entry(text.clone())
                        .or_insert((0, x_start, x_end, y, y));
                    e.0 += 1;
                    e.1 = e.1.min(x_start);
                    e.2 = e.2.max(x_end);
                    e.3 = e.3.min(y);
                    e.4 = e.4.max(y);
                    (e.0, e.1, e.2, e.3, e.4)
                };
                if count >= self.min_rows {
                    return Some(self.finish(&text, x0, x1, y0, y1));
                }
            }
        }
        self.votes
            .iter()
            .max_by_key(|(_, v)| v.0)
            .filter(|(_, v)| v.0 >= self.min_rows)
            .map(|(t, v)| self.finish(t, v.1, v.2, v.3, v.4))
    }

    /// Try a symbol whose start pattern begins at run `start`.
    fn try_symbol(&self, start: usize) -> Option<(String, u32, u32)> {
        // START: space 1, bar 8, space 4, bar 1, space 1.
        if start + 5 > self.runs.len() {
            return None;
        }
        let start_runs = &self.runs[start..start + 5];
        let colors = [false, true, false, true, false];
        if start_runs.iter().zip(colors).any(|(r, c)| r.0 != c) {
            return None;
        }
        let unit = module_unit(start_runs, 15);
        if quantize(start_runs, unit)? != [1, 8, 4, 1, 1] {
            return None;
        }
        // Backwards: guard bar (1 module), then the check character. The
        // guard space merges with the quiet zone, so it is not inspected.
        if start < 9 || !self.runs[start - 9].0 {
            return None;
        }
        if quantize(&self.runs[start - 9..start - 8], unit)? != [1] {
            return None;
        }
        let check_runs = &self.runs[start - 8..start];
        let check_widths = quantize(check_runs, module_unit(check_runs, 17))?;
        if check_widths.iter().sum::<u32>() != 17 {
            return None;
        }
        // Forwards: [char][char][block] ..., with a final right guard.
        let mut values = Vec::new();
        let mut j = start + 5;
        while j + 8 <= self.runs.len() {
            let char_runs = &self.runs[j..j + 8];
            let Some(widths) = quantize(char_runs, module_unit(char_runs, 17)) else {
                break;
            };
            if widths.iter().sum::<u32>() != 17 {
                break;
            }
            let Some(value) = char_value(&widths) else {
                break;
            };
            values.push(value);
            j += 8;
            if values.len() % 2 == 0 && j + 5 <= self.runs.len() {
                // Optional 15-module block after each pair.
                if let Some(b) = quantize(&self.runs[j..j + 5], module_unit(&self.runs[j..j + 5], 15)) {
                    if b.iter().sum::<u32>() == 15 {
                        j += 5;
                    }
                }
            }
        }
        if values.len() < 3 {
            return None;
        }
        let bits = values_to_bits(&values);
        let Some(text) = decode_bits(&bits) else {
            return None;
        };
        let x_start = self.offsets[start - 9];
        let last = j.min(self.runs.len()) - 1;
        let x_end = self.offsets[last] + self.runs[last].1;
        Some((text, x_start, x_end))
    }
}

impl SymbologyDecoder for DataBarExpandedDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::DataBarExpanded
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        self.decode_impl(image).into_iter().collect()
    }
}

/// Module size from a run window and its known module count.
fn module_unit(runs: &[Run], modules: u32) -> f32 {
    let total: u32 = runs.iter().map(|r| r.1).sum();
    total as f32 / modules as f32
}

/// Quantize a run window to module widths. Requires at least 0.5 px per
/// module and clean rounding; returns `None` for any non-symbol window.
fn quantize(runs: &[Run], unit: f32) -> Option<Vec<u32>> {
    if unit <= 0.0 {
        return None;
    }
    let mut out = Vec::with_capacity(runs.len());
    for &(_, w) in runs {
        let m = (w as f32 / unit).round();
        if m < 1.0 || m > 9.0 || (w as f32 / unit - m).abs() > 0.45 {
            return None;
        }
        out.push(m as u32);
    }
    Some(out)
}

/// Pack eight element widths into the table key (one nibble each).
fn pack(widths: &[u32]) -> u32 {
    let mut key = 0u32;
    for &w in widths {
        key = (key << 4) | w;
    }
    key
}

/// Canonical width key -> symbol-character value. A character is valid in
/// either reading direction, so both the vector and its reverse are tried.
fn char_value(widths: &[u32]) -> Option<u32> {
    debug_assert_eq!(widths.len(), 8);
    let map = decode_map();
    let key = pack(widths);
    if let Ok(i) = map.binary_search_by_key(&key, |&(k, _)| k) {
        return Some(map[i].1 as u32);
    }
    let mut rev = [0u32; 8];
    for (i, &w) in widths.iter().rev().enumerate() {
        rev[i] = w;
    }
    let key = pack(&rev);
    map.binary_search_by_key(&key, |&(k, _)| k)
        .ok()
        .map(|i| map[i].1 as u32)
}

fn decode_map() -> &'static Vec<(u32, u16)> {
    static MAP: OnceLock<Vec<(u32, u16)>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut v: Vec<(u32, u16)> = EXP_WIDTHS
            .iter()
            .enumerate()
            .map(|(i, &k)| (k, i as u16))
            .collect();
        v.sort_unstable();
        v
    })
}

fn values_to_bits(values: &[u32]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(values.len() * 12);
    for &v in values {
        for shift in (0..12).rev() {
            bits.push((v >> shift) & 1 == 1);
        }
    }
    bits
}

fn take(bits: &[bool], count: usize, at: usize) -> Option<u32> {
    if at + count > bits.len() {
        return None;
    }
    let mut v = 0u32;
    for &b in &bits[at..at + count] {
        v = (v << 1) | u32::from(b);
    }
    Some(v)
}

fn bits_eq(bits: &[bool], at: usize, pattern: &[bool]) -> bool {
    at + pattern.len() <= bits.len() && &bits[at..at + pattern.len()] == pattern
}

/// Decode the general-purpose data compaction field. Returns the raw GS1
/// element string with FNC1 as `\x1d`; trailing FNC1 padding is removed.
fn decode_general(bits: &[bool], start: usize) -> Option<String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Numeric,
        Alnum,
        Iso646,
    }
    let mut out = String::new();
    let mut pos = start;
    let mut mode = Mode::Numeric;
    while pos < bits.len() {
        match mode {
            Mode::Numeric => {
                if pos + 7 <= bits.len() && bits_eq(bits, pos, &[false, false, false, false]) {
                    pos += 4;
                    mode = Mode::Alnum;
                    continue;
                }
                if pos + 7 <= bits.len() {
                    let v = take(bits, 7, pos)?;
                    pos += 7;
                    if v < 8 {
                        out.push(GS);
                        continue;
                    }
                    let d1 = (v - 8) / 11;
                    let d2 = (v - 8) % 11;
                    if d1 == 10 {
                        out.push(GS);
                        if d2 != 10 {
                            out.push(char::from(b'0' + d2 as u8));
                        }
                    } else if d2 == 10 {
                        out.push(char::from(b'0' + d1 as u8));
                        out.push(GS);
                    } else {
                        out.push(char::from(b'0' + d1 as u8));
                        out.push(char::from(b'0' + d2 as u8));
                    }
                } else {
                    let Some(v) = take(bits, 4, pos) else { break };
                    pos += 4;
                    if v == 0 {
                        out.push(GS);
                    } else if v <= 10 {
                        out.push(char::from(b'0' + (v - 1) as u8));
                    } else {
                        return None;
                    }
                }
            }
            Mode::Alnum => {
                if bits_eq(bits, pos, &[false, false, false]) {
                    pos += 3;
                    mode = Mode::Numeric;
                    continue;
                }
                let Some(v5) = take(bits, 5, pos) else { break };
                pos += 5;
                if v5 == 4 {
                    mode = Mode::Iso646;
                } else if (5..=14).contains(&v5) {
                    out.push(char::from(b'0' + (v5 - 5) as u8));
                } else if v5 == 15 {
                    out.push(GS);
                    mode = Mode::Numeric;
                } else {
                    pos -= 5;
                    let Some(v6) = take(bits, 6, pos) else { break };
                    pos += 6;
                    if (32..=57).contains(&v6) {
                        out.push(char::from(b'A' + (v6 - 32) as u8));
                    } else if (58..=62).contains(&v6) {
                        out.push(ALNUM_PUNCT[(v6 - 58) as usize] as char);
                    } else {
                        return None;
                    }
                }
            }
            Mode::Iso646 => {
                if bits_eq(bits, pos, &[false, false, false]) {
                    pos += 3;
                    mode = Mode::Numeric;
                    continue;
                }
                let Some(v5) = take(bits, 5, pos) else { break };
                pos += 5;
                if v5 == 4 {
                    mode = Mode::Alnum;
                } else if (5..=14).contains(&v5) {
                    out.push(char::from(b'0' + (v5 - 5) as u8));
                } else if v5 == 15 {
                    out.push(GS);
                    mode = Mode::Numeric;
                } else {
                    pos -= 5;
                    if pos + 7 <= bits.len() {
                        let v7 = take(bits, 7, pos).unwrap_or(0);
                        if (64..=89).contains(&v7) && pos + 7 <= bits.len() {
                            pos += 7;
                            out.push(char::from((v7 + 1) as u8));
                            continue;
                        }
                        if (90..=115).contains(&v7) && pos + 7 <= bits.len() {
                            pos += 7;
                            out.push(char::from((v7 + 7) as u8));
                            continue;
                        }
                    }
                    let Some(v8) = take(bits, 8, pos) else { break };
                    if !(232..=252).contains(&v8) {
                        return None;
                    }
                    pos += 8;
                    out.push(ISO646_PUNCT[(v8 - 232) as usize] as char);
                }
            }
        }
    }
    while out.ends_with(GS) {
        out.pop();
    }
    Some(out)
}

/// Split an element string into `(AI)data` fields using the GS1 AI table.
/// Fixed-length fields are consumed by the table; variable-length fields
/// end at the next FNC1 (or the end of the string).
fn split_element_string(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len() + 16);
    let mut i = 0;
    while i < raw.len() {
        if raw.as_bytes()[i] == GS as u8 {
            i += 1;
            continue;
        }
        let mut found = None;
        for digits in [2usize, 3, 4] {
            let end = i + digits;
            if end > raw.len() || !raw[i..end].bytes().all(|b| b.is_ascii_digit()) {
                break;
            }
            let ai = raw[i..end].parse::<u16>().ok()?;
            if let Some(len) = ai_length(ai, digits as u8) {
                found = Some((digits, len));
                break;
            }
        }
        let (digits, len) = found?;
        out.push('(');
        out.push_str(&raw[i..i + digits]);
        out.push(')');
        i += digits;
        if len == 0 {
            let end = raw[i..].find(GS).map(|k| i + k).unwrap_or(raw.len());
            out.push_str(&raw[i..end]);
            i = end;
        } else {
            let end = i + len as usize;
            if end > raw.len() {
                return None;
            }
            out.push_str(&raw[i..end]);
            i = end;
        }
    }
    Some(out)
}

fn ai_length(ai: u16, digits: u8) -> Option<u8> {
    AI_LENGTHS
        .binary_search_by_key(&(ai, digits), |&(a, d, _)| (a, d))
        .ok()
        .map(|i| AI_LENGTHS[i].2)
}

/// Four 10-bit GTIN groups (3 decimal digits each) -> 13-digit body plus
/// the recomputed check digit.
fn gtin14(indicator: u32, groups: [u32; 4]) -> Option<String> {
    let mut body = format!("{indicator}");
    for g in groups {
        if g > 999 {
            return None;
        }
        body.push_str(&format!("{g:03}"));
    }
    let mut sum = 0usize;
    for (i, b) in body.bytes().enumerate() {
        sum += (b - b'0') as usize * if i % 2 == 0 { 3 } else { 1 };
    }
    let check = (10 - sum % 10) % 10;
    Some(format!("{body}{check}"))
}

/// Read the four 10-bit GTIN groups starting at `at`.
fn gtin_groups(bits: &[bool], at: usize) -> Option<[u32; 4]> {
    let mut groups = [0u32; 4];
    for (k, g) in groups.iter_mut().enumerate() {
        *g = take(bits, 10, at + 10 * k)?;
    }
    Some(groups)
}

fn weight_6(v: u32) -> String {
    format!("{v:06}")
}

fn decode_bits(bits: &[bool]) -> Option<String> {
    // Linkage bit (0), then the encodation method.
    if bits.len() < 5 || bits[0] {
        return None;
    }
    if bits[1] {
        // Method 1: 4-bit header, 4-bit indicator, 40-bit GTIN body.
        let indicator = take(bits, 4, 4)?;
        if indicator > 9 {
            return None;
        }
        let groups = gtin_groups(bits, 8)?;
        let mut text = format!("(01){}", gtin14(indicator, groups)?);
        let gp = decode_general(bits, 48)?;
        text.push_str(&split_element_string(&gp)?);
        return Some(text);
    }
    if !bits[2] {
        // Method 00: 5-bit header, general purpose only.
        let gp = decode_general(bits, 5)?;
        return split_element_string(&gp);
    }
    match take(bits, 4, 1)? {
        4 | 5 => {
            let method = take(bits, 4, 1)?;
            let groups = gtin_groups(bits, 5)?;
            let weight = take(bits, 15, 45)?;
            let gtin = gtin14(9, groups)?;
            if method == 4 {
                Some(format!("(01){gtin}(3103){}", weight_6(weight)))
            } else if weight < 10_000 {
                Some(format!("(01){gtin}(3202){}", weight_6(weight)))
            } else {
                Some(format!("(01){gtin}(3203){}", weight_6(weight - 10_000)))
            }
        }
        _ => match take(bits, 5, 1)? {
            12 | 13 => {
                let method = take(bits, 5, 1)?;
                let groups = gtin_groups(bits, 8)?;
                let x = take(bits, 2, 48)?;
                let gtin = gtin14(9, groups)?;
                let kind = if method == 12 { 2 } else { 3 };
                let mut text = format!("(01){gtin}(39{kind}{x})");
                if method == 13 {
                    let currency = take(bits, 10, 50)?;
                    text.push_str(&format!("{currency:03}"));
                    let price = decode_general(bits, 60)?;
                    text.push_str(&price);
                } else {
                    let price = decode_general(bits, 50)?;
                    text.push_str(&price);
                }
                Some(text)
            }
            _ => {
                let method = take(bits, 7, 1)?;
                if !(56..=63).contains(&method) {
                    return None;
                }
                let groups = gtin_groups(bits, 8)?;
                let weight = take(bits, 20, 48)?;
                let date = take(bits, 16, 68)?;
                let x = weight / 100_000;
                if x > 9 {
                    return None;
                }
                let prefix = if (method - 56) % 2 == 0 { 310 } else { 320 };
                let gtin = gtin14(9, groups)?;
                let mut text = format!(
                    "(01){gtin}({prefix}{x}){}",
                    weight_6(weight % 100_000)
                );
                if date != NO_DATE {
                    let day = date % 32;
                    let month = (date / 32) % 12 + 1;
                    let year = date / 384;
                    let ai = [11, 11, 13, 13, 15, 15, 17, 17][(method - 56) as usize];
                    text.push_str(&format!("({ai}){year:02}{month:02}{day:02}"));
                }
                Some(text)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draw a module row into a packed bitmap at `scale` pixels per module.
    fn decode_modules(mods: &str) -> Option<String> {
        let scale = 3u32;
        let margin = 6u32;
        let height = 3u32;
        let width = (mods.len() as u32 + 2 * margin) * scale;
        let bpr = (width.div_ceil(8).div_ceil(4) * 4) as usize;
        let mut data = vec![0u8; bpr * height as usize];
        for (i, ch) in mods.chars().enumerate() {
            if ch != '1' {
                continue;
            }
            for y in 0..height {
                for dx in 0..scale {
                    let x = (margin + i as u32) * scale + dx;
                    data[y as usize * bpr + (x >> 3) as usize] |= 1 << (x & 7);
                }
            }
        }
        let img = BitImage::new(width, height, &data);
        let mut d = DataBarExpandedDecoder::new(2);
        d.decode(&img).into_iter().next().map(|r| r.text)
    }

    #[test]
    fn compressed_methods_end_to_end() {
        // bardecode.com RSS Expanded examples (independent encoder), one
        // symbol per compressed encodation method.
        let cases = [
            ("enc0100", "10111001000001001101111111100001011100001001100010101111011100110001011110000001110010111000111011101111010111100011000111111000010101100001001111101", "(01)90012345678908(3103)001750"),
            ("enc0111000", "1001000100001110110111111110000101000110111000010010111100110111110101111110000111000101100001101110001111011010111100011111100001011000011010110000111001100110001001011111111001110001101111010000101", "(01)90012345678908(3103)012233(11)991231"),
            ("enc01100", "10110000010001011101111111100001010011100000101100101111001101111101011111100001110001011000011011100011110110101111000111111000010100111101110100001100011011100100010111111110011101", "(01)90012345678908(3922)795"),
            ("enc01101", "1000111101010000010111111110000101110100000110010010111100110111110101111110000111000101100001101110001111011010111100011111100001011100011001101100100111110001011101011111111001110001101111001011101", "(01)90012345678908(3932)0401234"),
            ("enc0111111", "1001100000101110010111111110000101111000100010100010111100110111110101111110000111000101100001101110001111011010111100011111100001011000011010110000111001100110001001011111111001110001101111010000101", "(01)90012345678908(3203)012233(17)991231"),
        ];
        for (name, mods, want) in cases {
            assert_eq!(decode_modules(mods).as_deref(), Some(want), "{name}");
        }
    }

    #[test]
    fn general_purpose_methods_end_to_end() {
        // zint BARCODE_RSS_EXP module strings (black-box oracle).
        let cases = [
            (
                "method 00 numeric",
                "010100000101000001101111111100001010000010000100110100111110001011101011110000000010111000011000110101",
                "(21)12345",
            ),
            (
                "method 00 multi-AI",
                "010100011000011100101111111100001010001000000010110101111110011110101011110000001110011100111110010100110110010111111000111111000010101001100111110001",
                "(10)ABC(21)123",
            ),
            (
                "method 1 + other AIs",
                "01000011001011000110111111110000101111010000100100010000101111101110101111100000011000101100000110010111100001101100100011111100001011100011001000010100111011111101101011111111001110010111110110111101010111101111111000111100000010100011110010011101",
                "(01)40063813339314(10)ABC(21)123",
            ),
        ];
        for (name, mods, want) in cases {
            assert_eq!(decode_modules(mods).as_deref(), Some(want), "{name}");
        }
    }

    #[test]
    fn compressed_field_layouts() {
        // Data-character values measured from the bardecode examples,
        // excluding the leading check character.
        let cases: [(&[u32], &str); 4] = [
            (&[512, 629, 1135, 3024, 1750], "(01)90012345678908(3103)001750"),
            (&[640, 629, 1135, 3027, 868], "(01)90012345678908(3203)003156"),
            (&[800, 78, 2701, 3962, 2804, 2306], "(01)90012345678908(3922)795"),
            (&[832, 78, 2701, 3962, 2088, 683, 1032], "(01)90012345678908(3932)0401234"),
        ];
        for (values, want) in cases {
            assert_eq!(decode_bits(&values_to_bits(values)).as_deref(), Some(want));
        }
    }
}

