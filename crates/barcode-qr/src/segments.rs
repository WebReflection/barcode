//! Milestone 4: data segment decoding (ISO/IEC 18004 §8.3-§8.4).
//!
//! The data codeword bitstream is a sequence of segments: a 4-bit mode
//! indicator, a character-count indicator (whose length grows with the
//! version), and the mode's payload — optionally repeated, then a 4-bit
//! terminator (0000) that may be truncated to the remaining capacity,
//! zero-padding to the byte boundary, and 0xEC/0x11 pad codewords.
//!
//! All payload is accumulated into one byte buffer: numeric and
//! alphanumeric characters are ASCII, and ASCII bytes are inert inside
//! UTF-8 — so even a multi-byte character split across byte segments
//! reassembles correctly. The buffer becomes the result text when it is
//! valid UTF-8, and is mapped as ISO-8859-1 otherwise.

const ALNUM: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

/// Read n bits MSB-first; None past the end of the stream.
fn take(data: &[u8], pos: &mut usize, n: usize) -> Option<u32> {
    let mut v = 0u32;
    for _ in 0..n {
        let byte = data.get(*pos / 8)?;
        v = (v << 1) | ((byte >> (7 - *pos % 8)) & 1) as u32;
        *pos += 1;
    }
    Some(v)
}

/// Byte → `char` (Latin-1 style) display mapping (see barcode-core).
pub fn payload_text(bytes: &[u8]) -> String {
    barcode_core::payload_text(bytes)
}

/// Parse the data-codeword bitstream of a `version` symbol into its
/// payload bytes. Returns None on an unsupported mode or a truncated
/// stream. (Numeric/alphanumeric/byte modes; ECI, kanji, FNC1 and
/// structured append are rejected.)
pub fn parse_segments(version: u32, data: &[u8]) -> Option<Vec<u8>> {
    let total = data.len() * 8;
    let mut pos = 0usize;
    let mut out: Vec<u8> = Vec::new();
    loop {
        if total - pos < 4 {
            break; // no room for another mode indicator: implicit terminator
        }
        let mode = take(data, &mut pos, 4)?;
        match mode {
            0 => break, // terminator
            1 => {
                // Numeric: 3 digits per 10 bits; remainders 2 → 7, 1 → 4.
                let count = take(data, &mut pos, (count_for(version, 1)?) as usize)? as usize;
                let mut left = count;
                while left >= 3 {
                    let v = take(data, &mut pos, 10)?;
                    out.push((v / 100) as u8 + b'0');
                    out.push((v / 10 % 10) as u8 + b'0');
                    out.push((v % 10) as u8 + b'0');
                    left -= 3;
                }
                if left == 2 {
                    let v = take(data, &mut pos, 7)?;
                    if v > 99 {
                        return None;
                    }
                    out.push((v / 10) as u8 + b'0');
                    out.push((v % 10) as u8 + b'0');
                } else if left == 1 {
                    let v = take(data, &mut pos, 4)?;
                    if v > 9 {
                        return None;
                    }
                    out.push(v as u8 + b'0');
                }
            }
            2 => {
                // Alphanumeric: pairs per 11 bits (first·45 + second).
                let count = take(data, &mut pos, (count_for(version, 2)?) as usize)? as usize;
                let mut left = count;
                while left >= 2 {
                    let v = take(data, &mut pos, 11)? as usize;
                    out.push(*ALNUM.get(v / 45)?);
                    out.push(*ALNUM.get(v % 45)?);
                    left -= 2;
                }
                if left == 1 {
                    let v = take(data, &mut pos, 6)? as usize;
                    out.push(*ALNUM.get(v)?);
                }
            }
            4 => {
                // Byte: `count` raw bytes.
                let count = take(data, &mut pos, (count_for(version, 4)?) as usize)? as usize;
                for _ in 0..count {
                    out.push(take(data, &mut pos, 8)? as u8);
                }
            }
            7 => {
                // ECI designator (ISO/IEC 18004 §8.4.1): 1-, 2- or 3-byte
                // assignment number, then the next segment. Camera QR
                // generators prepend ECI 26 (UTF-8) to byte payloads —
                // rejecting the mode drops perfectly valid symbols. The
                // charset only affects how a consumer interprets the
                // emitted bytes; this parser's contract is raw bytes, so
                // the value is read and skipped.
                let first = take(data, &mut pos, 8)?;
                let _eci = if first & 0x80 == 0 {
                    first
                } else if first & 0xC0 == 0x80 {
                    let second = take(data, &mut pos, 8)?;
                    ((first & 0x3F) << 8) | second
                } else if first & 0xE0 == 0xC0 {
                    let second = take(data, &mut pos, 8)?;
                    let third = take(data, &mut pos, 8)?;
                    ((first & 0x1F) << 16) | (second << 8) | third
                } else {
                    return None; // reserved designator prefix
                };
            }
            _ => return None, // kanji, FNC1, structured append, …
        }
    }
    Some(out)
}

