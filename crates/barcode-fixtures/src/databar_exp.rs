//! GS1 DataBar Expanded general-purpose (method 00) encoder.
//!
//! Clean-room transcription of the ISO/IEC 24724 general-purpose field:
//! numeric / alphanumeric / ISO/IEC 646 state machine, the 15-module
//! finder blocks, and the mod-211 check character. The tables were
//! measured from the zint oracle (`notes/databar-recon.md`); zxing
//! round-trips every data-character count 3..=21.

use super::databar_exp_tables::{
    EXP_BLOCKS, EXP_ORIENT_REVERSED, EXP_WEIGHTS, EXP_WEIGHT_OFFSET, EXP_WIDTHS,
};
use crate::Canvas;

const GS: u8 = 0x1d;
const MODULUS: u32 = 211;
const MIN_CHARS: usize = 3;
const MAX_CHARS: usize = 21;
const ALNUM_PUNCT: &[u8] = b"*,-./";
const ISO646_PUNCT: &[u8] = b"!\"%&'()*+,-./:;<=>?_ ";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Numeric,
    Alnum,
    Iso646,
}

struct Bits {
    bits: Vec<bool>,
}

impl Bits {
    fn new() -> Self {
        Self {
            bits: Vec::with_capacity(MAX_CHARS * 12),
        }
    }

    fn put(&mut self, value: u32, count: usize) {
        debug_assert!(count <= 32 && (count == 32 || value < (1 << count)));
        for shift in (0..count).rev() {
            self.bits.push((value >> shift) & 1 == 1);
        }
    }

    fn len(&self) -> usize {
        self.bits.len()
    }

    fn pad_to_char(&mut self) {
        while self.bits.len() % 12 != 0 || self.bits.len() < MIN_CHARS * 12 {
            self.bits.push(false);
        }
    }

    fn values(self) -> Vec<u16> {
        let mut values = Vec::with_capacity(self.bits.len() / 12);
        for chunk in self.bits.chunks(12) {
            let mut v = 0u16;
            for &b in chunk {
                v = (v << 1) | u16::from(b);
            }
            values.push(v);
        }
        values
    }
}

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

fn is_upper(c: u8) -> bool {
    c.is_ascii_uppercase()
}

fn is_lower(c: u8) -> bool {
    c.is_ascii_lowercase()
}

fn latch(out: &mut Bits, mode: &mut Mode, new: Mode) {
    if *mode == new {
        return;
    }
    match (*mode, new) {
        (Mode::Numeric, Mode::Alnum) => out.put(0b0000, 4),
        (Mode::Alnum, Mode::Iso646) | (Mode::Iso646, Mode::Alnum) => out.put(0b00100, 5),
        (_, Mode::Numeric) => out.put(0b000, 3),
        _ => unreachable!("unsupported DataBar Expanded mode switch"),
    }
    *mode = new;
}

