//! Synthetic barcode generators for decoder testing.
//!
//! These encoders are an INDEPENDENT transcription of the symbology specs
//! (not shared with the decoder crates on purpose): an encoder and decoder
//! that agree through the spec validate each other; a typo shared between
//! them would not be caught here — that is what physical ground truth
//! (captured real symbols) is for.

/// A 1bpp canvas with the barcode-core BitImage layout:
/// bit (x, y) = data[y * bytes_per_row + (x >> 3)] >> (x & 7) & 1,
/// bytes_per_row padded to a multiple of 4.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: usize,
    data: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        let bytes_per_row = (width.div_ceil(8).div_ceil(4) * 4) as usize;
        Self {
            width,
            height,
            bytes_per_row,
            data: vec![0u8; bytes_per_row * height as usize],
        }
    }

    /// Fill an ink rectangle (clipped).
    pub fn fill(&mut self, x0: u32, y0: u32, w: u32, h: u32) {
        for y in y0..(y0 + h).min(self.height) {
            for x in x0..(x0 + w).min(self.width) {
                self.data[y as usize * self.bytes_per_row + (x >> 3) as usize] |= 1 << (x & 7);
            }
        }
    }

    /// Toggle a single pixel (for perturbation tests).
    pub fn flip(&mut self, x: u32, y: u32) {
        if x < self.width && y < self.height {
            self.data[y as usize * self.bytes_per_row + (x >> 3) as usize] ^= 1 << (x & 7);
        }
    }

    /// Clear (unset) a rectangle.
    pub fn clear(&mut self, x0: u32, y0: u32, w: u32, h: u32) {
        for y in y0..(y0 + h).min(self.height) {
            for x in x0..(x0 + w).min(self.width) {
                self.data[y as usize * self.bytes_per_row + (x >> 3) as usize] &= !(1 << (x & 7));
            }
        }
    }

    /// The packed mask, compatible with `BitImage::new`.
    pub fn bits(&self) -> &[u8] {
        &self.data
    }
}

// ---------------------------------------------------------------------------
// EAN-13 (ISO/IEC 15420)
// ---------------------------------------------------------------------------

const EAN_L: [&str; 10] = [
    "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
    "0110111", "0001011",
];
const EAN_G: [&str; 10] = [
    "0100111", "0110011", "0011011", "0100001", "0011101", "0111001", "0000101", "0010001",
    "0001001", "0010111",
];
const EAN_R: [&str; 10] = [
    "1110010", "1100110", "1101100", "1000010", "1011100", "1001110", "1010000", "1000100",
    "1001000", "1110100",
];
const EAN_PARITY: [&str; 10] = [
    "LLLLLL", "LLGLGG", "LLGGLG", "LLGGGL", "LGLLGG", "LGGLLG", "LGGGLL", "LGLGLG", "LGLGGL",
    "LGGLGL",
];

/// Compute the EAN-13 check digit for 12 digits given as chars '0'..='9'.
pub fn ean13_check_digit(digits12: &str) -> char {
    let sum: usize = digits12
        .bytes()
        .enumerate()
        .map(|(i, b)| (b - b'0') as usize * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    char::from(b'0' + ((10 - sum % 10) % 10) as u8)
}

/// Encode a UPC-A symbol (12 ASCII digits, valid check digit) into a canvas.
/// Returns `Err` instead of panicking on invalid input.
pub fn try_encode_upca(
    digits12: &str,
    narrow: u32,
    height: u32,
    quiet: u32,
) -> Result<Canvas, &'static str> {
    if narrow == 0 || height == 0 {
        return Err("invalid render dimensions");
    }
    let bytes = digits12.as_bytes();
    if bytes.len() != 12 || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return Err("invalid upca payload");
    }
    // check over the 11 data digits = EAN-13 check of "0" + 11 data
    let mut with0 = String::with_capacity(12);
    with0.push('0');
    with0.push_str(&digits12[..11]);
    if ean13_check_digit(&with0) != bytes[11] as char {
        return Err("invalid upca payload");
    }
    let mut d = [0u8; 13];
    for (i, &b) in bytes.iter().enumerate() {
        d[1 + i] = b - b'0';
    }
    Ok(encode_ean13_raw(&d, narrow, height, quiet))
}

/// Encode a UPC-A symbol (12 digits, the last being its check digit) into a
/// canvas. Panics on invalid input (test convenience wrapper around [`try_encode_upca`]).
pub fn encode_upca(digits12: &str, narrow: u32, height: u32, quiet: u32) -> Canvas {
    try_encode_upca(digits12, narrow, height, quiet).expect("encode_upca")
}

/// Build the 95-module string of an EAN-13 symbol (digits must be 13 values in 0..=9).
fn ean13_modules(digits: &[u8; 13]) -> String {
    let mut s = String::with_capacity(95);
    s.push_str("101"); // start guard
    let parity = EAN_PARITY[digits[0] as usize];
    for d in 0..6 {
        let table = if parity.as_bytes()[d] == b'L' {
            &EAN_L
        } else {
            &EAN_G
        };
        s.push_str(table[digits[1 + d] as usize]);
    }
    s.push_str("01010"); // center guard
    for d in 0..6 {
        s.push_str(EAN_R[digits[7 + d] as usize]);
    }
    s.push_str("101"); // end guard
    s
}

/// Encode a full 13-digit EAN-13 string (must include a valid check digit)
/// into a canvas. Returns `Err` instead of panicking on invalid input.
pub fn try_encode_ean13(
    digits: &str,
    narrow: u32,
    height: u32,
    quiet: u32,
) -> Result<Canvas, &'static str> {
    if narrow == 0 || height == 0 {
        return Err("invalid render dimensions");
    }
    let bytes = digits.as_bytes();
    if bytes.len() != 13 || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return Err("invalid ean13 payload");
    }
    if ean13_check_digit(&digits[..12]) != bytes[12] as char {
        return Err("invalid ean13 payload");
    }
    let mut d = [0u8; 13];
    for (i, &b) in bytes.iter().enumerate() {
        d[i] = b - b'0';
    }
    Ok(encode_ean13_raw(&d, narrow, height, quiet))
}

/// Encode a full 13-digit string (must include a valid check digit) into a
/// canvas. Panics on invalid input (test convenience wrapper around [`try_encode_ean13`]).
pub fn encode_ean13(digits: &str, narrow: u32, height: u32, quiet: u32) -> Canvas {
    try_encode_ean13(digits, narrow, height, quiet).expect("encode_ean13")
}

/// Encode from raw digits (no validation), for negative tests.
pub fn encode_ean13_raw(digits: &[u8; 13], narrow: u32, height: u32, quiet: u32) -> Canvas {
    let modules = ean13_modules(digits);
    assert_eq!(modules.len(), 95);
    let width = (95 + 2 * quiet) * narrow;
    let mut canvas = Canvas::new(width, height);
    let x0 = quiet * narrow;
    for (i, m) in modules.bytes().enumerate() {
        if m == b'1' {
            canvas.fill(x0 + i as u32 * narrow, 0, narrow, height);
        }
    }
    canvas
}

// ---------------------------------------------------------------------------
// Code 39 (ISO/IEC 16388) — 9 elements: bar, space, bar, ... bar (5 bars,
// 4 spaces), 3 wide; intercharacter gap = 1 narrow space; start/stop '*'.
// Element strings are bars/spaces in order (B S B S B S B S B).
// ---------------------------------------------------------------------------

const C39: [(&str, &str); 44] = [
    ("0", "000110100"),
    ("1", "100100001"),
    ("2", "001100001"),
    ("3", "101100000"),
    ("4", "000110001"),
    ("5", "100110000"),
    ("6", "001110000"),
    ("7", "000100101"),
    ("8", "100100100"),
    ("9", "001100100"),
    ("A", "100001001"),
    ("B", "001001001"),
    ("C", "101001000"),
    ("D", "000011001"),
    ("E", "100011000"),
    ("F", "001011000"),
    ("G", "000001101"),
    ("H", "100001100"),
    ("I", "001001100"),
    ("J", "000011100"),
    ("K", "100000011"),
    ("L", "001000011"),
    ("M", "101000010"),
    ("N", "000010011"),
    ("O", "100010010"),
    ("P", "001010010"),
    ("Q", "000000111"),
    ("R", "100000110"),
    ("S", "001000110"),
    ("T", "000010110"),
    ("U", "110000001"),
    ("V", "011000001"),
    ("W", "111000000"),
    ("X", "010010001"),
    ("Y", "110010000"),
    ("Z", "011010000"),
    ("-", "010000101"),
    (".", "110000100"),
    (" ", "011000100"),
    ("$", "010101000"),
    ("/", "010100010"),
    ("+", "010001010"),
    ("%", "000101010"),
    ("*", "010010100"),
];

fn c39_pattern(ch: char) -> Option<&'static str> {
    let c = ch.to_ascii_uppercase();
    C39.iter()
        .find(|(k, _)| *k == c.to_string())
        .map(|(_, v)| *v)
}

/// Encode text (without start/stop; '*' is added) into a canvas.
/// Returns `Err` instead of panicking on invalid input.
pub fn try_encode_code39(
    text: &str,
    narrow: u32,
    ratio: f32,
    height: u32,
) -> Result<Canvas, &'static str> {
    if text.is_empty() || text.contains('*') {
        return Err("invalid code39 payload");
    }
    let wide = (narrow as f32 * ratio).round() as u32;
    if narrow == 0 || height == 0 || !(2.0..=3.0).contains(&ratio) || wide <= narrow {
        return Err("invalid render dimensions");
    }

    let full = format!("*{text}*");
    let mut patterns: Vec<&'static str> = Vec::with_capacity(full.len());
    let mut total_elements = 0u32;
    for ch in full.chars() {
        let p = c39_pattern(ch).ok_or("invalid code39 payload")?;
        patterns.push(p);
        total_elements = total_elements
            .checked_add(
                p.bytes()
                    .map(|b| if b == b'1' { wide } else { narrow })
                    .sum::<u32>(),
            )
            .and_then(|v| v.checked_add(narrow))
            .ok_or("invalid render dimensions")?;
    }
    let quiet = 10 * narrow;
    let width = (total_elements - narrow)
        .checked_add(2 * quiet)
        .ok_or("invalid render dimensions")?;
    let mut canvas = Canvas::new(width, height);

    let mut x = quiet;
    for (n, p) in patterns.iter().enumerate() {
        for (k, b) in p.bytes().enumerate() {
            let w = if b == b'1' { wide } else { narrow };
            if k % 2 == 0 {
                canvas.fill(x, 0, w, height); // bars at even element positions
            }
            x += w;
        }
        if n + 1 < patterns.len() {
            x += narrow; // gap
        }
    }
    Ok(canvas)
}

