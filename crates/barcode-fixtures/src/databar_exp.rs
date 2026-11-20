//! GS1 DataBar Expanded encoder.
//!
//! Clean-room transcription of the ISO/IEC 24724 encodation methods: the
//! compressed AI (01) families (method 1, weight, price, weight + date)
//! when the element string matches one, and the general-purpose (method
//! 00) numeric / alphanumeric / ISO/IEC 646 state machine otherwise. The
//! 15-module finder blocks and the mod-211 check character were measured
//! from the zint oracle (`notes/databar-recon.md`); zxing validates the
//! rendered symbols, and zxing round-trips every data-character count
//! 3..=21.

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

/// Write the general-purpose field (AI digits + data, FNC1 0x1d between
/// variable-length fields). The caller has already written the header, so
/// every bit-position-dependent choice below uses the absolute position.
fn write_general(out: &mut Bits, raw: &[u8], start: Mode, extra_gs: bool) -> Result<Mode, String> {
    let mut raw = raw.to_vec();
    if extra_gs && raw.last() != Some(&GS) {
        raw.push(GS);
    }
    let n = raw.len();
    let mut mode = Mode::Numeric;
    if start != Mode::Numeric {
        latch(out, &mut mode, start);
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
                        latch(out, &mut mode, Mode::Alnum);
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
                    latch(out, &mut mode, Mode::Alnum);
                    continue;
                }
                latch(out, &mut mode, Mode::Alnum);
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
                    latch(out, &mut mode, Mode::Iso646);
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
    Ok(mode)
}

/// Variable-length bits in the method-00 5-bit `0 00 LL` header: header
/// bits 3..4 are symbol-character bits 8..7.
const VARIABLE_HEADER_00: (u16, u16) = (0b10 << 7, 0b1 << 7);
/// Variable-length bits in the method-1 4-bit `0 1 LL` header: header
/// bits 2..3 are symbol-character bits 9..8.
const VARIABLE_HEADER_1: (u16, u16) = (0b10 << 8, 0b1 << 8);
/// Variable-length bits in the 8-bit `0 01100 LL` / `0 01101 LL` headers:
/// header bits 6..7 are symbol-character bits 5..4.
const VARIABLE_HEADER_8: (u16, u16) = (0b1 << 5, 0b1 << 4);
const NO_DATE: u32 = 38_400;

/// Pad to a whole number of symbol characters, enforce the limits, and set
/// the two variable-length header bits (`None` for methods without them).
/// The first is the odd symbol-character count, the second "more than 14".
fn finish(mut out: Bits, mode: Mode, parity: Option<(u16, u16)>) -> Result<Vec<u16>, String> {
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
    if let Some((odd, long)) = parity {
        // symbol_chars = data_chars + 1: odd count == even data chars.
        if values.len() % 2 == 0 {
            values[0] |= odd;
        }
        if values.len() > 13 {
            values[0] |= long;
        }
    }
    Ok(values)
}

/// General-purpose (method 00) encodation: valid for every element string,
/// used as the fallback when a compressed method does not apply.
fn encode_method_00(raw: &[u8], start: Mode, extra_gs: bool) -> Result<Vec<u16>, String> {
    let mut out = Bits::new();
    out.put(0b00000, 5); // linkage 0, method 00 (any AIs)
    let mode = write_general(&mut out, raw, start, extra_gs)?;
    finish(out, mode, Some(VARIABLE_HEADER_00))
}

/// One `(AI)data` field of a GS1 element string.
#[derive(Clone, Copy)]
struct Field<'a> {
    ai: &'a [u8],
    data: &'a [u8],
}

fn ai_is(ai: &[u8], want: &[u8]) -> bool {
    ai == want
}

/// Split a fully bracketed element string into fields. Returns `None` for a
/// raw payload (no brackets), where only method 00 applies.
fn parse_fields(text: &str) -> Option<Vec<Field<'_>>> {
    let bytes = text.trim().as_bytes();
    let mut fields = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let Some(ai_end) = ai_marker(bytes, at) else {
            return None;
        };
        let data_start = ai_end;
        let next = (data_start..bytes.len())
            .find(|&i| ai_marker(bytes, i).is_some())
            .unwrap_or(bytes.len());
        fields.push(Field {
            ai: &bytes[at + 1..ai_end - 1],
            data: &bytes[data_start..next],
        });
        at = next;
    }
    (!fields.is_empty()).then_some(fields)
}