/// Encode the general-purpose element string (AI digits + data, FNC1 0x1d
/// between variable-length fields) into 12-bit symbol-character values.
fn encode(raw: &[u8], start: Mode, extra_gs: bool) -> Result<Vec<u16>, String> {
    let mut raw = raw.to_vec();
    if extra_gs && raw.last() != Some(&GS) {
        raw.push(GS);
    }
    let n = raw.len();
    let mut out = Bits::new();
    out.put(0b00000, 5); // linkage 0, method 00 (any AIs)
    let mut mode = Mode::Numeric;
    if start != Mode::Numeric {
        latch(&mut out, &mut mode, start);
    }
    let mut i = 0;
    while i < n {
        let c = raw[i];
        match mode {
            Mode::Numeric => {
                if c == GS {
                    if i + 1 < n && is_digit(raw[i + 1]) {
                        // 7-bit (FNC1, digit) pair; the 4-bit FNC1 code is
                        // only legal at the very end of the bit stream.
                        let d = (raw[i + 1] - b'0') as u32;
                        out.put(11 * 10 + d + 8, 7);
                        i += 2;
                    } else {
                        // FNC1 in alphanumeric, which returns to numeric.
                        latch(&mut out, &mut mode, Mode::Alnum);
                        out.put(15, 5);
                        mode = Mode::Numeric;
                        i += 1;
                    }
                    continue;
                }
                if is_digit(c) {
                    if i + 1 < n && is_digit(raw[i + 1]) {
                        let d1 = (c - b'0') as u32;
                        let d2 = (raw[i + 1] - b'0') as u32;
                        out.put(11 * d1 + d2 + 8, 7);
                        i += 2;
                        continue;
                    }
                    if i + 1 == n {
                        // Last character: the 4-bit code is legal only when
                        // fewer than 7 bits (including padding) remain.
                        let p = out.len();
                        let mut m = p + 4;
                        while m % 12 != 0 {
                            m += 1;
                        }
                        let d = (c - b'0') as u32;
                        if m - p < 7 {
                            out.put(d + 1, 4);
                        } else {
                            out.put(11 * d + 18, 7);
                        }
                        i += 1;
                        continue;
                    }
                    // Lone digit followed by a non-digit: alphanumeric, so
                    // no spurious FNC1 is inserted.
                    latch(&mut out, &mut mode, Mode::Alnum);
                    continue;
                }
                latch(&mut out, &mut mode, Mode::Alnum);
            }
            Mode::Alnum => {
                if c == GS {
                    out.put(15, 5);
                    mode = Mode::Numeric;
                    i += 1;
                } else if is_digit(c) {
                    out.put((c - b'0') as u32 + 5, 5);
                    i += 1;
                } else if is_upper(c) {
                    out.put((c - b'A') as u32 + 32, 6);
                    i += 1;
                } else if let Some(pos) = ALNUM_PUNCT.iter().position(|&p| p == c) {
                    out.put(58 + pos as u32, 6);
                    i += 1;
                } else {
                    latch(&mut out, &mut mode, Mode::Iso646);
                }
            }
            Mode::Iso646 => {
                if c == GS {
                    out.put(15, 5);
                    mode = Mode::Numeric;
                    i += 1;
                } else if is_digit(c) {
                    out.put((c - b'0') as u32 + 5, 5);
                    i += 1;
                } else if is_upper(c) {
                    out.put((c - b'A') as u32 + 64, 7);
                    i += 1;
                } else if is_lower(c) {
                    out.put((c - b'a') as u32 + 90, 7);
                    i += 1;
                } else if let Some(pos) = ISO646_PUNCT.iter().position(|&p| p == c) {
                    out.put(232 + pos as u32, 8);
                    i += 1;
                } else {
                    return Err(format!(
                        "character {:?} is not in the GS1 DataBar Expanded subset",
                        c as char
                    ));
                }
            }
        }
    }
    // In ISO646 an 8-bit zero padding would fail the decoder; a 3-bit
    // numeric latch makes the rest decode as (trimmed) FNC1.
    let p = out.len();
    let mut m = p;
    while m % 12 != 0 || m < MIN_CHARS * 12 {
        m += 1;
    }
    if mode == Mode::Iso646 && m - p >= 8 {
        out.put(0b000, 3);
    }
    out.pad_to_char();
    let mut values = out.values();
    if values.len() > MAX_CHARS {
        return Err(format!(
            "payload needs {} DataBar Expanded characters (max {})",
            values.len(),
            MAX_CHARS
        ));
    }
    // The two header bits after the method are the length parity: 10 for an
    // even number of data characters, 00 for odd.
    if values.len() % 2 == 0 {
        values[0] |= 0b10 << 7;
    }
    Ok(values)
}

fn encode_safe(raw: &[u8]) -> Result<Vec<u16>, String> {
    // 4095 has no symbol character (all-ones) and 4093's table orientation
    // was never observed by the oracle sweep. Both are rare; a different
    // valid encodation of the same payload avoids them. 50k random payloads
    // needed a retry 5 times, always the second variant.
    const VARIANTS: [(Mode, bool); 6] = [
        (Mode::Numeric, false),
        (Mode::Alnum, false),
        (Mode::Iso646, false),
        (Mode::Numeric, true),
        (Mode::Alnum, true),
        (Mode::Iso646, true),
    ];
    for (start, extra_gs) in VARIANTS {
        if let Ok(values) = encode(raw, start, extra_gs) {
            if !values.iter().any(|&v| v == 4093 || v == 4095) {
                return Ok(values);
            }
        }
    }
    Err("could not avoid the reserved DataBar Expanded character values".into())
}

fn canonical_widths(value: u16) -> [u8; 8] {
    let packed = EXP_WIDTHS[value as usize];
    let mut widths = [0u8; 8];
    for (i, w) in widths.iter_mut().enumerate() {
        *w = ((packed >> (4 * (7 - i))) & 0xf) as u8;
    }
    widths
}

fn orientation_reversed(value: u16) -> bool {
    EXP_ORIENT_REVERSED[(value >> 3) as usize] & (1 << (value & 7)) != 0
}

/// The physical eight-element widths for a character at a data position.
/// Symbol characters alternate orientation by position; the check character
/// sits at position -1 (odd).
fn physical_widths(value: u16, position: usize) -> [u8; 8] {
    let mut widths = canonical_widths(value);
    if orientation_reversed(value) ^ (position % 2 == 1) {
        widths.reverse();
    }
    widths
}

fn checksum(values: &[u16]) -> u16 {
    let n = values.len();
    let offset = EXP_WEIGHT_OFFSET[n] as usize;
    let coef = &EXP_WEIGHTS[offset..offset + 8 * n + 1];
    let mut sum = coef[8 * n] as u32;
    for (i, &value) in values.iter().enumerate() {
        for (j, w) in physical_widths(value, i).iter().enumerate() {
            sum += coef[i * 8 + j] as u32 * *w as u32;
        }
    }
    (sum % MODULUS) as u16
}

