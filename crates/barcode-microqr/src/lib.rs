//! Clean-room ISO/IEC 18004 Micro QR Code encoder and decoder (versions M1–M4).
//!
//! Full algorithmic implementation of the public ISO/IEC 18004 Micro QR
//! specification (MIT licensed), reusing [`barcode_qr::rs`] for GF(2^8)
//! Reed-Solomon coding:
//!
//! | version | size  | levels  | max numeric | max alnum | max byte |
//! |---------|-------|---------|-------------|-----------|----------|
//! | M1      | 11×11 | L       | 5           | —         | —        |
//! | M2      | 13×13 | L, M    | 10 / 8      | 6 / 5     | —        |
//! | M3      | 15×15 | L, M    | 23 / 18     | 14 / 11   | 9 / 7    |
//! | M4      | 17×17 | L, M, Q | 35 / 30 / 21| 21 / 18 / 13 | 15 / 13 / 9 |
//!
//! # Example
//!
//! ```
//! use barcode_microqr::{encode, Level, Version};
//!
//! let sym = encode("12345", Version::M1, Level::L, None).unwrap();
//! assert_eq!(sym.size, 11);
//! assert!(sym.ink(0, 0));
//! ```

pub mod decoder;

pub use decoder::{decode, decode as decode_microqr};

use barcode_qr::rs::{rs_encode, Gf};

/// Micro QR version (size and capacity tier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    M1,
    M2,
    M3,
    M4,
}

/// Error correction level. Availability depends on the version.
///
/// Note: M1 carries 2 error-detection codewords (no error correction) and is
/// addressed as `(Version::M1, Level::L)` for API uniformity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    L,
    M,
    Q,
}

/// Encoding mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Numeric,
    Alnum,
    Byte,
}

impl Mode {
    pub(crate) fn code(self) -> u8 {
        match self {
            Mode::Numeric => 0,
            Mode::Alnum => 1,
            Mode::Byte => 2,
        }
    }
}

/// A rendered Micro QR symbol: a square row-major 0/1 module matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Side length in modules (11, 13, 15, or 17).
    pub size: usize,
    /// Row-major module values, `size * size` entries, 1 = inked.
    pub cells: Vec<u8>,
}

impl Symbol {
    /// Module value at (x, y); `false` outside the symbol.
    pub fn ink(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.cells[y * self.size + x] == 1
    }
}

pub(crate) const ALNUM: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

/// ISO/IEC 18004 Table 1 / Table 9 symbol geometry and RS parameters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spec {
    /// 3-bit Micro QR symbol number (`0..=7`) encoded in the format word.
    pub sym: u8,
    pub version: Version,
    pub level: Level,
    pub size: usize,
    /// Total data capacity in bits (including mode/count/terminator/padding).
    pub data_bits: usize,
    /// Data codewords fed into RS (M1 and M3 include a 4-bit final codeword
    /// placed in the upper nibble of an 8-bit RS symbol).
    pub dc: usize,
    /// Reed-Solomon error-correction (or detection for M1) codewords.
    pub ec: usize,
    /// True for M1 and M3, where the last data codeword is 4 bits wide.
    pub half_last: bool,
}

pub(crate) const SPECS: [Spec; 8] = [
    Spec { sym: 0, version: Version::M1, level: Level::L, size: 11, data_bits: 20,  dc: 3,  ec: 2,  half_last: true  },
    Spec { sym: 1, version: Version::M2, level: Level::L, size: 13, data_bits: 40,  dc: 5,  ec: 5,  half_last: false },
    Spec { sym: 2, version: Version::M2, level: Level::M, size: 13, data_bits: 32,  dc: 4,  ec: 6,  half_last: false },
    Spec { sym: 3, version: Version::M3, level: Level::L, size: 15, data_bits: 84,  dc: 11, ec: 6,  half_last: true  },
    Spec { sym: 4, version: Version::M3, level: Level::M, size: 15, data_bits: 68,  dc: 9,  ec: 8,  half_last: true  },
    Spec { sym: 5, version: Version::M4, level: Level::L, size: 17, data_bits: 128, dc: 16, ec: 8,  half_last: false },
    Spec { sym: 6, version: Version::M4, level: Level::M, size: 17, data_bits: 112, dc: 14, ec: 10, half_last: false },
    Spec { sym: 7, version: Version::M4, level: Level::Q, size: 17, data_bits: 80,  dc: 10, ec: 14, half_last: false },
];

/// Coordinates of the 15-bit format strip, MSB (bit 14) to LSB (bit 0).
pub(crate) const FORMAT_COORDS: [(usize, usize); 15] = [
    (1, 8), (2, 8), (3, 8), (4, 8), (5, 8), (6, 8), (7, 8), (8, 8),
    (8, 7), (8, 6), (8, 5), (8, 4), (8, 3), (8, 2), (8, 1),
];