fn parse_digits(bytes: &[u8], len: usize) -> Option<u64> {
    if bytes.len() != len || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(
        bytes
            .iter()
            .fold(0u64, |acc, &c| acc * 10 + (c - b'0') as u64),
    )
}

/// The 14-digit GTIN's check digit must match the first 13 digits: the
/// compressed methods rebuild the GTIN from its 13-digit body.
fn gtin_check_ok(digits: &[u8]) -> bool {
    if parse_digits(digits, 14).is_none() {
        return false;
    }
    let mut sum = 0;
    for (i, &c) in digits[..13].iter().enumerate() {
        sum += (c - b'0') as u32 * if i % 2 == 0 { 3 } else { 1 };
    }
    ((10 - sum % 10) % 10) as u8 + b'0' == digits[13]
}

/// Four 10-bit groups of three decimal digits (the GTIN's 13-digit body).
fn push_gtin(out: &mut Bits, body: &[u8]) {
    for k in 0..4 {
        let d = &body[3 * k..3 * k + 3];
        out.put(
            (d[0] - b'0') as u32 * 100 + (d[1] - b'0') as u32 * 10 + (d[2] - b'0') as u32,
            10,
        );
    }
}

/// `310x` / `320x`: returns (prefix, decimal places).
fn weight_ai(ai: &[u8]) -> Option<(u32, u32)> {
    let value = parse_digits(ai, 4)? as u32;
    let (prefix, x) = (value / 10, value % 10);
    if (prefix == 310 || prefix == 320) && x <= 5 {
        Some((prefix, x))
    } else {
        None
    }
}

/// `11` production, `13` packaging, `15` best-before, `17` expiry.
fn date_index(ai: &[u8]) -> Option<usize> {
    if ai_is(ai, b"11") {
        Some(0)
    } else if ai_is(ai, b"13") {
        Some(1)
    } else if ai_is(ai, b"15") {
        Some(2)
    } else if ai_is(ai, b"17") {
        Some(3)
    } else {
        None
    }
}

fn encode_date(bytes: &[u8]) -> Option<u32> {
    let value = parse_digits(bytes, 6)? as u32;
    let (year, month, day) = (value / 10_000, (value / 100) % 100, value % 100);
    if !(1..=12).contains(&month) || day > 31 {
        return None;
    }
    Some(year * 384 + (month - 1) * 32 + day)
}

/// `392x` price or `393x` price per unit of measure.
fn encode_price(body: &[u8], ai: &[u8], data: &[u8]) -> Option<Vec<u16>> {
    if ai.len() != 4 || data.is_empty() || !data.iter().all(u8::is_ascii_digit) {
        return None;
    }
    // zint only uses the price methods for 3920..3923 / 3930..3933.
    let x = (ai[3] - b'0') as u32;
    if x > 3 {
        return None;
    }
    let mut out = Bits::new();
    if ai_is(&ai[..3], b"392") {
        out.put(0b001100, 6);
        out.put(0, 2);
        push_gtin(&mut out, body);
        out.put(x, 2);
        let mode = write_general(&mut out, data, Mode::Numeric, false).ok()?;
        return finish(out, mode, Some(VARIABLE_HEADER_8)).ok();
    }
    if ai_is(&ai[..3], b"393") {
        if data.len() < 4 {
            return None;
        }
        let currency = parse_digits(&data[..3], 3)? as u32;
        out.put(0b001101, 6);
        out.put(0, 2);
        push_gtin(&mut out, body);
        out.put(x, 2);
        out.put(currency, 10);
        let mode = write_general(&mut out, &data[3..], Mode::Numeric, false).ok()?;
        return finish(out, mode, Some(VARIABLE_HEADER_8)).ok();
    }
    None
}