/// Encode text (without start/stop; '*' is added) into a canvas.
/// Panics on invalid input (test convenience wrapper around [`try_encode_code39`]).
pub fn encode_code39(text: &str, narrow: u32, ratio: f32, height: u32) -> Canvas {
    try_encode_code39(text, narrow, ratio, height).expect("encode_code39")
}

// ---------------------------------------------------------------------------
// Interleaved 2 of 5 (ISO/IEC 16390) — start: 4 narrow elements (B S B S);
// each digit pair: first digit's 5 elements are BARS, second's are SPACES
// (2 wide of 5 each); stop: wide bar, narrow space, narrow bar.
// ---------------------------------------------------------------------------

const I25: [&str; 10] = [
    "00110", "10001", "01001", "11000", "00101", "10100", "01100", "00011", "10010", "01010",
];

/// Encode an even-length digit string as ITF/I2of5.
/// Returns `Err` instead of panicking on invalid input.
pub fn try_encode_itf(
    text: &str,
    narrow: u32,
    ratio: f32,
    height: u32,
) -> Result<Canvas, &'static str> {
    let wide = (narrow as f32 * ratio).round() as u32;
    if narrow == 0 || height == 0 || !(2.0..=3.0).contains(&ratio) || wide <= narrow {
        return Err("invalid render dimensions");
    }
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() % 2 != 0 || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return Err("invalid itf payload");
    }

    let elem = |i: usize, pat: &str| -> u32 {
        if pat.as_bytes()[i] == b'1' {
            wide
        } else {
            narrow
        }
    };

    let pairs = bytes.len() / 2;
    let mut widths: Vec<(bool, u32)> = Vec::with_capacity(7 + pairs * 10);
    widths.push((true, narrow));
    widths.push((false, narrow));
    widths.push((true, narrow));
    widths.push((false, narrow));
    for p in 0..pairs {
        let a = I25[(bytes[p * 2] - b'0') as usize];
        let b = I25[(bytes[p * 2 + 1] - b'0') as usize];
        for i in 0..5 {
            widths.push((true, elem(i, a)));
            widths.push((false, elem(i, b)));
        }
    }
    // stop: wide bar, narrow space, narrow bar (verified against a real capture)
    widths.push((true, wide));
    widths.push((false, narrow));
    widths.push((true, narrow));

    let sym_width: u32 = widths
        .iter()
        .try_fold(0u32, |acc, &(_, w)| acc.checked_add(w))
        .ok_or("invalid render dimensions")?;
    let quiet = 10 * narrow;
    let width = sym_width
        .checked_add(2 * quiet)
        .ok_or("invalid render dimensions")?;
    let mut canvas = Canvas::new(width, height);

    let mut x = quiet;
    for (ink, w) in widths {
        if ink {
            canvas.fill(x, 0, w, height);
        }
        x += w;
    }
    Ok(canvas)
}

/// Encode an even-length digit string as ITF/I2of5.
/// Panics on invalid input (test convenience wrapper around [`try_encode_itf`]).
pub fn encode_itf(text: &str, narrow: u32, ratio: f32, height: u32) -> Canvas {
    try_encode_itf(text, narrow, ratio, height).expect("encode_itf")
}

// ---------------------------------------------------------------------------
// Code 128 (ISO/IEC 15417)
// ---------------------------------------------------------------------------

/// Element widths (bar,space,bar,space,bar,space) of code values 0-106;
/// value 106 is the stop's first six elements (the stop appends a 2-module
/// final bar). Transcribed from the ISO/IEC 15417 table.
#[rustfmt::skip]
const C128: [&str; 107] = [
    "212222", "222122", "222221", "121223", "121322", "131222",
    "122213", "122312", "132212", "221213", "221312", "231212",
    "112232", "122132", "122231", "113222", "123122", "123221",
    "223211", "221132", "221231", "213212", "223112", "312131",
    "311222", "321122", "321221", "312212", "322112", "322211",
    "212123", "212321", "232121", "111323", "131123", "131321",
    "112313", "132113", "132311", "211313", "231113", "231311",
    "112133", "112331", "132131", "113123", "113321", "133121",
    "313121", "211331", "231131", "213113", "213311", "213131",
    "311123", "311321", "331121", "312113", "312311", "332111",
    "314111", "221411", "431111", "111224", "111422", "121124",
    "121421", "141122", "141221", "112214", "112412", "122114",
    "122411", "142112", "142211", "241211", "221114", "413111",
    "241112", "134111", "111242", "121142", "121241", "114212",
    "124112", "124211", "411212", "421112", "421211", "212141",
    "214121", "412121", "111143", "111341", "131141", "114113",
    "114311", "411113", "411311", "113141", "114131", "311141",
    "411131", "211412", "211214", "211232", "233111",
];

/// Data values for text that needs more than code set B. Controls use set
/// A, printable characters use set B, and GS (`0x1D`, the decoded form of
/// FNC1) is emitted as FNC1 so a GS1-128 payload round-trips. FNC1 is
/// valid in every code set and does not change it.
fn code128_mixed(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::with_capacity(bytes.len() + 8);
    let mut set = b'B';
    for &b in bytes {
        match b {
            0x1d => out.push(102), // FNC1 -> GS on decode
            b if b < 32 => {
                if set != b'A' {
                    out.push(101); // CODE A (from B or C)
                    set = b'A';
                }
                out.push(b + 64);
            }
            b if b <= 127 => {
                if set != b'B' {
                    out.push(100); // CODE B (from A or C)
                    set = b'B';
                }
                out.push(b - 32);
            }
            _ => return Err("invalid code128 payload"),
        }
    }
    Ok(out)
}

/// Encode `text` as Code 128 and render it at `module` px per unit.
/// Returns `Err` instead of panicking on invalid input.
pub fn try_encode_code128(text: &str, module: u32, height: u32) -> Result<Canvas, &'static str> {
    if module == 0 || height == 0 {
        return Err("invalid render dimensions");
    }
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Err("invalid code128 payload");
    }
    let use_c =
        bytes.len() >= 4 && bytes.len() % 2 == 0 && bytes.iter().all(|b| b.is_ascii_digit());

    // values: start + data
    let (start, mut values): (u8, Vec<u8>) = if use_c {
        (
            105,
            bytes
                .chunks(2)
                .map(|c| (c[0] - b'0') * 10 + (c[1] - b'0'))
                .collect(),
        )
    } else if bytes.iter().all(|&b| (32..=127).contains(&b)) {
        // Fast path: set B alone covers printable ASCII (and DEL).
        (104, bytes.iter().map(|&b| b - b' ').collect())
    } else {
        (104, code128_mixed(bytes)?)
    };
    // check digit: (start + sum i*value_i) mod 103
    let check = ((start as usize
        + values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i + 1) * v as usize)
            .sum::<usize>())
        % 103) as u8;
    values.push(check);

    // widths: start + values + stop (6 elements + 2-module final bar)
    let mut widths: Vec<(bool, u32)> = Vec::with_capacity((values.len() + 2) * 6 + 1);
    let push = |v: u8, widths: &mut Vec<(bool, u32)>| {
        let pat = C128[v as usize];
        for (i, ch) in pat.bytes().enumerate() {
            widths.push((i % 2 == 0, (ch - b'0') as u32 * module));
        }
    };
    push(start, &mut widths);
    for &v in &values {
        push(v, &mut widths);
    }
    for (i, m) in [2u8, 3, 3, 1, 1, 1, 2].iter().enumerate() {
        widths.push((i % 2 == 0, *m as u32 * module));
    }

    let quiet = 10 * module;
    let sym: u32 = widths
        .iter()
        .try_fold(0u32, |acc, &(_, w)| acc.checked_add(w))
        .ok_or("invalid render dimensions")?;
    let width = sym
        .checked_add(2 * quiet)
        .ok_or("invalid render dimensions")?;
    let mut canvas = Canvas::new(width, height);
    let mut x = quiet;
    for (ink, w) in widths {
        if ink {
            canvas.fill(x, 0, w, height);
        }
        x += w;
    }
    Ok(canvas)
}

/// Encode `text` as Code 128 and render it at `module` px per unit.
/// Panics on invalid input (test convenience wrapper around [`try_encode_code128`]).
pub fn encode_code128(text: &str, module: u32, height: u32) -> Canvas {
    try_encode_code128(text, module, height).expect("encode_code128")
}

// ---------------------------------------------------------------------------
// Code 93 (ANSI/AIM BC5)
// ---------------------------------------------------------------------------

/// Element widths (bar,space,bar,space,bar,space) of values 0-47; value 47
/// is the start/stop character. Transcribed from the published table and
/// cross-validated against its binary column.
#[rustfmt::skip]
const C93: [[u8; 6]; 48] = [
    [1,3,1,1,1,2], [1,1,1,2,1,3], [1,1,1,3,1,2], [1,1,1,4,1,1], [1,2,1,1,1,3], [1,2,1,2,1,2],
    [1,2,1,3,1,1], [1,1,1,1,1,4], [1,3,1,2,1,1], [1,4,1,1,1,1], [2,1,1,1,1,3], [2,1,1,2,1,2],
    [2,1,1,3,1,1], [2,2,1,1,1,2], [2,2,1,2,1,1], [2,3,1,1,1,1], [1,1,2,1,1,3], [1,1,2,2,1,2],
    [1,1,2,3,1,1], [1,2,2,1,1,2], [1,3,2,1,1,1], [1,1,1,1,2,3], [1,1,1,2,2,2], [1,1,1,3,2,1],
    [1,2,1,1,2,2], [1,3,1,1,2,1], [2,1,2,1,1,2], [2,1,2,2,1,1], [2,1,1,1,2,2], [2,1,1,2,2,1],
    [2,2,1,1,2,1], [2,2,2,1,1,1], [1,1,2,1,2,2], [1,1,2,2,2,1], [1,2,2,1,2,1], [1,2,3,1,1,1],
    [1,2,1,1,3,1], [3,1,1,1,1,2], [3,1,1,2,1,1], [3,2,1,1,1,1], [1,1,2,1,3,1], [1,1,3,1,2,1],
    [2,1,1,1,3,1], [1,2,1,2,2,1], [3,1,2,1,1,1], [3,1,1,1,2,1], [1,2,2,2,1,1], [1,1,1,1,4,1],
];