/// Format-information XOR mask defined by ISO/IEC 18004 §7.9.2 (`100010001000101`).
pub(crate) const FORMAT_XOR: u16 = 0x4445;

/// BCH(15, 5) format word for `sym` (`0..=7`) and `mask` (`0..=3`).
pub(crate) fn format_word(sym: u8, mask: u8) -> u16 {
    let data = ((sym as u16) << 2) | (mask as u16 & 3);
    let mut rem = data << 10;
    for i in (10..=14).rev() {
        if (rem >> i) & 1 != 0 {
            rem ^= 0x537 << (i - 10);
        }
    }
    ((data << 10) | rem) ^ FORMAT_XOR
}

/// ISO/IEC 18004 Table 10 Micro QR data mask patterns (`0..=3`).
#[inline]
pub(crate) fn mask_bit(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => y % 2 == 0,
        1 => (y / 2 + x / 3) % 2 == 0,
        2 => ((y * x) % 2 + (y * x) % 3) % 2 == 0,
        _ => (((y + x) % 2) + ((y * x) % 3)) % 2 == 0,
    }
}

/// True if `(x, y)` is reserved for finder, separator, timing, or format info.
#[inline]
pub(crate) fn is_function(x: usize, y: usize) -> bool {
    x == 0 || y == 0 || (x <= 8 && y <= 8)
}

#[inline]
pub(crate) fn mode_bits(v: Version) -> usize {
    match v {
        Version::M1 => 0,
        Version::M2 => 1,
        Version::M3 => 2,
        Version::M4 => 3,
    }
}

#[inline]
pub(crate) fn term_bits(v: Version) -> usize {
    match v {
        Version::M1 => 3,
        Version::M2 => 5,
        Version::M3 => 7,
        Version::M4 => 9,
    }
}

#[inline]
pub(crate) fn count_bits(v: Version, mode_code: u8) -> Option<usize> {
    match (v, mode_code) {
        (Version::M1, 0) => Some(3),
        (Version::M2, 0) => Some(4),
        (Version::M2, 1) => Some(3),
        (Version::M3, 0) => Some(5),
        (Version::M3, 1) => Some(4),
        (Version::M3, 2) => Some(4),
        (Version::M4, 0) => Some(6),
        (Version::M4, 1) => Some(5),
        (Version::M4, 2) => Some(5),
        _ => None,
    }
}

pub(crate) fn spec_for(version: Version, level: Level) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.version == version && s.level == level)
}

/// Content-based mode detection: all ASCII digits map to numeric, the 45-char
/// QR alphanumeric set to alnum, else byte.
pub fn detect(payload: &[u8]) -> Mode {
    if !payload.is_empty() && payload.iter().all(|b| b.is_ascii_digit()) {
        return Mode::Numeric;
    }
    if payload.iter().all(|b| ALNUM.contains(b)) {
        return Mode::Alnum;
    }
    Mode::Byte
}

/// Split a payload length into `(char-count, bit-width)` fields for `mode`.
pub fn fields(len: usize, mode: Mode) -> Vec<(usize, usize)> {
    match mode {
        Mode::Byte => vec![(1, 8); len],
        Mode::Alnum => {
            let mut f = vec![(2, 11); len / 2];
            if len % 2 == 1 {
                f.push((1, 6));
            }
            f
        }
        Mode::Numeric => {
            let mut rem = len;
            let mut f = Vec::new();
            while rem >= 3 {
                f.push((3, 10));
                rem -= 3;
            }
            match rem {
                2 => f.push((2, 7)),
                1 => f.push((1, 4)),
                _ => {}
            }
            f
        }
    }
}

/// Encode a text payload with automatic mode detection.
///
/// `mask` selects an ISO data mask pattern (`0..=3`); `None` picks the
/// ISO/IEC 18004 optimal mask automatically. Returns `None` for empty
/// payloads or payloads outside the version/level/mode capacity.
pub fn encode(
    payload: &str,
    version: Version,
    level: Level,
    mask: Option<usize>,
) -> Option<Symbol> {
    encode_bytes(payload.as_bytes(), version, level, None, mask)
}