fn encode_weight_date(
    body: &[u8],
    method: u32,
    x: u32,
    weight: u32,
    date: u32,
) -> Option<Vec<u16>> {
    let mut out = Bits::new();
    out.put(0, 1); // linkage
    out.put(method, 7); // 0111000..0111111
    push_gtin(&mut out, body);
    out.put(x * 100_000 + weight, 20);
    out.put(date, 16);
    finish(out, Mode::Numeric, None).ok()
}

fn encode_weight(body: &[u8], rest: &[Field]) -> Option<Vec<u16>> {
    let (first, date) = match rest {
        [w] => (w, None),
        [w, d] => (w, Some(d)),
        _ => return None,
    };
    let (prefix, x) = weight_ai(first.ai)?;
    let value = parse_digits(first.data, 6)? as u32;
    let date = match date {
        Some(d) => {
            let index = date_index(d.ai)?;
            let date = encode_date(d.data)?;
            Some((56 + index as u32 * 2 + u32::from(prefix == 320), date))
        }
        None => None,
    };
    if let Some((method, date)) = date {
        if value > 99_999 {
            return None;
        }
        return encode_weight_date(body, method, x, value, date);
    }
    // AI (3103) and (3202)/(3203) have dedicated 60-bit methods.
    if prefix == 310 && ai_is(first.ai, b"3103") && value <= 0x7fff {
        let mut out = Bits::new();
        out.put(0b00100, 5);
        push_gtin(&mut out, body);
        out.put(value, 15);
        return finish(out, Mode::Numeric, None).ok();
    }
    if prefix == 320 && (ai_is(first.ai, b"3202") || ai_is(first.ai, b"3203")) {
        // (3202) is the 0..9999.99 pounds form; (3203) is offset by 10000.
        if ai_is(first.ai, b"3202") && value <= 9_999 {
            let mut out = Bits::new();
            out.put(0b00101, 5);
            push_gtin(&mut out, body);
            out.put(value, 15);
            return finish(out, Mode::Numeric, None).ok();
        }
        if ai_is(first.ai, b"3203") {
            if let Some(encoded) = value.checked_add(10_000) {
                if encoded <= 0x7fff {
                    let mut out = Bits::new();
                    out.put(0b00101, 5);
                    push_gtin(&mut out, body);
                    out.put(encoded, 15);
                    return finish(out, Mode::Numeric, None).ok();
                }
            }
        }
    }
    // Other weights (or one that does not fit the 15-bit methods) use the
    // weight+date method with the "no date" sentinel.
    if value > 99_999 {
        return None;
    }
    let method = if prefix == 320 { 57 } else { 56 };
    encode_weight_date(body, method, x, value, NO_DATE)
}

fn encode_compressed(fields: &[Field]) -> Option<Vec<u16>> {
    let gtin = fields.first()?;
    if !ai_is(gtin.ai, b"01") || !gtin_check_ok(gtin.data) {
        return None;
    }
    let indicator = (gtin.data[0] - b'0') as u32;
    let body = &gtin.data[1..13];
    let rest = &fields[1..];
    if indicator == 9 {
        if let [only] = rest {
            if let Some(values) = encode_price(body, only.ai, only.data) {
                return Some(values);
            }
        }
        if let Some(values) = encode_weight(body, rest) {
            return Some(values);
        }
    }
    // Method 1: AI (01) plus the general-purpose remainder.
    let mut out = Bits::new();
    out.put(0b01, 2); // linkage 0, method 1
    out.put(0, 2); // length parity, set by finish
    out.put(indicator, 4);
    push_gtin(&mut out, body);
    let mut raw = Vec::new();
    for (i, field) in rest.iter().enumerate() {
        if i > 0 {
            raw.push(GS);
        }
        raw.extend_from_slice(field.ai);
        raw.extend_from_slice(field.data);
    }
    let mode = write_general(&mut out, &raw, Mode::Numeric, false).ok()?;
    finish(out, mode, Some(VARIABLE_HEADER_1)).ok()
}