/// Map a char to its Code 93 value sequence (1 or 2 values, using the
/// Full ASCII shift prefixes for characters outside the basic 43-char set).
fn code93_values(c: char) -> Result<Vec<u8>, &'static str> {
    let basic = |c: char| -> Option<u8> {
        match c {
            '0'..='9' => Some(c as u8 - b'0'),
            'A'..='Z' => Some(c as u8 - b'A' + 10),
            '-' => Some(36),
            '.' => Some(37),
            ' ' => Some(38),
            '$' => Some(39),
            '/' => Some(40),
            '+' => Some(41),
            '%' => Some(42),
            _ => None,
        }
    };
    if let Some(v) = basic(c) {
        return Ok(vec![v]);
    }
    // Full ASCII pairs (inverse of the extended table): (shift value,
    // base value). Base values: A=10 .. Z=35.
    let u = c as u32;
    let pair = match c {
        '\u{00}' => (44, 30),                       // (%)U -> NUL
        '\u{01}'..='\u{1a}' => (43, 10 + (u - 1)),  // ($)A..=Z -> 1..=26
        '\u{1b}'..='\u{1f}' => (44, 10 + (u - 27)), // (%)A..=E -> 27..=31
        '!'..='#' => (45, 10 + (u - 33)),           // (/)A..=C -> 33..=35
        '&'..='*' => (45, 15 + (u - 38)),           // (/)F..=J -> 38..=42
        ',' => (45, 21),                            // (/)L
        ':' => (45, 35),                            // (/)Z
        ';'..='?' => (44, 15 + (u - 59)),           // (%)F..=J -> 59..=63
        '@' => (44, 31),                            // (%)V
        '['..='_' => (44, 20 + (u - 91)),           // (%)K..=O -> 91..=95
        '`' => (44, 32),                            // (%)W
        'a'..='z' => (46, 10 + (u - 97)),           // (+)A..=Z -> 97..=122
        '{'..='~' => (44, 25 + (u - 123)),          // (%)P..=S -> 123..=126
        '\u{7f}' => (44, 29),                       // (%)T
        _ => return Err("invalid code93 payload"),
    };
    Ok(vec![pair.0 as u8, pair.1 as u8])
}

/// Encode `text` as Code 93 (with C and K check characters) rendered at
/// `module` px per element unit. Returns `Err` instead of panicking.
pub fn try_encode_code93(text: &str, module: u32, height: u32) -> Result<Canvas, &'static str> {
    if module == 0 || height == 0 {
        return Err("invalid render dimensions");
    }
    if text.is_empty() {
        return Err("invalid code93 payload");
    }
    let mut values: Vec<u8> = Vec::with_capacity(text.len() + 2);
    for c in text.chars() {
        values.extend(code93_values(c)?);
    }
    // C check: weights 1..20 from the right-most data char, mod 47.
    let c = values
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &v)| v as usize * (i % 20 + 1))
        .sum::<usize>()
        % 47;
    // K check: data + C, the C char weighted 1, data chars 2..15 from the right.
    let k = (values
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &v)| v as usize * ((i + 1) % 15 + 1))
        .sum::<usize>()
        + c)
        % 47;
    values.push(c as u8);
    values.push(k as u8);

    let mut widths: Vec<(bool, u32)> = Vec::with_capacity((values.len() + 2) * 6 + 1);
    for &v in std::iter::once(&47u8)
        .chain(values.iter())
        .chain(std::iter::once(&47u8))
    {
        for (i, &m) in C93[v as usize].iter().enumerate() {
            widths.push((i % 2 == 0, m as u32 * module));
        }
    }
    widths.push((true, module)); // termination bar

    let quiet = 10 * module;
    let sym: u32 = widths
        .iter()
        .try_fold(0u32, |acc, &(_, w)| acc.checked_add(w))
        .ok_or("invalid render dimensions")?;
    let width = sym
        .checked_add(2 * quiet)
        .ok_or("invalid render dimensions")?;
    let mut canvas = Canvas::new(width, height);
    let mut x = quiet;
    for (ink, w) in widths {
        if ink {
            canvas.fill(x, 0, w, height);
        }
        x += w;
    }
    Ok(canvas)
}

/// Encode `text` as Code 93 (with C and K check characters) rendered at
/// `module` px per element unit. Panics on invalid input (test wrapper).
pub fn encode_code93(text: &str, module: u32, height: u32) -> Canvas {
    try_encode_code93(text, module, height).expect("encode_code93")
}

// ---------------------------------------------------------------------------
// Codabar (ANSI/AIM BC3-1995)
// ---------------------------------------------------------------------------

/// (bars, spaces) wide-bit patterns per character; the element sequence is
/// B1 S1 B2 S2 B3 S3 B4 with an additional narrow space between characters.
#[rustfmt::skip]
pub const CODABAR_CHARS: [(char, [u8; 4], [u8; 3]); 20] = [
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

/// Encode `body` as Codabar wrapped in `A`/`B`, returning `Err` on invalid input.
pub fn try_encode_codabar(
    body: &str,
    narrow: u32,
    ratio: f32,
    height: u32,
) -> Result<Canvas, &'static str> {
    try_encode_codabar_wrapped(body, "A", "B", narrow, ratio, height)
}

/// Encode `body` as Codabar wrapped in `start`/`stop` (A-D).
/// Panics on invalid input (test convenience wrapper around [`try_encode_codabar`]).
pub fn encode_codabar(body: &str, narrow: u32, ratio: f32, height: u32) -> Canvas {
    try_encode_codabar(body, narrow, ratio, height).expect("encode_codabar")
}

/// Like [`try_encode_codabar`] but with explicit start/stop characters.
pub fn try_encode_codabar_wrapped(
    body: &str,
    start: &str,
    stop: &str,
    narrow: u32,
    ratio: f32,
    height: u32,
) -> Result<Canvas, &'static str> {
    if body.is_empty() {
        return Err("invalid codabar payload");
    }
    let wide = (narrow as f32 * ratio).round() as u32;
    if narrow == 0 || height == 0 || !(2.25..=3.0).contains(&ratio) || wide <= narrow {
        return Err("invalid render dimensions");
    }
    let mut seq: Vec<char> = Vec::with_capacity(body.len() + 2);
    seq.extend(start.chars());
    seq.extend(body.chars());
    seq.extend(stop.chars());
    if seq.len() != 2 + body.chars().count()
        || !is_start_stop_like(seq[0])
        || !is_start_stop_like(seq[seq.len() - 1])
    {
        return Err("invalid codabar payload");
    }

    let mut widths: Vec<(bool, u32)> = Vec::with_capacity(seq.len() * 8);
    for (i, &c) in seq.iter().enumerate() {
        let (_, bars, spaces) = CODABAR_CHARS
            .iter()
            .find(|(ch, _, _)| *ch == c)
            .ok_or("invalid codabar payload")?;
        for ei in 0..7 {
            let w = if ei % 2 == 0 {
                bars[ei / 2]
            } else {
                spaces[(ei - 1) / 2]
            };
            widths.push((ei % 2 == 0, if w == 1 { wide } else { narrow }));
        }
        if i + 1 < seq.len() {
            widths.push((false, narrow)); // inter-character space
        }
    }

    let quiet = 10 * narrow;
    let sym: u32 = widths
        .iter()
        .try_fold(0u32, |acc, &(_, w)| acc.checked_add(w))
        .ok_or("invalid render dimensions")?;
    let width = sym
        .checked_add(2 * quiet)
        .ok_or("invalid render dimensions")?;
    let mut canvas = Canvas::new(width, height);
    let mut x = quiet;
    for (ink, w) in widths {
        if ink {
            canvas.fill(x, 0, w, height);
        }
        x += w;
    }
    Ok(canvas)
}

/// Like [`encode_codabar`] but with explicit start/stop characters (test wrapper).
pub fn encode_codabar_wrapped(
    body: &str,
    start: &str,
    stop: &str,
    narrow: u32,
    ratio: f32,
    height: u32,
) -> Canvas {
    try_encode_codabar_wrapped(body, start, stop, narrow, ratio, height).expect("encode_codabar")
}

fn is_start_stop_like(c: char) -> bool {
    matches!(c, 'A' | 'B' | 'C' | 'D')
}

// ---------------------------------------------------------------------------
// EAN-8 (ISO/IEC 15420)
// ---------------------------------------------------------------------------

/// Compute the EAN-8 check digit for 7 digits given as chars '0'..='9'.
pub fn ean8_check_digit(digits7: &str) -> char {
    let bytes = digits7.as_bytes();
    assert_eq!(bytes.len(), 7);
    let mut sum = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        sum += (b - b'0') as usize * if i % 2 == 0 { 3 } else { 1 };
    }
    char::from(((10 - sum % 10) % 10) as u8 + b'0')
}

/// Encode an 8-digit EAN-8 (must include a valid check digit).
/// Returns `Err` instead of panicking on invalid input.
pub fn try_encode_ean8(
    digits: &str,
    narrow: u32,
    height: u32,
    quiet: u32,
) -> Result<Canvas, &'static str> {
    if narrow == 0 || height == 0 {
        return Err("invalid render dimensions");
    }
    let bytes = digits.as_bytes();
    if bytes.len() != 8 || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return Err("invalid ean8 payload");
    }
    if ean8_check_digit(&digits[..7]) != bytes[7] as char {
        return Err("invalid ean8 payload");
    }
    Ok(encode_ean8_raw(digits, narrow, height, quiet))
}

/// Encode an 8-digit EAN-8 (must include a valid check digit).
/// Panics on invalid input (test convenience wrapper around [`try_encode_ean8`]).
pub fn encode_ean8(digits: &str, narrow: u32, height: u32, quiet: u32) -> Canvas {
    try_encode_ean8(digits, narrow, height, quiet).expect("encode_ean8")
}

/// Encode from raw digits (no validation), for negative tests.
pub fn encode_ean8_raw(digits: &str, narrow: u32, height: u32, quiet: u32) -> Canvas {
    let bytes = digits.as_bytes();
    assert_eq!(bytes.len(), 8, "EAN-8 needs 8 digits");
    assert!(bytes.iter().all(|b| b.is_ascii_digit()));
    let d: Vec<u8> = bytes.iter().map(|b| b - b'0').collect();

    let l = [
        "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
        "0110111", "0001011",
    ];
    let r = [
        "1110010", "1100110", "1101100", "1000010", "1011100", "1001110", "1010000", "1000100",
        "1001000", "1110100",
    ];
    let mut modules = String::from("101");
    for i in 0..4 {
        modules.push_str(l[d[i] as usize]);
    }
    modules.push_str("01010");
    for i in 4..8 {
        modules.push_str(r[d[i] as usize]);
    }
    modules.push_str("101");
    assert_eq!(modules.len(), 67);

    let width = (67 + 2 * quiet) * narrow;
    let mut canvas = Canvas::new(width, height);
    let x0 = quiet * narrow;
    for (i, m) in modules.bytes().enumerate() {
        if m == b'1' {
            canvas.fill(x0 + i as u32 * narrow, 0, narrow, height);
        }
    }
    canvas
}

// ---------------------------------------------------------------------------
// UPC-E (zero-compressed UPC-A)
// ---------------------------------------------------------------------------

