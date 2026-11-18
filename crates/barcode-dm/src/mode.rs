//! Mode decoding: ECC 200 data-codeword stream -> payload bytes.
//!
//! Clean-room: every rule here was derived empirically from zint output
//! and cross-checked against libdmtx (see `docs/STATUS.md` "MODE
//! ENCODATION" and `tests/mode.rs` for the frozen oracle vectors).

/// Latch codewords.
const C40: u8 = 230;
const BASE256: u8 = 231;
const UPPER_SHIFT: u8 = 235;
const X12: u8 = 238;
const TEXT: u8 = 239;
const EDIFACT: u8 = 240;
const UNLATCH: u8 = 254;

/// 255-state randomizing, decoder side: the stored codeword is
/// `(v + ((149 * p) % 255) + 1) mod 256` with `p` the 1-based absolute
/// position of the codeword in the symbol's data-codeword stream (the
/// 231 latch and any preceding segments count; ECC does not).
fn unrandomize(cw: u8, position: usize) -> u8 {
    let x = (149 * position as u32 % 255) as i32;
    (cw as i32 - x - 1).rem_euclid(256) as u8
}

/// Shift-2 set: indices 0..26 are ASCII; 27 = FNC1 (decoded as GS);
/// 28, 29 unassigned; 30 = upper shift; 31 unassigned.
const SHIFT2: [u8; 27] = [
    33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, //
    58, 59, 60, 61, 62, 63, 64, //
    91, 92, 93, 94, 95,
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    C40,
    Text,
}

/// Decode a corrected data-codeword stream into the payload bytes.
///
/// Returns `None` on streams that use reserved codewords or are
/// structurally inconsistent (dangling half-pair, Base256 length
/// overrun, ...).
pub fn decode_payload(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let cw = data[i];
        i += 1;
        match cw {
            0 => return None,
            1..=128 => out.push(cw - 1),
            129 => break, // end of data; the rest is padding
            130..=229 => {
                // Two digits packed as 130 + 10*d1 + d2.
                let v = cw - 130;
                out.push(b'0' + v / 10);
                out.push(b'0' + v % 10);
            }
            C40 => i = decode_40(data, i, &mut out, Kind::C40)?,
            BASE256 => i = decode_base256(data, i, &mut out)?,
            UPPER_SHIFT => {
                // (235, v - 127) encodes ASCII 128..=255.
                let x = *data.get(i)?;
                if !(1..=128).contains(&x) {
                    return None;
                }
                i += 1;
                out.push(x + 127);
            }
            X12 => i = decode_x12(data, i, &mut out)?,
            TEXT => i = decode_40(data, i, &mut out, Kind::Text)?,
            EDIFACT => i = decode_edifact(data, i, &mut out)?,
            _ => return None, // 232-234, 236 f, 237, 241-253, 254, 255
        }
    }
    Some(out)
}

/// C40 / Text sections: pairs of codewords -> three base-40 values,
/// with shift 1/2/3 and the upper-shift flag. Ends at 254, at symbol
/// end, or (values trailing) on padding the decoder never reaches.
fn decode_40(data: &[u8], mut i: usize, out: &mut Vec<u8>, kind: Kind) -> Option<usize> {
    let mut shift = 0u8;
    let mut upper = false;
    while i < data.len() {
        if data[i] == UNLATCH {
            return Some(i + 1);
        }
        if i + 1 == data.len() {
            return None; // dangling half pair
        }
        let v = u16::from_be_bytes([data[i], data[i + 1]]).wrapping_sub(1);
        i += 2;
        for val in [v / 1600, v % 1600 / 40, v % 40] {
            let ch: u16 = match shift {
                0 => match val {
                    0..=2 => {
                        shift = (val + 1) as u8;
                        continue;
                    }
                    3 => 32, // ' '
                    4..=13 => 48 + val - 4,
                    14..=39 => match kind {
                        Kind::C40 => 65 + val - 14,
                        Kind::Text => 97 + val - 14,
                    },
                    _ => return None,
                },
                // Shift 1: next value is the raw ASCII code.
                1 => {
                    shift = 0;
                    val
                }
                // Shift 2: table (or FNC1 / upper shift).
                2 => {
                    shift = 0;
                    match val {
                        30 => {
                            upper = true;
                            continue;
                        }
                        27 => 29, // FNC1 -> GS
                        0..=26 => SHIFT2[val as usize] as u16,
                        _ => return None,
                    }
                }
                // Shift 3: C40 affine 96+v; Text is a table.
                _ => {
                    shift = 0;
                    match kind {
                        Kind::C40 => match val {
                            0..=31 => 96 + val,
                            _ => return None,
                        },
                        Kind::Text => match val {
                            0 => 96, // '`'
                            1..=26 => 64 + val,
                            27..=31 => 96 + val,
                            _ => return None,
                        },
                    }
                }
            };
            let mut ch = ch as u8;
            if upper {
                // Upper shift applies to the next produced character,
                // however it was encoded (shift-1 raw or a basic value).
                ch += 128;
                upper = false;
            }
            out.push(ch);
        }
    }
    Some(i)
}

/// X12 sections: same pair packing, no shifts. Values: 0=CR, 1='*',
/// 2='>', 3=' ', 4-13 digits, 14-39 'A'-'Z'.
fn decode_x12(data: &[u8], mut i: usize, out: &mut Vec<u8>) -> Option<usize> {
    while i < data.len() {
        if data[i] == UNLATCH {
            return Some(i + 1);
        }
        if i + 1 == data.len() {
            return None;
        }
        let v = u16::from_be_bytes([data[i], data[i + 1]]).wrapping_sub(1);
        i += 2;
        for val in [v / 1600, v % 1600 / 40, v % 40] {
            let ch = match val {
                0 => 13u8,
                1 => 42,
                2 => 62,
                3 => 32,
                4..=13 => 48 + (val - 4) as u8,
                14..=39 => 65 + (val - 14) as u8,
                _ => return None,
            };
            out.push(ch);
        }
    }
    Some(i)
}

/// EDIFACT sections: three codewords -> four 6-bit values (MSB first),
/// char = value + 64 for values < 32. Value 31 unlatches; a trailing
/// group with fewer than 3 codewords left falls back to ASCII.
fn decode_edifact(data: &[u8], mut i: usize, out: &mut Vec<u8>) -> Option<usize> {
    while i + 3 <= data.len() {
        let bits = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        i += 3;
        for k in 0..4 {
            let val = (bits >> (18 - 6 * k)) & 63;
            if val == 31 {
                return Some(i);
            }
            out.push(if val >= 32 { val as u8 } else { val as u8 + 64 });
        }
    }
    Some(i)
}

/// Base256 sections: `231, randomized length, randomized bytes..`. The
/// length indicator stores the raw byte count.
fn decode_base256(data: &[u8], i: usize, out: &mut Vec<u8>) -> Option<usize> {
    let n = unrandomize(*data.get(i)?, i + 1) as usize;
    if i + 1 + n > data.len() {
        return None;
    }
    for j in 0..n {
        out.push(unrandomize(data[i + 1 + j], i + 2 + j));
    }
    Some(i + 1 + n)
}