/// Parse a decoded GS1 element string (`(AI)data` or `[AI]data`) back into
/// the general-purpose field: AI digits + data, FNC1 between fields.
/// Byte offset just past a `(AI)` / `[AI]` marker at `at`, if one starts
/// there. A literal `(` inside field data is not a marker unless it is
/// followed by 2..=4 digits and its closing bracket.
fn ai_marker(bytes: &[u8], at: usize) -> Option<usize> {
    let close = match bytes.get(at) {
        Some(b'(') => b')',
        Some(b'[') => b']',
        _ => return None,
    };
    let mut end = at + 1;
    while end < bytes.len() && end < at + 5 && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let digits = end - (at + 1);
    if (2..=4).contains(&digits) && bytes.get(end) == Some(&close) {
        Some(end + 1)
    } else {
        None
    }
}

fn parse_general_purpose(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    let bytes = text.as_bytes();
    let first = bytes
        .iter()
        .enumerate()
        .find_map(|(i, _)| ai_marker(bytes, i).map(|end| (i, end)));
    let Some((start, _)) = first else {
        return Ok(bytes.to_vec());
    };
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = start;
    let mut first_field = true;
    while let Some(ai_end) = ai_marker(bytes, at) {
        let ai = &text[at + 1..ai_end - 1];
        let data_start = ai_end;
        let next = (data_start..bytes.len())
            .find(|&i| ai_marker(bytes, i).is_some())
            .unwrap_or(bytes.len());
        if !first_field {
            out.push(GS);
        }
        out.extend_from_slice(ai.as_bytes());
        out.extend_from_slice(&bytes[data_start..next]);
        first_field = false;
        at = next;
    }
    if at < bytes.len() {
        // Text before the first marker: treat it as a raw payload.
        return Ok(bytes.to_vec());
    }
    Ok(out)
}

/// Render a GS1 DataBar Expanded symbol from a decoded element string.
pub fn encode_databarexpanded(text: &str, module: u32, height: u32) -> Result<Canvas, String> {
    let raw = parse_general_purpose(text)?;
    let values = encode_safe(&raw)?;
    let n = values.len();
    debug_assert!((MIN_CHARS..=MAX_CHARS).contains(&n));
    let check = (MODULUS * (n as u32 - 3) + checksum(&values) as u32) as u16;

    let mut runs: Vec<(bool, u32)> = Vec::with_capacity(3 + 8 + 5 + n * 8 + (n / 2) * 5 + 2);
    let mut ink = false;
    let mut add = |widths: &[u8]| {
        for w in widths {
            if *w > 0 {
                runs.push((ink, *w as u32));
                ink = !ink;
            }
        }
    };
    add(&[1, 1]); // left guard: space, bar
    add(&physical_widths(check, 1));
    add(&[1, 8, 4, 1, 1]); // START
    let blocks = EXP_BLOCKS[n / 2];
    for (i, &value) in values.iter().enumerate() {
        add(&physical_widths(value, i));
        if (i + 1) % 2 == 0 {
            add(&blocks[i / 2]);
        }
    }
    add(&[1, 1]); // right end

    let quiet = 10;
    let modules: u32 = runs.iter().map(|&(_, w)| w).sum();
    let width = (modules + 2 * quiet) * module;
    let mut canvas = Canvas::new(width, height);
    let mut x = quiet * module;
    for (bar, w) in runs {
        if bar {
            canvas.fill(x, 0, w * module, height);
        }
        x += w * module;
    }
    Ok(canvas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_zint_method_00() {
        // zint: Data / Check char for three general-purpose payloads.
        let cases: &[(&[u8], &[u16], u32)] = &[
            (b"2112345", &[31, 683, 1608], 96),
            (b"10ABC\x1d2112345", &[275, 130, 393, 3708, 2733, 2336], 808),
            (b"90abc\x1d2112345", &[363, 37, 2743, 1822, 1994, 2774], 710),
        ];
        for (raw, want, check) in cases {
            let values = encode(raw, Mode::Numeric, false).unwrap();
            assert_eq!(&values, want, "raw {raw:?}");
            let n = values.len() as u32;
            assert_eq!(
                MODULUS * (n - 3) + checksum(&values) as u32,
                *check,
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn parses_element_strings() {
        assert_eq!(parse_general_purpose("(21)12345").unwrap(), b"2112345");
        assert_eq!(
            parse_general_purpose("(01)123(10)AB").unwrap(),
            b"01123\x1d10AB"
        );
        assert_eq!(
            parse_general_purpose("[90]abc[21]1").unwrap(),
            b"90abc\x1d211"
        );
        assert_eq!(parse_general_purpose("2112345").unwrap(), b"2112345");
        // A literal '(' in field data is not an AI marker.
        assert_eq!(parse_general_purpose("(10)A(B").unwrap(), b"10A(B");
        assert_eq!(
            parse_general_purpose("(10)A(B(21)123").unwrap(),
            b"10A(B\x1d21123"
        );
    }

    #[test]
    fn avoids_unencodable_character_values() {
        // 4095 has no width-table entry and 4093 was never observed by the
        // oracle; encode_safe must pick a different valid encodation.
        for n in 3..=21 {
            let text = "(21)".to_string() + &"7".repeat(n * 2);
            if let Ok(values) = encode_safe(parse_general_purpose(&text).unwrap().as_slice()) {
                assert!(!values.iter().any(|&v| v == 4093 || v == 4095));
            }
        }
    }
}