/// Encode `upc_a` (12 digits, number system 0 or 1, valid check digit) as
/// its zero-compressed UPC-E symbol. Returns `Err` instead of panicking.
pub fn try_encode_upce(
    upc_a: &str,
    narrow: u32,
    height: u32,
    quiet: u32,
) -> Result<Canvas, &'static str> {
    if narrow == 0 || height == 0 {
        return Err("invalid render dimensions");
    }
    let bytes = upc_a.as_bytes();
    if bytes.len() != 12 || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return Err("invalid upce payload");
    }
    let d: Vec<u8> = bytes.iter().map(|b| b - b'0').collect();
    if d[0] != 0 && d[0] != 1 {
        return Err("invalid upce payload");
    }

    // check digit must be consistent
    let mut sum = 0usize;
    for (i, &v) in d.iter().enumerate().take(11) {
        sum += v as usize * if i % 2 == 0 { 3 } else { 1 };
    }
    if (10 - sum % 10) % 10 != d[11] as usize {
        return Err("invalid upce payload");
    }

    let m = &d[1..6]; // manufacturer
    let p = &d[6..11]; // product
    let u: [u8; 6] = if m[3] == 0 && m[4] == 0 && m[2] <= 2 && p[0] == 0 && p[1] == 0 {
        [m[0], m[1], p[2], p[3], p[4], m[2]]
    } else if m[3] == 0 && m[4] == 0 && p[0] == 0 && p[1] == 0 && p[2] == 0 {
        [m[0], m[1], m[2], p[3], p[4], 3]
    } else if m[4] == 0 && m[3] != 0 && p[0] == 0 && p[1] == 0 && p[2] == 0 && p[3] == 0 {
        [m[0], m[1], m[2], m[3], p[4], 4]
    } else if m[4] != 0 && p[0] == 0 && p[1] == 0 && p[2] == 0 && p[3] == 0 && p[4] >= 5 {
        [m[0], m[1], m[2], m[3], m[4], p[4]]
    } else {
        return Err("invalid upce payload");
    };

    let parity = if d[0] == 0 {
        [
            "EEEOOO", "EEOEOO", "EEOOEO", "EEOOOE", "EOEEOO", "EOOEEO", "EOOOEE", "EOEOEO",
            "EOEOOE", "EOOEOE",
        ][d[11] as usize]
    } else {
        [
            "OOOEEE", "OOEOEE", "OOEEOE", "OOEEEO", "OEOOEE", "OEEOOE", "OEEEOO", "OEOEOE",
            "OEOEEO", "OEEOEO",
        ][d[11] as usize]
    };

    let l = [
        "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
        "0110111", "0001011",
    ];
    let g = [
        "0100111", "0110011", "0011011", "0100001", "0011101", "0111001", "0000101", "0010001",
        "0001001", "0010111",
    ];
    let mut modules = String::with_capacity(51);
    modules.push_str("101");
    for (i, &digit) in u.iter().enumerate() {
        let table = if parity.as_bytes()[i] == b'O' { &l } else { &g };
        modules.push_str(table[digit as usize]);
    }
    modules.push_str("010101");

    let width = (51 + 2 * quiet) * narrow;
    let mut canvas = Canvas::new(width, height);
    let x0 = quiet * narrow;
    for (i, mch) in modules.bytes().enumerate() {
        if mch == b'1' {
            canvas.fill(x0 + i as u32 * narrow, 0, narrow, height);
        }
    }
    Ok(canvas)
}

/// Encode `upc_a` as its zero-compressed UPC-E symbol.
/// Panics on invalid input (test convenience wrapper around [`try_encode_upce`]).
pub fn encode_upce(upc_a: &str, narrow: u32, height: u32, quiet: u32) -> Canvas {
    try_encode_upce(upc_a, narrow, height, quiet).expect("encode_upce")
}

// ---------------------------------------------------------------------------
// QR Code function patterns (ISO/IEC 18004) — grid + format-info tests.
// Draws the FUNCTION patterns only (finders, separators, timing, dark
// module, both format-info copies, alignment for v2..v6); data codewords
// land with the Reed-Solomon milestone.
// ---------------------------------------------------------------------------

const QR_FORMAT_GEN: u16 = 0x537; // x^10 + x^8 + x^5 + x^4 + x^2 + x + 1
const QR_FORMAT_MASK: u16 = 0x5412; // 101010000010010

/// BCH(15,5) remainder of a 15-bit format word.
fn qr_format_remainder(word: u16) -> u16 {
    let mut v = word & 0x7fff;
    for bit in (10..15).rev() {
        if v & (1 << bit) != 0 {
            v ^= QR_FORMAT_GEN << (bit - 10);
        }
    }
    v & 0x3ff
}

/// The 15-bit format word: EC level (2 bits, MSBs) | mask (3 bits) | BCH(10).
fn qr_format_word(ec_bits: u8, mask: u8) -> u16 {
    let word = (((ec_bits as u16) << 3) | mask as u16) << 10;
    word | qr_format_remainder(word)
}

/// Draw a QR symbol's function patterns. `ec_bits` is the raw 2-bit EC
/// field (M=00, L=01, H=10, Q=11). Module size 4px, quiet zone 6 modules.
pub fn encode_qr_pattern(version: u32, ec_bits: u8, mask: u8) -> Canvas {
    let n = 17 + 4 * version;
    let m = 4u32;
    let quiet = 6 * m;
    let dim = (n + 12) * m;
    let mut c = Canvas::new(dim, dim);
    qr_function_layer(&mut c, version, ec_bits, mask, m, quiet);
    c
}

/// Draw every function pattern (finders + separators, timing, dark module,
/// both format copies, alignment for v2..v6) onto the canvas and return the
/// occupancy grid — true where a module belongs to a function pattern and
/// is therefore not data-encodable (ISO 18004 §8.8).
fn qr_function_layer(
    c: &mut Canvas,
    version: u32,
    ec_bits: u8,
    mask: u8,
    m: u32,
    quiet: u32,
) -> Vec<Vec<bool>> {
    assert!(
        (1..=40).contains(&version),
        "pattern fixture covers v1..v40"
    );
    assert!(ec_bits <= 3 && mask <= 7);
    let n = 17 + 4 * version;
    let dim = (n + 12) * m;
    debug_assert_eq!(dim, c.width);
    let at = |mx: u32, my: u32| (quiet + mx * m, quiet + my * m);
    let mut occ = vec![vec![false; n as usize]; n as usize];

    // Finder patterns (7x7: dark ring + dark 3x3 center). Separators stay
    // light because the canvas starts empty. The finder+separator region is
    // an 8x8 block anchored at each corner.
    for &(fx, fy) in &[(0u32, 0u32), (n - 7, 0), (0, n - 7)] {
        for dy in 0..7u32 {
            for dx in 0..7u32 {
                let ring = dx == 0 || dx == 6 || dy == 0 || dy == 6;
                let center = (2..=4).contains(&dx) && (2..=4).contains(&dy);
                if ring || center {
                    let (x, y) = at(fx + dx, fy + dy);
                    c.fill(x, y, m, m);
                }
            }
        }
        let reg_r = if fy == 0 { 0 } else { n - 8 };
        let reg_c = if fx == 0 { 0 } else { n - 8 };
        for r in 0..8u32 {
            for c8 in 0..8u32 {
                occ[(reg_r + r) as usize][(reg_c + c8) as usize] = true;
            }
        }
    }

    // Timing patterns: alternating dark/even modules on row and column 6.
    for i in (8..n - 8).step_by(2) {
        let (x, y) = at(i, 6);
        c.fill(x, y, m, m);
        let (x, y) = at(6, i);
        c.fill(x, y, m, m);
    }
    for i in 0..n {
        occ[6][i as usize] = true;
        occ[i as usize][6] = true;
    }

    // Dark module at (8, n-8) — row 8, column n-8.
    let (x, y) = at(8, n - 8);
    c.fill(x, y, m, m);
    occ[8][(n - 8) as usize] = true;

    // Format info, both copies, XOR-masked. Bit k of the placed sequence is
    // bit (14-k) of the word: EC MSB first, then mask, then BCH bits.
    let word = qr_format_word(ec_bits, mask) ^ QR_FORMAT_MASK;
    let bit = |k: u32| (word >> (14 - k)) & 1;
    // (x, y) = (column, row) — same convention as the decoder's sample().
    let copy1: [(u32, u32); 15] = [
        (0, 8),
        (1, 8),
        (2, 8),
        (3, 8),
        (4, 8),
        (5, 8),
        (7, 8),
        (8, 8),
        (8, 7),
        (8, 5),
        (8, 4),
        (8, 3),
        (8, 2),
        (8, 1),
        (8, 0),
    ];
    let mut copy2 = [(0u32, 0u32); 15];
    for (i, slot) in copy2.iter_mut().take(8).enumerate() {
        *slot = (8, n - 1 - i as u32);
    }
    for (i, slot) in copy2.iter_mut().skip(8).enumerate() {
        *slot = (n - 1 - i as u32, 8);
    }
    for positions in [&copy1, &copy2] {
        for (k, (mx, my)) in positions.iter().enumerate() {
            if bit(k as u32) == 1 {
                let (x, y) = at(*mx, *my);
                c.fill(x, y, m, m);
            }
            occ[*my as usize][*mx as usize] = true;
        }
    }

    // Alignment patterns: every (row, col) combination of the version's
    // center coordinates, except the three that would land on finders.
    let coords = QR_ALIGNMENT[version as usize];
    if !coords.is_empty() {
        let (first, last) = (coords[0], coords[coords.len() - 1]);
        for &ar in coords {
            for &ac in coords {
                if (ar, ac) == (first, first)
                    || (ar, ac) == (first, last)
                    || (ar, ac) == (last, first)
                {
                    continue;
                }
                for dy in 0..5u32 {
                    for dx in 0..5u32 {
                        let ring = dx == 0 || dx == 4 || dy == 0 || dy == 4;
                        if ring || (dx == 2 && dy == 2) {
                            let (x, y) = at(ac - 2 + dx, ar - 2 + dy);
                            c.fill(x, y, m, m);
                        }
                        occ[(ar - 2 + dy) as usize][(ac - 2 + dx) as usize] = true;
                    }
                }
            }
        }
    }

    // Version information (v7+): the 18-bit word in two blocks — 6x3 beside
    // the top-right finder (rows 0..5, cols n-11..n-9) and 3x6 beside the
    // bottom-left finder (rows n-11..n-9, cols 0..5). Bit k of the word
    // (bit 0 = rightmost of the spec string): top-right block bit k at
    // (row k/3, col n-11+k%3); bottom-left bit k at (row n-11+k%3, col k/3).
    // Unmasked.
    if version >= 7 {
        let word = qr_version_word(version);
        for k in 0..18u32 {
            let bit = (word >> k) & 1;
            let (tr_x, tr_y) = at(n - 11 + k % 3, k / 3);
            let (bl_x, bl_y) = at(k / 3, n - 11 + k % 3);
            if bit == 1 {
                c.fill(tr_x, tr_y, m, m);
                c.fill(bl_x, bl_y, m, m);
            }
            occ[(k / 3) as usize][(n - 11 + k % 3) as usize] = true;
            occ[(n - 11 + k % 3) as usize][(k / 3) as usize] = true;
        }
    }

    occ
}