/// Character-count indicator length for `mode` on `version`
/// (ISO 18004 §8.3.2: v1-9 / v10-26 / v27-40).
fn count_for(version: u32, mode: u32) -> Option<u32> {
    let group = if version <= 9 {
        0
    } else if version <= 26 {
        1
    } else {
        2
    };
    match (mode, group) {
        (1, 0) => Some(10),
        (1, 1) => Some(12),
        (1, 2) => Some(14),
        (2, 0) => Some(9),
        (2, 1) => Some(11),
        (2, 2) => Some(13),
        (4, 0) => Some(8),
        (4, 1) => Some(16),
        (4, 2) => Some(16),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(bits: &mut Vec<bool>, v: u32, n: u32) {
        for i in (0..n).rev() {
            bits.push((v >> i) & 1 == 1);
        }
    }

    fn pack(bits: &[bool]) -> Vec<u8> {
        let mut out = vec![0u8; bits.len().div_ceil(8)];
        for (i, &b) in bits.iter().enumerate() {
            if b {
                out[i / 8] |= 0x80 >> (i % 8);
            }
        }
        out
    }

    #[test]
    fn parses_numeric_segments() {
        // "4006381333931": 4 full groups (10 bits each) + 1 digit (4 bits).
        let digits = "4006381333931";
        let mut bits = Vec::new();
        push(&mut bits, 1, 4); // numeric
        push(&mut bits, digits.len() as u32, 10);
        for g in digits.as_bytes().chunks(3) {
            let v: u32 = g.iter().map(|&d| (d - b'0') as u32).fold(0, |a, d| a * 10 + d);
            let w = if g.len() == 3 { 10 } else if g.len() == 2 { 7 } else { 4 };
            push(&mut bits, v, w);
        }
        push(&mut bits, 0, 4); // terminator
        assert_eq!(payload_text(&parse_segments(1, &pack(&bits)).unwrap()), digits);
    }

    #[test]
    fn parses_alphanumeric_segments() {
        // "HELLO WORLD": 5 pairs + 1 single.
        let text = "HELLO WORLD";
        let mut bits = Vec::new();
        push(&mut bits, 2, 4);
        push(&mut bits, text.len() as u32, 9);
        for p in text.as_bytes().chunks(2) {
            let v: u32 = p.iter().map(|&c| ALNUM.iter().position(|&a| a == c).unwrap() as u32)
                .fold(0, |a, d| a * 45 + d);
            push(&mut bits, v, if p.len() == 2 { 11 } else { 6 });
        }
        push(&mut bits, 0, 4);
        assert_eq!(payload_text(&parse_segments(1, &pack(&bits)).unwrap()), text);
    }

    #[test]
    fn parses_byte_segments_as_utf8() {
        let text = "Grüße aus Wien";
        let mut bits = Vec::new();
        push(&mut bits, 4, 4);
        push(&mut bits, text.len() as u32, 8);
        for &b in text.as_bytes() {
            push(&mut bits, b as u32, 8);
        }
        push(&mut bits, 0, 4);
        assert_eq!(payload_text(&parse_segments(1, &pack(&bits)).unwrap()), text);
    }

    #[test]
    fn concatenates_mixed_segments_in_order() {
        // Numeric "12" then byte "AB" — both append ASCII to one buffer.
        let mut bits = Vec::new();
        push(&mut bits, 1, 4);
        push(&mut bits, 2, 10);
        push(&mut bits, 12, 7); // "12" as one 2-digit group
        push(&mut bits, 4, 4);
        push(&mut bits, 2, 8);
        push(&mut bits, b'A' as u32, 8);
        push(&mut bits, b'B' as u32, 8);
        push(&mut bits, 0, 4);
        assert_eq!(payload_text(&parse_segments(1, &pack(&bits)).unwrap()), "12AB");
    }

    #[test]
    fn ignores_pad_codewords_after_terminator() {
        let mut bits = Vec::new();
        push(&mut bits, 4, 4);
        push(&mut bits, 1, 8);
        push(&mut bits, b'x' as u32, 8);
        push(&mut bits, 0, 4);
        // Byte-align + two pad codewords.
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        push(&mut bits, 0xEC, 8);
        push(&mut bits, 0x11, 8);
        assert_eq!(payload_text(&parse_segments(1, &pack(&bits)).unwrap()), "x");
    }

    #[test]
    fn byte_segment_without_explicit_terminator() {
        // Stream ends exactly after the data (no terminator bits).
        let mut bits = Vec::new();
        push(&mut bits, 4, 4);
        push(&mut bits, 2, 8);
        push(&mut bits, b'o' as u32, 8);
        push(&mut bits, b'k' as u32, 8);
        assert_eq!(payload_text(&parse_segments(1, &pack(&bits)).unwrap()), "ok");
    }

    #[test]
    fn rejects_unsupported_modes_and_bad_payloads() {
        // Kanji mode indicator.
        let mut bits = Vec::new();
        push(&mut bits, 8, 4);
        push(&mut bits, 1, 8);
        assert_eq!(parse_segments(1, &pack(&bits)), None);
        // Numeric 1-digit group with value > 9.
        let mut bits = Vec::new();
        push(&mut bits, 1, 4);
        push(&mut bits, 1, 10);
        push(&mut bits, 11, 4); // invalid: single digit must be ≤ 9
        assert_eq!(parse_segments(1, &pack(&bits)), None);
        // Truncated byte payload.
        let mut bits = Vec::new();
        push(&mut bits, 4, 4);
        push(&mut bits, 5, 8);
        push(&mut bits, b'a' as u32, 8); // one byte short
        assert_eq!(parse_segments(1, &pack(&bits)), None);
    }

    #[test]
    fn falls_back_to_latin1_for_non_utf8_bytes() {
        let mut bits = Vec::new();
        push(&mut bits, 4, 4);
        push(&mut bits, 2, 8);
        push(&mut bits, 0xe9, 8); // é in ISO-8859-1 — invalid UTF-8 alone
        push(&mut bits, b'x' as u32, 8);
        push(&mut bits, 0, 4);
        assert_eq!(payload_text(&parse_segments(1, &pack(&bits)).unwrap()), "éx");
    }

    #[test]
    fn count_indicator_length_grows_with_version() {
        assert_eq!(count_for(6, 4), Some(8));
        assert_eq!(count_for(10, 4), Some(16)); // ISO: byte is 8/16/16
        assert_eq!(count_for(27, 4), Some(16));
        assert_eq!(count_for(6, 1), Some(10));
        assert_eq!(count_for(10, 1), Some(12));
        assert_eq!(count_for(6, 8), None); // kanji: unsupported
    }
}

#[cfg(test)]
mod eci_tests {
    use super::*;

    fn push(bits: &mut Vec<bool>, v: u32, n: u32) {
        for i in (0..n).rev() {
            bits.push((v >> i) & 1 == 1);
        }
    }

    fn pack(bits: &[bool]) -> Vec<u8> {
        let mut out = vec![0u8; bits.len().div_ceil(8)];
        for (i, &b) in bits.iter().enumerate() {
            if b {
                out[i / 8] |= 0x80 >> (i % 8);
            }
        }
        out
    }

    /// Generator libraries (Cortilia's print, many QR services) prefix
    /// byte segments with an ECI designator — ECI 26 (UTF-8) is the
    /// common one. A field QR decoded by zxing but not by us:
    /// `https://qrco.de/bgsnku`, v3-H, stream starts 71 a4 16
    /// (mode 7, ECI 26, byte mode).
    #[test]
    fn byte_segment_after_eci_26_decodes() {
        let url = b"https://qrco.de/bgsnku";
        let mut bits = Vec::new();
        push(&mut bits, 7, 4); // ECI mode
        push(&mut bits, 26, 8); // UTF-8
        push(&mut bits, 4, 4); // byte mode
        push(&mut bits, url.len() as u32, 8);
        for &b in url {
            push(&mut bits, b as u32, 8);
        }
        push(&mut bits, 0, 4); // terminator
        let data = pack(&bits);
        assert_eq!(parse_segments(3, &data).unwrap(), url.to_vec());
    }

    /// The two- and three-byte ECI designators must consume the right
    /// number of bits and leave the following segment aligned.
    #[test]
    fn multi_byte_eci_designators_are_skipped() {
        for (eci, width) in [(128u32, 16), (16384u32, 24)] {
            let mut bits = Vec::new();
            push(&mut bits, 7, 4);
            match width {
                16 => push(&mut bits, 0x8000 | eci, 16),
                _ => push(&mut bits, 0xC00000 | eci, 24),
            }
            push(&mut bits, 4, 4);
            push(&mut bits, 2, 8);
            for &b in b"hi" {
                push(&mut bits, b as u32, 8);
            }
            push(&mut bits, 0, 4);
            assert_eq!(parse_segments(3, &pack(&bits)).unwrap(), b"hi".to_vec());
        }
    }
}