fn encode_safe(raw: &[u8], fields: Option<&[Field]>) -> Result<Vec<u16>, String> {
    // 4095 has no symbol character (all-ones) and 4093 is avoided like the
    // method-00 encoder always did. Both are rare; a different valid
    // encodation of the same payload avoids them.
    const VARIANTS: [(Mode, bool); 6] = [
        (Mode::Numeric, false),
        (Mode::Alnum, false),
        (Mode::Iso646, false),
        (Mode::Numeric, true),
        (Mode::Alnum, true),
        (Mode::Iso646, true),
    ];
    let reserved = |values: &[u16]| values.iter().any(|&v| v == 4093 || v == 4095);
    // Compressed methods first: same payload, fewer symbol characters.
    if let Some(fields) = fields {
        if let Some(values) = encode_compressed(fields) {
            if !reserved(&values) {
                return Ok(values);
            }
        }
    }
    for (start, extra_gs) in VARIANTS {
        if let Ok(values) = encode_method_00(raw, start, extra_gs) {
            if !reserved(&values) {
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
    let fields = parse_fields(text);
    let values = encode_safe(&raw, fields.as_deref())?;
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
            let values = encode_method_00(raw, Mode::Numeric, false).unwrap();
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
    fn matches_zint_compressed_methods() {
        // zint 2.16 `Data:` / `Check char:`, one payload per method. `exact`
        // is false where zint fills the final data character with its own
        // padding digits: the leading symbol characters still must match.
        let cases: &[(&str, &[u16], u32, bool)] = &[
            // method 1: AI (01) + general-purpose remainder
            ("(01)90012345678908", &[1680, 78, 2701, 3962], 246, true),
            (
                "(01)40063813339314(10)ABC",
                &[1088, 407, 3411, 1955, 609, 67, 272],
                1037,
                false,
            ),
            (
                "(01)40063813339314(10)ABC(21)12345",
                &[1088, 407, 3411, 1955, 609, 67, 316, 3989, 1452],
                1333,
                false,
            ),
            (
                "(01)90012345678908(90)abc",
                &[1680, 78, 2701, 3962, 3424, 1205, 1774, 264],
                1148,
                false,
            ),
            // 15 data characters: the "more than 14" header bit is set
            (
                "(01)40063813339314(10)ABCDEFGHIJKLMNOPQRST",
                &[
                    1344, 407, 3411, 1955, 609, 67, 327, 587, 847, 1107, 1367, 1627, 1887, 2147,
                    2406,
                ],
                2665,
                false,
            ),
            // 0100: (01) + (3103)
            (
                "(01)90012345678908(3103)001234",
                &[512, 629, 1135, 3024, 1234],
                455,
                true,
            ),
            // 0101: (01)9 + (3202)/(3203)
            (
                "(01)90012345678908(3202)000150",
                &[640, 629, 1135, 3024, 150],
                457,
                true,
            ),
            (
                "(01)90012345678908(3203)001234",
                &[640, 629, 1135, 3026, 3042],
                575,
                true,
            ),
            (
                "(01)90012345678908(3203)000150",
                &[640, 629, 1135, 3026, 1958],
                446,
                true,
            ),
            // 01100: (01)9 + (392x)
            (
                "(01)90012345678908(3920)12345",
                &[800, 78, 2701, 3962, 170, 3474],
                636,
                false,
            ),
            (
                "(01)90012345678908(3922)12345",
                &[800, 78, 2701, 3962, 2218, 3474],
                742,
                false,
            ),
            (
                "(01)90012345678908(3920)123",
                &[800, 78, 2701, 3962, 171, 770],
                838,
                false,
            ),
            // 01101: (01)9 + (393x) currency + price
            (
                "(01)90012345678908(3932)97812345",
                &[832, 78, 2701, 3962, 3026, 683, 1608],
                931,
                false,
            ),
            (
                "(01)90012345678908(3930)97812345",
                &[832, 78, 2701, 3962, 978, 683, 1608],
                864,
                false,
            ),
            // 0111xxx: (01)9 + weight + date (one vector per date AI/prefix)
            (
                "(01)90012345678908(3103)001234(11)261231",
                &[896, 78, 2701, 3962, 1176, 2850, 2175],
                920,
                true,
            ),
            (
                "(01)90012345678908(3205)000150(13)261231",
                &[944, 78, 2701, 3962, 1953, 2914, 2175],
                1043,
                true,
            ),
            (
                "(01)90012345678908(3103)001234(15)261231",
                &[960, 78, 2701, 3962, 1176, 2850, 2175],
                1010,
                true,
            ),
            (
                "(01)90012345678908(3105)001234(17)271231",
                &[992, 78, 2701, 3962, 1957, 3874, 2559],
                902,
                true,
            ),
            (
                "(01)90012345678908(3200)000150(15)261231",
                &[976, 78, 2701, 3962, 0, 2402, 2175],
                1026,
                true,
            ),
            // day 00 means "last day of the month" (GS1 3.4.2)
            (
                "(01)90012345678908(3103)001234(15)261200",
                &[960, 78, 2701, 3962, 1176, 2850, 2144],
                996,
                true,
            ),
            // 0111xxx without a date field (the NO_DATE sentinel)
            (
                "(01)90012345678908(3100)000123",
                &[896, 78, 2701, 3962, 0, 1977, 1536],
                977,
                true,
            ),
            (
                "(01)90012345678908(3200)000150",
                &[912, 78, 2701, 3962, 0, 2409, 1536],
                942,
                true,
            ),
            (
                "(01)90012345678908(3202)012345",
                &[912, 78, 2701, 3962, 829, 1945, 1536],
                1018,
                true,
            ),
            // AI (01) is not first: method 00 stays the only valid choice.
            (
                "(10)ABC(01)90012345678908",
                &[275, 130, 393, 3623, 1417, 1070, 1678, 2336],
                1165,
                true,
            ),
            // 19 data characters: method 00 sets the "more than 14" bit too
            (
                "(10)ABCDEFGHIJKLMNOPQRST(21)12345678901234567890",
                &[
                    147, 130, 394, 914, 1434, 1954, 2474, 2994, 3514, 4035, 459, 889, 3882, 2914,
                    3549, 1621, 1733, 3002, 3080,
                ],
                3439,
                false,
            ),
        ];
        for (text, want, check, exact) in cases {
            let raw = parse_general_purpose(text).unwrap();
            let fields = parse_fields(text);
            let values = encode_safe(&raw, fields.as_deref()).unwrap();
            assert_eq!(values.len(), want.len(), "{text}");
            if *exact {
                assert_eq!(&values, want, "{text}");
                let n = values.len() as u32;
                assert_eq!(
                    MODULUS * (n - 3) + checksum(&values) as u32,
                    *check,
                    "{text}"
                );
            } else {
                assert_eq!(&values[..want.len() - 1], &want[..want.len() - 1], "{text}");
            }
        }
    }

    #[test]
    fn compressed_methods_render_shorter_symbols() {
        // module counts include the 10-module quiet zone on each side.
        let cases: &[(&str, u32)] = &[
            ("(01)40063813339314(10)ABC", 200),
            ("(01)90012345678908(3103)001234", 151),
            ("(01)90012345678908(3922)12345", 183),
            ("(01)90012345678908(3103)001234(15)261231", 200),
        ];
        for (text, want) in cases {
            let compressed = encode_databarexpanded(text, 1, 1).unwrap().width - 20;
            assert_eq!(compressed, *want, "{text}");
            // The same payload through method 00: guard + check + START +
            // eight elements per character + a finder block per pair + end.
            let raw = parse_general_purpose(text).unwrap();
            let values = encode_method_00(&raw, Mode::Numeric, false).unwrap();
            let n = values.len() as u32;
            let method_00 = 2 + 17 + 15 + n * 17 + (n / 2) * 15 + 2;
            assert!(
                compressed < method_00,
                "{text}: {compressed} !< {method_00}"
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
            if let Ok(values) = encode_safe(
                parse_general_purpose(&text).unwrap().as_slice(),
                parse_fields(&text).as_deref(),
            ) {
                assert!(!values.iter().any(|&v| v == 4093 || v == 4095));
            }
        }
    }
}