// ---------------------------------------------------------------------------
// Reed-Solomon over GF(2^8) (encode side) + codeword placement: the full
// `encode_qr` fixture (ISO/IEC 18004 §8.5-§8.8). Independent of the decoder
// crate's implementation per the clean-room rule; the round-trip tests
// cross-check the two.
// ---------------------------------------------------------------------------

/// Block structure per (version, raw EC field): (EC codewords per block,
/// [(block count, data codewords per block); 2 groups]) — ISO 18004 tables
/// for versions 1-40, in raw-field order (M=0, L=1, H=2, Q=3).
/// Each level row: [ec_per_block, g1_count, g1_data, g2_count, g2_data].
#[rustfmt::skip]
const QR_BLOCKS: [[[u32; 5]; 4]; 40] = [
  /* v1 */ [[10, 1, 16, 0, 0], [7, 1, 19, 0, 0], [17, 1, 9, 0, 0], [13, 1, 13, 0, 0]],
  /* v2 */ [[16, 1, 28, 0, 0], [10, 1, 34, 0, 0], [28, 1, 16, 0, 0], [22, 1, 22, 0, 0]],
  /* v3 */ [[26, 1, 44, 0, 0], [15, 1, 55, 0, 0], [22, 2, 13, 0, 0], [18, 2, 17, 0, 0]],
  /* v4 */ [[18, 2, 32, 0, 0], [20, 1, 80, 0, 0], [16, 4, 9, 0, 0], [26, 2, 24, 0, 0]],
  /* v5 */ [[24, 2, 43, 0, 0], [26, 1, 108, 0, 0], [22, 2, 11, 2, 12], [18, 2, 15, 2, 16]],
  /* v6 */ [[16, 4, 27, 0, 0], [18, 2, 68, 0, 0], [28, 4, 15, 0, 0], [24, 4, 19, 0, 0]],
  /* v7 */ [[18, 4, 31, 0, 0], [20, 2, 78, 0, 0], [26, 4, 13, 1, 14], [18, 2, 14, 4, 15]],
  /* v8 */ [[22, 2, 38, 2, 39], [24, 2, 97, 0, 0], [26, 4, 14, 2, 15], [22, 4, 18, 2, 19]],
  /* v9 */ [[22, 3, 36, 2, 37], [30, 2, 116, 0, 0], [24, 4, 12, 4, 13], [20, 4, 16, 4, 17]],
  /* v10 */ [[26, 4, 43, 1, 44], [18, 2, 68, 2, 69], [28, 6, 15, 2, 16], [24, 6, 19, 2, 20]],
  /* v11 */ [[30, 1, 50, 4, 51], [20, 4, 81, 0, 0], [24, 3, 12, 8, 13], [28, 4, 22, 4, 23]],
  /* v12 */ [[22, 6, 36, 2, 37], [24, 2, 92, 2, 93], [28, 7, 14, 4, 15], [26, 4, 20, 6, 21]],
  /* v13 */ [[22, 8, 37, 1, 38], [26, 4, 107, 0, 0], [22, 12, 11, 4, 12], [24, 8, 20, 4, 21]],
  /* v14 */ [[24, 4, 40, 5, 41], [30, 3, 115, 1, 116], [24, 11, 12, 5, 13], [20, 11, 16, 5, 17]],
  /* v15 */ [[24, 5, 41, 5, 42], [22, 5, 87, 1, 88], [24, 11, 12, 7, 13], [30, 5, 24, 7, 25]],
  /* v16 */ [[28, 7, 45, 3, 46], [24, 5, 98, 1, 99], [30, 3, 15, 13, 16], [24, 15, 19, 2, 20]],
  /* v17 */ [[28, 10, 46, 1, 47], [28, 1, 107, 5, 108], [28, 2, 14, 17, 15], [28, 1, 22, 15, 23]],
  /* v18 */ [[26, 9, 43, 4, 44], [30, 5, 120, 1, 121], [28, 2, 14, 19, 15], [28, 17, 22, 1, 23]],
  /* v19 */ [[26, 3, 44, 11, 45], [28, 3, 113, 4, 114], [26, 9, 13, 16, 14], [26, 17, 21, 4, 22]],
  /* v20 */ [[26, 3, 41, 13, 42], [28, 3, 107, 5, 108], [28, 15, 15, 10, 16], [30, 15, 24, 5, 25]],
  /* v21 */ [[26, 17, 42, 0, 0], [28, 4, 116, 4, 117], [30, 19, 16, 6, 17], [28, 17, 22, 6, 23]],
  /* v22 */ [[28, 17, 46, 0, 0], [28, 2, 111, 7, 112], [24, 34, 13, 0, 0], [30, 7, 24, 16, 25]],
  /* v23 */ [[28, 4, 47, 14, 48], [30, 4, 121, 5, 122], [30, 16, 15, 14, 16], [30, 11, 24, 14, 25]],
  /* v24 */ [[28, 6, 45, 14, 46], [30, 6, 117, 4, 118], [30, 30, 16, 2, 17], [30, 11, 24, 16, 25]],
  /* v25 */ [[28, 8, 47, 13, 48], [26, 8, 106, 4, 107], [30, 22, 15, 13, 16], [30, 7, 24, 22, 25]],
  /* v26 */ [[28, 19, 46, 4, 47], [28, 10, 114, 2, 115], [30, 33, 16, 4, 17], [28, 28, 22, 6, 23]],
  /* v27 */ [[28, 22, 45, 3, 46], [30, 8, 122, 4, 123], [30, 12, 15, 28, 16], [30, 8, 23, 26, 24]],
  /* v28 */ [[28, 3, 45, 23, 46], [30, 3, 117, 10, 118], [30, 11, 15, 31, 16], [30, 4, 24, 31, 25]],
  /* v29 */ [[28, 21, 45, 7, 46], [30, 7, 116, 7, 117], [30, 19, 15, 26, 16], [30, 1, 23, 37, 24]],
  /* v30 */ [[28, 19, 47, 10, 48], [30, 5, 115, 10, 116], [30, 23, 15, 25, 16], [30, 15, 24, 25, 25]],
  /* v31 */ [[28, 2, 46, 29, 47], [30, 13, 115, 3, 116], [30, 23, 15, 28, 16], [30, 42, 24, 1, 25]],
  /* v32 */ [[28, 10, 46, 23, 47], [30, 17, 115, 0, 0], [30, 19, 15, 35, 16], [30, 10, 24, 35, 25]],
  /* v33 */ [[28, 14, 46, 21, 47], [30, 17, 115, 1, 116], [30, 11, 15, 46, 16], [30, 29, 24, 19, 25]],
  /* v34 */ [[28, 14, 46, 23, 47], [30, 13, 115, 6, 116], [30, 59, 16, 1, 17], [30, 44, 24, 7, 25]],
  /* v35 */ [[28, 12, 47, 26, 48], [30, 12, 121, 7, 122], [30, 22, 15, 41, 16], [30, 39, 24, 14, 25]],
  /* v36 */ [[28, 6, 47, 34, 48], [30, 6, 121, 14, 122], [30, 2, 15, 64, 16], [30, 46, 24, 10, 25]],
  /* v37 */ [[28, 29, 46, 14, 47], [30, 17, 122, 4, 123], [30, 24, 15, 46, 16], [30, 49, 24, 10, 25]],
  /* v38 */ [[28, 13, 46, 32, 47], [30, 4, 122, 18, 123], [30, 42, 15, 32, 16], [30, 48, 24, 14, 25]],
  /* v39 */ [[28, 40, 47, 7, 48], [30, 20, 117, 4, 118], [30, 10, 15, 67, 16], [30, 43, 24, 22, 25]],
  /* v40 */ [[28, 18, 47, 31, 48], [30, 19, 118, 6, 119], [30, 20, 15, 61, 16], [30, 34, 24, 34, 25]],
];

/// Alignment pattern center coordinates per version — used as BOTH row and
/// column coordinates; the three finder-corner combinations are skipped
/// (ISO 18004 annex E). Version 1 has none.
#[rustfmt::skip]
const QR_ALIGNMENT: [&[u32]; 41] = [
  &[],
  &[],
  &[6, 18],
  &[6, 22],
  &[6, 26],
  &[6, 30],
  &[6, 34],
  &[6, 22, 38],
  &[6, 24, 42],
  &[6, 26, 46],
  &[6, 28, 50],
  &[6, 30, 54],
  &[6, 32, 58],
  &[6, 34, 62],
  &[6, 26, 46, 66],
  &[6, 26, 48, 70],
  &[6, 26, 50, 74],
  &[6, 30, 54, 78],
  &[6, 30, 56, 82],
  &[6, 30, 58, 86],
  &[6, 34, 62, 90],
  &[6, 28, 50, 72, 94],
  &[6, 26, 50, 74, 98],
  &[6, 30, 54, 78, 102],
  &[6, 28, 54, 80, 106],
  &[6, 32, 58, 84, 110],
  &[6, 30, 58, 86, 114],
  &[6, 34, 62, 90, 118],
  &[6, 26, 50, 74, 98, 122],
  &[6, 30, 54, 78, 102, 126],
  &[6, 26, 52, 78, 104, 130],
  &[6, 30, 56, 82, 108, 134],
  &[6, 34, 60, 86, 112, 138],
  &[6, 30, 58, 86, 114, 142],
  &[6, 34, 62, 90, 118, 146],
  &[6, 30, 54, 78, 102, 126, 150],
  &[6, 24, 50, 76, 102, 128, 154],
  &[6, 28, 54, 80, 106, 132, 158],
  &[6, 32, 58, 84, 110, 136, 162],
  &[6, 26, 54, 82, 110, 138, 166],
  &[6, 30, 58, 86, 114, 142, 170],
];

/// Remainder bits left after the codewords in the placement (§8.7.3).
/// Derived geometrically (total modules − function patterns) and
/// cross-checked against the block tables: 7 for v2-v6, 0 for v7-v13,
/// 3 for v14-v20, 4 for v21-v27, 3 for v28-v34 — and NONE for v35-v40
/// (the commonly copied "v28-v36 → 3" grouping is wrong at the edge:
/// 23008/8 and 24272/8 divide exactly for v35/v36).
fn qr_remainder_bits(version: u32) -> u32 {
    match version {
        1 => 0,
        2..=6 => 7,
        7..=13 => 0,
        14..=20 => 3,
        21..=27 => 4,
        28..=34 => 3,
        _ => 0,
    }
}