/// Encode a byte payload with an explicit mode (or `None` to auto-detect) and
/// an optional ISO mask (`0..=3`, or `None` for automatic mask selection).
pub fn encode_bytes(
    payload: &[u8],
    version: Version,
    level: Level,
    mode: Option<Mode>,
    mask: Option<usize>,
) -> Option<Symbol> {
    if payload.is_empty() {
        return None;
    }
    let spec = spec_for(version, level)?;
    if let Some(m) = mask {
        if m >= 4 {
            return None;
        }
    }
    let mode = mode.unwrap_or_else(|| detect(payload));
    let cb = count_bits(version, mode.code())?;
    if payload.len() >= (1usize << cb) {
        return None;
    }
    // Validate characters for the selected mode.
    match mode {
        Mode::Numeric => {
            if !payload.iter().all(|b| b.is_ascii_digit()) {
                return None;
            }
        }
        Mode::Alnum => {
            if !payload.iter().all(|b| ALNUM.contains(b)) {
                return None;
            }
        }
        Mode::Byte => {}
    }

    // Build data bitstream: mode indicator + char count + payload fields.
    let mut bits = Vec::with_capacity(spec.data_bits);
    let push_bits = |bits: &mut Vec<u8>, val: u32, n: usize| {
        for i in (0..n).rev() {
            bits.push(((val >> i) & 1) as u8);
        }
    };
    let mb = mode_bits(version);
    if mb > 0 {
        push_bits(&mut bits, u32::from(mode.code()), mb);
    }
    push_bits(&mut bits, payload.len() as u32, cb);

    match mode {
        Mode::Numeric => {
            let mut i = 0;
            while i + 3 <= payload.len() {
                let v = u32::from(payload[i] - b'0') * 100
                    + u32::from(payload[i + 1] - b'0') * 10
                    + u32::from(payload[i + 2] - b'0');
                push_bits(&mut bits, v, 10);
                i += 3;
            }
            match payload.len() - i {
                2 => {
                    let v = u32::from(payload[i] - b'0') * 10 + u32::from(payload[i + 1] - b'0');
                    push_bits(&mut bits, v, 7);
                }
                1 => push_bits(&mut bits, u32::from(payload[i] - b'0'), 4),
                _ => {}
            }
        }
        Mode::Alnum => {
            let mut i = 0;
            while i + 2 <= payload.len() {
                let a = ALNUM.iter().position(|&c| c == payload[i])? as u32;
                let b = ALNUM.iter().position(|&c| c == payload[i + 1])? as u32;
                push_bits(&mut bits, a * 45 + b, 11);
                i += 2;
            }
            if i < payload.len() {
                let a = ALNUM.iter().position(|&c| c == payload[i])? as u32;
                push_bits(&mut bits, a, 6);
            }
        }
        Mode::Byte => {
            for &b in payload {
                push_bits(&mut bits, u32::from(b), 8);
            }
        }
    }

    if bits.len() > spec.data_bits {
        return None;
    }

    // Terminator (truncated to remaining capacity).
    let term = term_bits(version).min(spec.data_bits - bits.len());
    bits.resize(bits.len() + term, 0);

    // Pad to 8-bit boundary and fill remaining full data codewords with 0xEC/0x11.
    let full_bytes = spec.dc - usize::from(spec.half_last);
    if bits.len() < full_bytes * 8 {
        while bits.len() % 8 != 0 {
            bits.push(0);
        }
        let mut pad = 0usize;
        while bits.len() < full_bytes * 8 {
            push_bits(&mut bits, if pad % 2 == 0 { 0xEC } else { 0x11 }, 8);
            pad += 1;
        }
    }
    // Final 4-bit codeword padding (M1 and M3): zero bits.
    bits.resize(spec.data_bits, 0);

    // Pack data codewords for RS (M1/M3's 4-bit codeword occupies the upper nibble).
    let mut data_cws = Vec::with_capacity(spec.dc);
    for chunk in bits[..full_bytes * 8].chunks_exact(8) {
        data_cws.push(chunk.iter().fold(0u8, |acc, &b| (acc << 1) | b));
    }
    if spec.half_last {
        let nib = bits[full_bytes * 8..spec.data_bits]
            .iter()
            .fold(0u8, |acc, &b| (acc << 1) | b);
        data_cws.push(nib << 4);
    }

    // Compute Reed-Solomon parity over GF(2^8) and append to the bit stream.
    let gf = Gf::new();
    let parity = rs_encode(&gf, &data_cws, spec.ec);
    let mut stream = bits;
    stream.reserve(spec.ec * 8);
    for p in parity {
        push_bits(&mut stream, u32::from(p), 8);
    }

    let size = spec.size;
    let render = |m: u8| -> Vec<u8> {
        let mut cells = vec![0u8; size * size];
        // 7x7 finder at top-left.
        for y in 0..7 {
            for x in 0..7 {
                if x == 0 || x == 6 || y == 0 || y == 6 || ((2..=4).contains(&x) && (2..=4).contains(&y)) {
                    cells[y * size + x] = 1;
                }
            }
        }
        // Timing patterns along row 0 and col 0.
        for k in 8..size {
            let dark = u8::from(k % 2 == 0);
            cells[k] = dark;
            cells[k * size] = dark;
        }
        // 15-bit format strip.
        let fw = format_word(spec.sym, m);
        for (idx, &(x, y)) in FORMAT_COORDS.iter().enumerate() {
            cells[y * size + x] = ((fw >> (14 - idx)) & 1) as u8;
        }
        // Zigzag data + EC placement (right-to-left 2-col strips, bottom-up first).
        let mut bi = 0usize;
        let mut up = true;
        let mut col = size - 1;
        while col >= 1 {
            for step in 0..size - 1 {
                let y = if up { size - 1 - step } else { 1 + step };
                for x in [col, col - 1] {
                    if is_function(x, y) {
                        continue;
                    }
                    let bit = stream[bi] ^ u8::from(mask_bit(m, x, y));
                    bi += 1;
                    cells[y * size + x] = bit;
                }
            }
            up = !up;
            if col < 2 {
                break;
            }
            col -= 2;
        }
        debug_assert_eq!(bi, stream.len());
        cells
    };

    let cells = if let Some(m) = mask {
        render(m as u8)
    } else {
        // ISO/IEC 18004 §7.8.3.2 mask scoring: maximize min(s1, s2)*16 + max(s1, s2)
        // over dark module counts on the right and bottom borders (excluding timing).
        let mut best_score = 0usize;
        let mut best_cells = Vec::new();
        for m in 0..4u8 {
            let candidate = render(m);
            let s1: usize = (1..size).map(|y| usize::from(candidate[y * size + (size - 1)])).sum();
            let s2: usize = (1..size).map(|x| usize::from(candidate[(size - 1) * size + x])).sum();
            let score = s1.min(s2) * 16 + s1.max(s2);
            if best_cells.is_empty() || score > best_score {
                best_score = score;
                best_cells = candidate;
            }
        }
        best_cells
    };

    Some(Symbol { size, cells })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_payloads() {
        assert!(encode("", Version::M1, Level::L, None).is_none());
        assert!(encode("12a5", Version::M1, Level::L, None).is_none());
        assert!(encode("123456", Version::M1, Level::L, None).is_none());
        // M1 has no M/Q level; M2/M3 have no Q level
        assert!(encode("12", Version::M1, Level::M, None).is_none());
        assert!(encode("12", Version::M2, Level::Q, None).is_none());
        assert!(encode("12", Version::M3, Level::Q, None).is_none());
        // M1/M2 do not support byte mode; M1 does not support alnum
        assert!(encode_bytes(b"AB", Version::M1, Level::L, Some(Mode::Alnum), None).is_none());
        assert!(encode_bytes(b"ab", Version::M2, Level::L, Some(Mode::Byte), None).is_none());
        // mask out of range (only 0..=3 exist in ISO 18004)
        assert!(encode("12345", Version::M1, Level::L, Some(4)).is_none());
        assert!(encode("12345", Version::M1, Level::L, Some(99)).is_none());
    }

    #[test]
    fn sizes_and_finder() {
        let s = encode("42", Version::M1, Level::L, None).unwrap();
        assert_eq!(s.size, 11);
        assert!(s.ink(0, 0) && s.ink(6, 0) && s.ink(0, 6));
        let s = encode("1234567890", Version::M2, Level::L, None).unwrap();
        assert_eq!(s.size, 13);
        let s = encode("12345678901234567890123", Version::M3, Level::L, None).unwrap();
        assert_eq!(s.size, 15);
        let s = encode("12345678901234567890123456789012345", Version::M4, Level::L, None).unwrap();
        assert_eq!(s.size, 17);
    }

    #[test]
    fn explicit_modes_work_on_all_valid_payloads() {
        // Explicit Alnum on digits and explicit Byte on ASCII are valid ISO 18004
        // (fixing binary render() on ASCII raw payloads).
        assert!(encode_bytes(b"12", Version::M2, Level::L, Some(Mode::Alnum), None).is_some());
        assert!(encode_bytes(b"HELLO", Version::M3, Level::L, Some(Mode::Byte), None).is_some());
        assert!(encode_bytes(b"0123456789abcde", Version::M4, Level::L, Some(Mode::Byte), None).is_some());
        assert!(encode_bytes(b"012345678", Version::M4, Level::Q, Some(Mode::Byte), None).is_some());
    }

    #[test]
    fn modes_detect() {
        assert_eq!(detect(b"123"), Mode::Numeric);
        assert_eq!(detect(b"A1"), Mode::Alnum);
        assert_eq!(detect(b"\x00"), Mode::Byte);
    }
}