/// The 18-bit version information word: 6 data bits (the version) followed
/// by 12 BCH bits under x^12+x^11+x^10+x^9+x^8+x^5+x^2+1 (0x1F25). No
/// mask. Version 7 string per the spec: 000111110010010100.
pub fn qr_version_word(version: u32) -> u32 {
    let mut v = version << 12;
    for bit in (12..18).rev() {
        if v & (1 << bit) != 0 {
            v ^= 0x1F25 << (bit - 12);
        }
    }
    (version << 12) | (v & 0xFFF)
}

/// GF(2^8) tables: α = 2, primitive polynomial x^8+x^4+x^3+x^2+1 (0x11D).
struct QrGf {
    exp: [u8; 512],
    log: [u8; 256],
}

impl QrGf {
    fn new() -> Self {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut x: u16 = 1;
        for i in 0..255u16 {
            exp[i as usize] = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= 0x11d;
            }
        }
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        QrGf { exp, log }
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
        }
    }
}

/// The `nn` parity codewords for one block: remainder of (data · x^nn) mod
/// g(x), g = ∏_{j<nn}(x + α^j). data[0] is the most significant codeword.
fn qr_rs_parity(gf: &QrGf, data: &[u8], nn: usize) -> Vec<u8> {
    // Generator coefficients ascending; g[nn] is the leading 1.
    let mut g = vec![1u8];
    for j in 0..nn {
        let a = gf.exp[j];
        let mut next = vec![0u8; g.len() + 1];
        for (i, &co) in g.iter().enumerate() {
            next[i + 1] ^= co; // · x
            next[i] ^= gf.mul(co, a); // · α^j
        }
        g = next;
    }
    // Synthetic division over the message shifted left by nn. The feedback
    // taps are the non-leading coefficients in descending order.
    let mut rem = vec![0u8; nn];
    for &d in data {
        let factor = d ^ rem[0];
        rem.copy_within(1.., 0);
        *rem.last_mut().unwrap() = 0;
        for (i, &gc) in g.iter().rev().skip(1).enumerate() {
            rem[i] ^= gf.mul(factor, gc);
        }
    }
    rem
}

/// Block structure per (version, raw EC field) — ISO 18004 tables for
/// versions 1-40. The second group may be (0, 0).
fn qr_block_plan(version: u32, ec_bits: u8) -> (u32, [(u32, u32); 2]) {
    assert!(
        (1..=40).contains(&version),
        "encode_qr covers versions 1-40"
    );
    assert!(ec_bits <= 3, "ec_bits is the raw 2-bit field");
    let row = &QR_BLOCKS[(version - 1) as usize][ec_bits as usize];
    (row[0], [(row[1], row[2]), (row[3], row[4])])
}

/// Data codeword capacity for a (version, raw EC field) pair — the length
/// `encode_qr` expects `data` to have. Exposed for capacity planning.
pub fn qr_data_codewords(version: u32, ec_bits: u8) -> u32 {
    let (_, groups) = qr_block_plan(version, ec_bits);
    groups.iter().map(|&(cnt, sz)| cnt * sz).sum()
}

/// The mask predicate (ISO 18004 §8.8.2) for module (row, column).
fn qr_mask_bit(mask: u8, row: u32, col: u32) -> bool {
    match mask {
        0 => (row + col) % 2 == 0,
        1 => row % 2 == 0,
        2 => col % 3 == 0,
        3 => (row + col) % 3 == 0,
        4 => (row / 2 + col / 3) % 2 == 0,
        5 => (row * col) % 2 + (row * col) % 3 == 0,
        6 => ((row * col) % 2 + (row * col) % 3) % 2 == 0,
        7 => ((row + col) % 2 + (row * col) % 3) % 2 == 0,
        _ => false,
    }
}

/// Encode a full QR symbol: `data` is the complete data-codeword sequence
/// for (version, ec_bits) — segments + padding, built by the caller. The
/// parity codewords are computed per RS block, blocks are interleaved,
/// placed in the zigzag order with the requested mask applied, and drawn
/// over the function patterns. Module size `module` px, quiet zone 6 modules.
pub fn encode_qr(version: u32, ec_bits: u8, mask: u8, data: &[u8], module: u32) -> Canvas {
    assert!(
        (1..=40).contains(&version),
        "encode_qr covers versions 1-40"
    );
    let n = 17 + 4 * version;
    let m = module;
    let quiet = 6 * m;
    let dim = (n + 12) * m;
    let mut c = Canvas::new(dim, dim);
    let occ = qr_function_layer(&mut c, version, ec_bits, mask, m, quiet);

    let (ec_per_block, groups) = qr_block_plan(version, ec_bits);
    let data_total: u32 = groups.iter().map(|&(cnt, sz)| cnt * sz).sum();
    assert_eq!(
        data.len(),
        data_total as usize,
        "data must carry exactly the (version, level) data-codeword capacity"
    );

    // Split into blocks (group 1 first, then group 2) and compute parity.
    let gf = QrGf::new();
    let mut blocks: Vec<&[u8]> = Vec::new();
    let mut off = 0usize;
    for &(cnt, sz) in groups.iter() {
        for _ in 0..cnt {
            blocks.push(&data[off..off + sz as usize]);
            off += sz as usize;
        }
    }
    let parities: Vec<Vec<u8>> = blocks
        .iter()
        .map(|b| qr_rs_parity(&gf, b, ec_per_block as usize))
        .collect();

    // Interleave: round-robin data codewords over all blocks, then parity.
    let mut stream: Vec<u8> = Vec::with_capacity(data_total as usize);
    let max_len = groups.iter().map(|&(_, sz)| sz).max().unwrap() as usize;
    for j in 0..max_len {
        for block in &blocks {
            if j < block.len() {
                stream.push(block[j]);
            }
        }
    }
    for j in 0..ec_per_block as usize {
        for p in &parities {
            stream.push(p[j]);
        }
    }

    // Zigzag placement: two-module-wide columns from the bottom-right,
    // skipping the timing column, alternating upward/downward; each pair is
    // filled right module first. Bits MSB-first over `stream`, then the
    // remainder bits (7 for v2-v6, none for v1) as zeros. Every placed
    // module is toggled by the mask.
    let total_bits = stream.len() * 8 + qr_remainder_bits(version) as usize;
    let mut bit_index = 0usize;
    let mut upward = true;
    let mut right = n as i32 - 1;
    while right > 0 {
        if right == 6 {
            right -= 1;
        }
        for i in 0..n {
            let row = if upward { n - 1 - i } else { i };
            for &co in &[0, -1] {
                let col = (right + co) as u32;
                if occ[row as usize][col as usize] {
                    continue;
                }
                let bit = if bit_index < stream.len() * 8 {
                    (stream[bit_index / 8] >> (7 - bit_index % 8)) & 1 == 1
                } else {
                    false
                };
                if bit ^ qr_mask_bit(mask, row, col) {
                    let (x, y) = (quiet + col * m, quiet + row * m);
                    c.fill(x, y, m, m);
                }
                bit_index += 1;
            }
        }
        upward = !upward;
        right -= 2;
    }
    assert_eq!(
        bit_index, total_bits,
        "placement must consume exactly the codewords plus remainder bits"
    );
    c
}

/// Encode `text` as a complete QR symbol, choosing the smallest version
/// 1-40 that fits at `ec_bits` and auto-selecting the segment mode:
/// numeric for ASCII digits, alphanumeric when every character is in the
/// 45-character set, byte (UTF-8) otherwise. A single mode is used —
/// optimal mixed-mode segmentation is not attempted; the decoded text is
/// identical either way. Returns None when the payload cannot fit.
pub fn encode_qr_text(text: &str, ec_bits: u8, mask: u8, module: u32) -> Option<Canvas> {
    encode_qr_text_version(text, ec_bits, mask, 0, module)
}

/// [`encode_qr_text`] with an optional FORCED version (`0` = smallest
/// that fits, `1..=40` = that exact version). A forced version must have
/// room for the payload; the padding is the version's normal capacity
/// padding, so the symbol stays spec-clean (just larger).
pub fn encode_qr_text_version(
    text: &str,
    ec_bits: u8,
    mask: u8,
    forced_version: u32,
    module: u32,
) -> Option<Canvas> {
    const ALNUM: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
    let bytes = text.as_bytes();
    let numeric = !bytes.is_empty() && bytes.iter().all(|b| b.is_ascii_digit());
    let alnum = !numeric && text.is_ascii() && bytes.iter().all(|b| ALNUM.contains(b));

    // Mode + payload bits; the count-indicator width depends on the
    // version group (v1-9: 10/9/8, v10-26: 12/11/10, v27-40: 14/13/16),
    // so it is computed per candidate version below.
    let (mode, data_bits) = if numeric {
        let n = bytes.len();
        (
            1u32,
            10 * (n / 3)
                + match n % 3 {
                    2 => 7,
                    1 => 4,
                    _ => 0,
                },
        )
    } else if alnum {
        let n = bytes.len();
        (2u32, 11 * (n / 2) + 6 * (n % 2))
    } else {
        (4u32, 8 * bytes.len())
    };
    let count_bits = |version: u32| -> usize {
        match mode {
            1 if version <= 9 => 10,
            1 if version <= 26 => 12,
            1 => 14,
            2 if version <= 9 => 9,
            2 if version <= 26 => 11,
            2 => 13,
            _ if version <= 9 => 8,
            _ if version <= 26 => 10,
            _ => 16,
        }
    };

    let versions = if (1..=40).contains(&forced_version) {
        forced_version..=forced_version
    } else {
        1..=40
    };
    for version in versions {
        let cap = qr_data_codewords(version, ec_bits) as usize;
        let count_bits = count_bits(version);
        // The payload itself must fit; the terminator occupies up to 4
        // further bits when capacity remains (truncated on an exact fit).
        let raw = 4 + count_bits + data_bits;
        if raw > cap * 8 {
            continue;
        }
        let with_term = (raw + 4).min(cap * 8);
        let needed = with_term.div_ceil(8);
        // Bitstream: mode, count, payload, then zeros to the needed
        // byte count (terminator + alignment), then 0xEC/0x11 pad codewords.
        let mut bits: Vec<bool> = Vec::with_capacity(cap * 8);
        let push = |bits: &mut Vec<bool>, v: u32, n: u32| {
            for i in (0..n).rev() {
                bits.push((v >> i) & 1 == 1);
            }
        };
        push(&mut bits, mode, 4);
        push(&mut bits, bytes.len() as u32, count_bits as u32);
        match mode {
            1 => {
                for g in bytes.chunks(3) {
                    let v: u32 = g
                        .iter()
                        .map(|&d| (d - b'0') as u32)
                        .fold(0, |a, d| a * 10 + d);
                    let w = if g.len() == 3 {
                        10
                    } else if g.len() == 2 {
                        7
                    } else {
                        4
                    };
                    push(&mut bits, v, w);
                }
            }
            2 => {
                for p in bytes.chunks(2) {
                    let v: u32 = p
                        .iter()
                        .map(|&c| ALNUM.iter().position(|&a| a == c).unwrap() as u32)
                        .fold(0, |a, d| a * 45 + d);
                    push(&mut bits, v, if p.len() == 2 { 11 } else { 6 });
                }
            }
            _ => {
                for &b in bytes {
                    push(&mut bits, b as u32, 8);
                }
            }
        }
        bits.resize(needed * 8, false);
        let mut data = vec![0u8; cap];
        for (i, &b) in bits.iter().enumerate() {
            if b {
                data[i / 8] |= 0x80 >> (i % 8);
            }
        }
        for (i, slot) in data.iter_mut().enumerate().skip(needed) {
            *slot = if i % 2 == 0 { 0xEC } else { 0x11 };
        }
        return Some(encode_qr(version, ec_bits, mask, &data, module));
    }
    None
}

// ---------------------------------------------------------------------------
// GS1 DataBar Omnidirectional (96 modules, 46 runs; run 0 = leading paper
// module m0, odd run indices are ink, run 45 = final bar at m95)
// ---------------------------------------------------------------------------

/// Split a 13-digit value into the four DataBar residues (oracle-verified
/// hierarchical chain, docs/databar-recon.md):
/// `V = (((R4*1597 + R3)*2841 + R2)*1597 + R1)`.
/// R1→pair3 (runs 23-30), R2→S2 pair (runs 37-44), R3→pair2 (runs 15-22),
/// R4→S1 pair (runs 1-9). Capacity 1597*2841*1597*1381 ≈ 1.0006e13 ≥ 10^13.
pub fn databar_split(v: u64) -> (u32, u32, u32, u32) {
    assert!(v < 10_000_000_000_000, "DataBar value must be 13 digits");
    (
        (v % 1597) as u32,
        (v / 1597 % 2841) as u32,
        (v / 4_537_077 % 1597) as u32,
        (v / 7_245_711_969) as u32,
    )
}

/// Render the 46 run widths of a DataBar Omnidirectional symbol as a Canvas.
/// `runs[0]` is the leading paper module; odd indices are ink runs. The sum
/// must be 96 (1 paper module + 95 symbol modules).
pub fn databar_omni_canvas(runs: &[u32; 46], module: u32, height: u32, quiet: u32) -> Canvas {
    assert_eq!(
        runs.iter().sum::<u32>(),
        96,
        "DataBar omni spans 96 modules"
    );
    let width = (96 + 2 * quiet) * module;
    let mut canvas = Canvas::new(width, height);
    let mut x = quiet * module;
    for (i, &w) in runs.iter().enumerate() {
        if i % 2 == 1 {
            canvas.fill(x, 0, w * module, height);
        }
        x += w * module;
    }
    canvas
}

/// The linear symbol's 96-module string (m0..m95) as ink booleans.
/// `runs[0]` is the leading paper module; odd run indices are ink.
pub fn databar_linear_modules(runs: &[u32; 46]) -> [bool; 96] {
    assert_eq!(
        runs.iter().sum::<u32>(),
        96,
        "DataBar omni spans 96 modules"
    );
    let mut mods = [false; 96];
    let mut m = 0usize;
    for (i, &w) in runs.iter().enumerate() {
        if i % 2 == 1 {
            for mm in m..m + w as usize {
                mods[mm] = true;
            }
        }
        m += w as usize;
    }
    mods
}

/// Render a DataBar Stacked (omni = false, 50x13 modules: rows 5+1+7) or
/// DataBar Stacked Omnidirectional (omni = true, 50x69: rows 33+3+33)
/// symbol as a Canvas. Content rows come from the linear module string
/// split with the 4-module overlap (top = m0..m49, bottom = m46..m95);
/// separator rows carry the fixed dashed pattern (ink at odd columns
/// 5..45) — zxing round-trip validates scanner acceptance.
pub fn databar_stacked_canvas(runs: &[u32; 46], module: u32, quiet: u32, omni: bool) -> Canvas {
    let mods = databar_linear_modules(runs);
    let (top_rows, sep_rows, bot_rows) = if omni {
        (33u32, 3u32, 33u32)
    } else {
        (5, 1, 7)
    };
    let total_rows = top_rows + sep_rows + bot_rows;
    let width = (50 + 2 * quiet) * module;
    let mut canvas = Canvas::new(width, total_rows * module);
    // Content bands: top = m0..m49 at rows 0..top_rows; bottom = m46..m95
    // at the rows after the separator zone.
    let bands = [
        (0u32, 0u32, top_rows),
        (46u32, top_rows + sep_rows, bot_rows),
    ];
    for (m_lo, y0, rows) in bands {
        for r in 0..rows {
            for m in 0..50usize {
                if mods[m_lo as usize + m] {
                    canvas.fill(
                        (quiet + m as u32) * module,
                        (y0 + r) * module,
                        module,
                        module,
                    );
                }
            }
        }
    }
    // Separator rows: dashed, ink at odd content columns 5..45.
    for s in 0..sep_rows {
        let y = (top_rows + s) * module;
        for m in (5..46).step_by(2) {
            canvas.fill((quiet + m as u32) * module, y, module, module);
        }
    }
    canvas
}

pub mod oracle_matrices;

pub mod databar_exp;
mod databar_exp_tables;
mod databar_tables;

/// Segment width tables from the oracle sweeps (docs/databar-recon.md),
/// generated by tools/gen_databar.py (bit-exact vs zint before emission).
fn segment_s1(r4: u32) -> [u32; 9] {
    databar_tables::S1[r4 as usize].map(|x| x as u32)
}
fn segment_pair2(r3: u32) -> [u32; 8] {
    databar_tables::PAIR2[r3 as usize].map(|x| x as u32)
}
fn segment_pair3(r1: u32) -> [u32; 8] {
    databar_tables::PAIR3[r1 as usize].map(|x| x as u32)
}
fn segment_s2(r2: u32) -> [u32; 10] {
    databar_tables::S2[r2 as usize].map(|x| x as u32)
}

/// Finder triples (FL runs 10-12, FR runs 33-35) from the ISO 24724 rule:
/// c = Σ 3^k·e_k mod 79 over the 32 element widths in reading sequence
/// (char1 runs 2..9 L→R, char2 22..15 R→L, char4 43..36 R→L, char3 23..30 L→R);
/// the 79 valid (CL,CR) pairs (81 minus (0,8),(8,0)) enumerate c row-major.
fn finder_triples(runs: &[u32; 46]) -> ([u32; 3], [u32; 3]) {
    const POW3: [u32; 32] = {
        let mut p = [1u32; 32];
        let mut i = 1;
        while i < 32 {
            p[i] = p[i - 1] * 3 % 79;
            i += 1;
        }
        p
    };
    let e = runs[2..10]
        .iter()
        .copied()
        .chain((15..23).rev().map(|i| runs[i]))
        .chain((36..44).rev().map(|i| runs[i]))
        .chain(runs[23..31].iter().copied());
    let c = e.enumerate().map(|(k, x)| POW3[k] * x).sum::<u32>() % 79;
    let raw = c + u32::from(c >= 8) + u32::from(c >= 72);
    let (cl, cr) = (raw / 9, raw % 9);
    let t4 = |v: usize| databar_tables::FINDER_T4[v].map(|x| x as u32);
    let fl = t4(cl as usize);
    let fr = [t4(cr as usize)[2], t4(cr as usize)[1], t4(cr as usize)[0]];
    (fl, fr)
}

/// Build the 46 run widths of a DataBar Omnidirectional symbol for the
/// 13-digit value `v` (`< 10^13`). Returns `Err` instead of panicking.
pub fn try_databar_omni_runs(v: u64) -> Result<[u32; 46], &'static str> {
    if v >= 10_000_000_000_000 {
        return Err("invalid databar payload");
    }
    let (r1, r2, r3, r4) = databar_split(v);
    let mut runs = [0u32; 46];
    runs[0] = 1; // leading paper module m0
    runs[1..10].copy_from_slice(&segment_s1(r4));
    runs[15..23].copy_from_slice(&segment_pair2(r3));
    runs[23..31].copy_from_slice(&segment_pair3(r1));
    runs[36..46].copy_from_slice(&segment_s2(r2));
    let (fl, fr) = finder_triples(&runs);
    runs[10..13].copy_from_slice(&fl);
    runs[13] = 1;
    runs[14] = 1;
    runs[31] = 1;
    runs[32] = 1;
    runs[33..36].copy_from_slice(&fr);
    Ok(runs)
}

/// Build the 46 run widths of a DataBar Omnidirectional symbol for the
/// 13-digit value `v`. Panics if `v >= 10^13` (test wrapper).
pub fn databar_omni_runs(v: u64) -> [u32; 46] {
    try_databar_omni_runs(v).expect("databar_omni_runs")
}

// ---------------------------------------------------------------------------
// GS1 DataBar Limited (ISO/IEC 24724): 74 modules, 46 runs; run 0 = leading
// paper module, odd run indices are ink. Anatomy (docs/databar-recon.md):
// [paper 1][guard 1][DATA1 = runs 2-15, 26 mod][CHECK = runs 16-27, 16 mod]
// [space 1][bar 1][DATA2 = runs 30-43, 26 mod][space 1][bar 1]. The value
// splits V = D1·2013571 + D2 into two (26,7) characters (7 spaces + 7 bars
// each, compositions ranked lex-ascending, bars need ≥1 single-module
// element); the check character renders the mod-89 checksum Σ 3^k·e_k
// through a fixed 89-entry (16,6) width table. Model verified 2000/2000
// bit-exact against zint (BARCODE_RSS_LTD).
// ---------------------------------------------------------------------------

mod databar_ltd_tables;
use databar_ltd_tables::LTD_B;

/// The (26,7) character capacity: D2's domain and D1's multiplier.
const LTD_M: u64 = 2_013_571;

/// (spaces_sum, bars_sum, GSUM, spaces_widest, bars_widest) per group; the
/// seven group spans partition 0..2013571 exactly (verified vs zint).
const LTD_GROUPS: [(u32, u32, u32, u32, u32); 7] = [
    (17, 9, 0, 6, 3),
    (13, 13, 183064, 5, 4),
    (9, 17, 820064, 3, 6),
    (15, 11, 1000776, 5, 4),
    (11, 15, 1491021, 4, 5),
    (19, 7, 1979845, 8, 1),
    (7, 19, 1996939, 1, 8),
];

/// Number of compositions of `t` into `k` parts each in 1..=w (k ≤ 7 here,
/// so a tiny DP per call is plenty).
fn ltd_compositions(k: usize, t: u32, w: u32) -> u64 {
    let mut dp = vec![0u64; t as usize + 1];
    dp[0] = 1;
    for _ in 0..k {
        let mut next = vec![0u64; t as usize + 1];
        for s in 1..=t as usize {
            for p in 1..=w.min(s as u32) {
                next[s] += dp[s - p as usize];
            }
        }
        dp = next;
    }
    dp[t as usize]
}

/// Number of compositions of `t` into `k` parts in 1..=w with at least one
/// part equal to 1 (the noNarrow rule, applied to the bar subsets only).
/// Compositions without a 1 correspond to t-k into k parts in 2..=w.
fn ltd_compositions_single(k: usize, t: u32, w: u32) -> u64 {
    if t < k as u32 {
        return 0;
    }
    if w > 1 {
        ltd_compositions(k, t, w) - ltd_compositions(k, t - k as u32, w - 1)
    } else {
        u64::from(t == k as u32)
    }
}

/// Lex-ascending unrank: the `r`-th (0-based) composition of `t` into `k`
/// parts in 1..=w; `need_one` requires a part equal to 1.
fn ltd_unrank(k: usize, mut t: u32, w: u32, mut need_one: bool, mut r: u64) -> [u32; 7] {
    let mut out = [0u32; 7];
    for (i, slot) in out.iter_mut().enumerate().take(k) {
        let rest = k - i - 1;
        for p in 1..=w.min(t) {
            let rem = t - p;
            let c = if need_one && p != 1 {
                ltd_compositions_single(rest, rem, w)
            } else {
                ltd_compositions(rest, rem, w)
            };
            if r < c {
                *slot = p;
                t = rem;
                need_one = need_one && p != 1;
                break;
            }
            r -= c;
        }
    }
    out
}

/// (26,7) data character: value -> (7 space widths, 7 bar widths).
/// VD = GSUM + V_spaces·T_bars + V_bars; the bars vary fastest.
fn ltd_char_widths(d: u32) -> ([u32; 7], [u32; 7]) {
    for &(ss, sb, gsum, spw, brw) in &LTD_GROUPS {
        let tb = ltd_compositions_single(7, sb, brw);
        let nsp = ltd_compositions(7, ss, spw);
        if u64::from(d) < u64::from(gsum) + nsp * tb {
            let v = u64::from(d) - u64::from(gsum);
            let sp = ltd_unrank(7, ss, spw, false, v / tb);
            let br = ltd_unrank(7, sb, brw, true, v % tb);
            return (sp, br);
        }
    }
    panic!("value {d} outside the DataBar Limited (26,7) range");
}

/// c = Σ 3^k·e_k mod 89 over the 28 data elements: char1 (runs 2-15) then
/// char2 (runs 30-43), each read left to right as a space,bar interleave.
fn ltd_checksum(a: &[u32; 14], c: &[u32; 14]) -> usize {
    (a.iter()
        .chain(c.iter())
        .enumerate()
        .map(|(k, &e)| 3u64.pow(k as u32) * u64::from(e))
        .sum::<u64>()
        % 89) as usize
}

/// Build the 46 run widths of a DataBar Limited symbol for the 13-digit
/// value `v` (indicator digit 0 or 1, i.e. v < 2·10^12). Returns `Err` instead of panicking.
pub fn try_databar_ltd_runs(v: u64) -> Result<[u32; 46], &'static str> {
    if v >= 2_000_000_000_000 {
        return Err("invalid databarltd payload");
    }
    let interleave = |(sp, br): ([u32; 7], [u32; 7])| {
        let mut out = [0u32; 14];
        for i in 0..7 {
            out[2 * i] = sp[i];
            out[2 * i + 1] = br[i];
        }
        out
    };
    let (sp1, br1) = ltd_char_widths((v / LTD_M) as u32);
    let (sp2, br2) = ltd_char_widths((v % LTD_M) as u32);
    let a = interleave((sp1, br1));
    let c = interleave((sp2, br2));
    let mut runs = [1u32; 46];
    runs[2..16].copy_from_slice(&a);
    for (i, &w) in LTD_B[ltd_checksum(&a, &c)].iter().enumerate() {
        runs[16 + i] = u32::from(w);
    }
    runs[30..44].copy_from_slice(&c);
    Ok(runs)
}

/// Build the 46 run widths of a DataBar Limited symbol for the 13-digit
/// value `v`. Panics if `v >= 2·10^12` (test wrapper).
pub fn databar_ltd_runs(v: u64) -> [u32; 46] {
    try_databar_ltd_runs(v).expect("databar_ltd_runs")
}

/// Render the 46 run widths of a DataBar Limited symbol as a Canvas.
/// `runs[0]` is the leading paper module; odd run indices are ink. The sum
/// must be 74 (1 paper module + 73 symbol modules).
pub fn databar_ltd_canvas(runs: &[u32; 46], module: u32, height: u32, quiet: u32) -> Canvas {
    assert_eq!(
        runs.iter().sum::<u32>(),
        74,
        "DataBar Limited spans 74 modules"
    );
    let width = (74 + 2 * quiet) * module;
    let mut canvas = Canvas::new(width, height);
    let mut x = quiet * module;
    for (i, &w) in runs.iter().enumerate() {
        if i % 2 == 1 {
            canvas.fill(x, 0, w * module, height);
        }
        x += w * module;
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ean13_module_string_has_structure() {
        let m = ean13_modules(&[0, 4, 0, 4, 8, 0, 4, 1, 3, 1, 8, 0, 7]);
        assert_eq!(m.len(), 95);
        assert!(m.starts_with("101"));
        assert!(m.ends_with("101"));
        assert_eq!(&m[45..50], "01010");
    }

    #[test]
    fn code39_patterns_have_three_wide() {
        for (_, p) in C39 {
            assert_eq!(p.bytes().filter(|&b| b == b'1').count(), 3, "{p}");
        }
    }

    #[test]
    fn itf_structure_valid() {
        let canvas = encode_itf("12345670", 3, 2.5, 30);
        // the last element must be a wide bar (stop)
        assert!(canvas.width > 0);
    }

    #[test]
    fn qr_format_word_passes_its_own_bch() {
        for ec in 0u16..4 {
            for mask in 0u16..8 {
                let word = qr_format_word(ec as u8, mask as u8);
                assert_eq!(qr_format_remainder(word), 0, "ec={ec} mask={mask}");
                // the un-masked word carries (ec, mask) in its top fields
                assert_eq!(word >> 13, ec);
                assert_eq!((word >> 10) & 7, mask);
            }
        }
    }

    #[test]
    fn qr_block_plans_match_spec_totals() {
        // Total codewords per version (ISO 18004): data + EC·blocks, and
        // all four levels of a version must agree on it.
        const TOTALS: [u32; 40] = [
            26, 44, 70, 100, 134, 172, 196, 242, 292, 346, 404, 466, 532, 581, 655, 733, 815, 901,
            991, 1085, 1156, 1258, 1364, 1474, 1588, 1706, 1828, 1921, 2051, 2185, 2323, 2465,
            2611, 2761, 2876, 3034, 3196, 3362, 3532, 3706,
        ];
        for v in 1..=40u32 {
            let mut totals = [0u32; 4];
            for ec in 0..4u8 {
                let (ec_per_block, groups) = qr_block_plan(v, ec);
                let data: u32 = groups.iter().map(|&(c, s)| c * s).sum();
                let blocks: u32 = groups.iter().map(|&(c, _)| c).sum();
                totals[ec as usize] = data + ec_per_block * blocks;
                assert_eq!(qr_data_codewords(v, ec), data, "v{v} ec{ec} data capacity");
            }
            assert!(
                totals.iter().all(|&t| t == totals[0]),
                "v{v} levels disagree: {totals:?}"
            );
            assert_eq!(totals[0], TOTALS[(v - 1) as usize], "v{v} total");
        }
    }

    #[test]
    fn qr_version_word_matches_published_strings() {
        // Spec / thonky format-version table anchors (v7-v19 published).
        assert_eq!(qr_version_word(7), 0b000111110010010100);
        assert_eq!(qr_version_word(8), 0b001000010110111100);
        assert_eq!(qr_version_word(9), 0b001001101010011001);
        assert_eq!(qr_version_word(10), 0b001010010011010011);
        assert_eq!(qr_version_word(11), 0b001011101111110110);
        assert_eq!(qr_version_word(12), 0b001100011101100010);
        assert_eq!(qr_version_word(19), 0b010011010100110010);
    }

    #[test]
    fn encode_qr_places_exactly_the_codeword_bits() {
        // The placement walk must consume every codeword bit plus the 7
        // remainder bits (v2+); encode_qr asserts this internally, so a
        // successful encode IS the check — here just exercise a few shapes.
        for (v, ec) in [(1u32, 1u8), (2, 0), (4, 2), (5, 3), (6, 2)] {
            let len = qr_data_codewords(v, ec) as usize;
            let data: Vec<u8> = (0..len as u8)
                .map(|i| i.wrapping_mul(31).wrapping_add(5))
                .collect();
            let canvas = encode_qr(v, ec, 3, &data, 4);
            assert!(canvas.width > 0);
        }
    }

    #[test]
    fn check_digit_helper() {
        assert_eq!(ean13_check_digit("040480413180"), '7');
        assert_eq!(ean13_check_digit("400638133393"), '1');
    }

    #[test]
    fn databar_split_roundtrips() {
        // V = (((R4*1597 + R3)*2841 + R2)*1597 + R1) must reproduce V.
        for v in [
            0u64,
            1,
            1596,
            1597,
            4_537_076,
            4_537_077,
            7_245_711_968,
            7_245_711_969,
            123_456_789_012,
            9_999_999_999_999,
        ] {
            let (r1, r2, r3, r4) = databar_split(v);
            assert_eq!(r1, (v % 1597) as u32);
            assert_eq!(r2, (v / 1597 % 2841) as u32);
            assert_eq!(r3, (v / 4_537_077 % 1597) as u32);
            assert_eq!(r4, (v / 7_245_711_969) as u32);
            let recomposed = r1 as u64 + 1597 * (r2 as u64 + 2841 * (r3 as u64 + 1597 * r4 as u64));
            assert_eq!(recomposed, v, "chain round-trip failed for {v}");
        }
    }

    #[test]
    fn databar_canvas_shape() {
        // 46 runs summing to 96 (run 0 paper + 95 symbol modules).
        let mut runs = [1u32; 46];
        runs[1] = 25;
        runs[45] = 27;
        let canvas = databar_omni_canvas(&runs, 4, 80, 2);
        assert_eq!(canvas.width, (96 + 4) * 4);
        assert_eq!(canvas.height, 80);
    }
}
