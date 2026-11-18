//! Text layer: the mode state machine over the de-stuffed data bit string
//! (ISO/IEC 24778 character set; see docs/STATUS.md aztec text blocks).
//!
//! The RS-corrected data codewords are the b-bit windows of the WRITTEN
//! (stuff-modulated) stream, so the message is recovered by concatenating
//! them and removing stuff bits: a non-final window whose first b−1 bits
//! are all equal carries the complementary stuff bit as its b-th bit; the
//! final window is raw. Codes are then read
//! sequentially: 5-bit in Upper/Lower/Mixed/Punct, 4-bit in Digit mode;
//! shifts apply to one code (in the target mode's width), latches persist,
//! B/S carries a 5-bit length (0 → 11 more bits, count−31) followed by
//! raw bytes, and Punct 0 is FLG(n). Trailing pad (all ones) only ever
//! yields shifts or a truncated/oversized B/S, all of which terminate the
//! scan without emitting content.

use crate::tables::bits_for_layers;
use crate::Stream;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Upper,
    Lower,
    Mixed,
    Punct,
    Digit,
}

enum Tok {
    /// One byte to append.
    Byte(u8),
    /// Bytes to append.
    Chars(&'static [u8]),
    /// P/S: punct shift for the next code.
    PSh,
    /// U/S: upper shift for the next code.
    USh,
    Latch(Mode),
    /// B/S: binary shift.
    Bin,
    /// FLG(n) (punct 0).
    Flg,
}

static MIXED_TAIL: [u8; 8] = [b'@', b'\\', b'^', b'_', b'`', b'|', b'~', 0x7f];
static PUNCT: [u8; 25] = *b"!\"#$%&'()*+,-./:;<=>?[]{}";

fn token(mode: Mode, code: u16) -> Tok {
    match mode {
        Mode::Upper => match code {
            0 => Tok::PSh,
            1 => Tok::Chars(b" "),
            2..=27 => Tok::Byte((code as u8 - 2) + b'A'),
            28 => Tok::Latch(Mode::Lower),
            29 => Tok::Latch(Mode::Mixed),
            30 => Tok::Latch(Mode::Digit),
            _ => Tok::Bin,
        },
        Mode::Lower => match code {
            0 => Tok::PSh,
            1 => Tok::Chars(b" "),
            2..=27 => Tok::Byte((code as u8 - 2) + b'a'),
            28 => Tok::USh,
            29 => Tok::Latch(Mode::Mixed),
            30 => Tok::Latch(Mode::Digit),
            _ => Tok::Bin,
        },
        Mode::Mixed => match code {
            0 => Tok::PSh,
            1 => Tok::Chars(b" "),
            2..=14 => Tok::Byte(code as u8 - 1),          // ^A..^M
            15..=19 => Tok::Byte(0x1b + code as u8 - 15), // ^[ ^\ ^] ^^ ^_
            20..=27 => Tok::Chars(&MIXED_TAIL[(code - 20) as usize..(code - 19) as usize]),
            28 => Tok::Latch(Mode::Lower),
            29 => Tok::Latch(Mode::Upper),
            30 => Tok::Latch(Mode::Punct),
            _ => Tok::Bin,
        },
        Mode::Punct => match code {
            0 => Tok::Flg,
            1 => Tok::Chars(b"\r"),
            2 => Tok::Chars(b"\r\n"),
            3 => Tok::Chars(b". "),
            4 => Tok::Chars(b", "),
            5 => Tok::Chars(b": "),
            6..=30 => Tok::Chars(&PUNCT[(code - 6) as usize..(code - 5) as usize]),
            _ => Tok::USh,
        },
        Mode::Digit => match code {
            0 => Tok::PSh,
            1 => Tok::Chars(b" "),
            2..=11 => Tok::Byte(b'0' + code as u8 - 2),
            12 => Tok::Byte(b','),
            13 => Tok::Byte(b'.'),
            14 => Tok::Latch(Mode::Upper),
            _ => Tok::USh,
        },
    }
}

/// Concatenate b-bit codewords MSB-first into a bit string.
fn concat_bits(data: &[u16], b: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * b);
    for &v in data {
        for i in (0..b).rev() {
            out.push((v >> i) as u8 & 1);
        }
    }
    out
}

/// Remove stuff bits (window-relative model, reproduced bit-exact against
/// zint for 'Code 128!' and 'é'): the data codewords ARE the b-bit windows
/// of the written stream, and the window grid is fixed — a window whose
/// first b−1 bits are all equal carries the complementary stuff bit as its
/// b-th bit, so exactly that bit is dropped and following windows stay at
/// their b-bit positions. The final window is written raw (a stuff there
/// would need a codeword past the data region), as is any truncated tail.
fn destuff(bits: &[u8], b: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bits.len());
    let mut pos = 0;
    while pos + b <= bits.len() {
        let run = &bits[pos..pos + b - 1];
        let stuffed = pos + b < bits.len() && run.iter().all(|&x| x == run[0]);
        let keep = if stuffed { b - 1 } else { b };
        out.extend_from_slice(&bits[pos..pos + keep]);
        pos += b;
    }
    out.extend_from_slice(bits.get(pos..).unwrap_or(&[]));
    out
}

struct Reader<'a> {
    bits: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<u16> {
        if self.pos + n > self.bits.len() {
            return None;
        }
        let mut v = 0u16;
        for &bit in &self.bits[self.pos..self.pos + n] {
            v = (v << 1) | bit as u16;
        }
        self.pos += n;
        Some(v)
    }

    fn take_bytes(&mut self, n: usize, out: &mut Vec<u8>) -> bool {
        if self.pos + 8 * n > self.bits.len() {
            return false;
        }
        for _ in 0..n {
            out.push(self.take(8).unwrap() as u8);
        }
        true
    }
}

/// Decode the message text (raw bytes) from a corrected stream.
pub fn decode_text(stream: &Stream) -> Vec<u8> {
    let b = bits_for_layers(stream.layers) as usize;
    let logical = destuff(&concat_bits(&stream.data, b), b);
    let mut r = Reader { bits: &logical, pos: 0 };
    let mut out = Vec::new();
    let mut mode = Mode::Upper;
    let mut shift: Option<Mode> = None;
    loop {
        let eff = shift.unwrap_or(mode);
        let width = if eff == Mode::Digit { 4 } else { 5 };
        let Some(code) = r.take(width) else { break };
        shift = None;
        match token(eff, code) {
            Tok::Byte(b) => out.push(b),
            Tok::Chars(cs) => out.extend_from_slice(cs),
            Tok::PSh => shift = Some(Mode::Punct),
            Tok::USh => shift = Some(Mode::Upper),
            Tok::Latch(m) => mode = m,
            Tok::Bin => {
                let Some(len) = r.take(5) else { break };
                let count = if len == 0 {
                    let Some(ext) = r.take(11) else { break };
                    ext as usize + 31
                } else {
                    len as usize
                };
                if !r.take_bytes(count, &mut out) {
                    break;
                }
            }
            Tok::Flg => {
                let Some(n) = r.take(3) else { break };
                if n == 0 {
                    out.push(0x1d); // FNC1
                } else if n <= 6 {
                    // ECI: n 4-bit digits, zero-padded to 6; the assignment
                    // is recorded by full readers — raw bytes for us.
                    if r.take(4 * n as usize).is_none() {
                        break;
                    }
                } else {
                    break; // n = 7 reserved
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(layers: u8, data: &[u16]) -> Stream {
        Stream { layers, data_cw: data.len() as u16, data: data.to_vec() }
    }

    /// Mini-encoder per the zint-verified window-relative model: fill
    /// b-bit windows with message bits; when a window's first b−1 bits
    /// come out all equal, a complementary stuff bit is appended as the
    /// window's b-th bit and the next message bit carries into the
    /// following window; the final window is raw, padded with ones
    /// (all-ones -> last bit 0). Reproduces zint's windows bit-exactly
    /// for the stuffed oracles ('Code 128!', 'é').
    fn encode_stream(msg: &[u8], b: usize) -> Vec<u16> {
        let mut windows: Vec<Vec<u8>> = Vec::new();
        let mut queue: Vec<u8> = msg.to_vec();
        while queue.len() > b {
            if queue[..b - 1].iter().all(|&x| x == queue[0]) {
                let mut w = queue[..b - 1].to_vec();
                w.push(1 - queue[0]);
                windows.push(w);
                queue.drain(..b - 1);
            } else {
                windows.push(queue[..b].to_vec());
                queue.drain(..b);
            }
        }
        while queue.len() < b {
            queue.push(1);
        }
        if queue.iter().all(|&x| x == 1) {
            let n = queue.len() - 1;
            queue[n] = 0;
        }
        windows.push(queue);
        windows
            .iter()
            .map(|w| w.iter().fold(0u16, |a, &x| (a << 1) | x as u16))
            .collect()
    }

    fn encoded_text(msg: &[u8], layers: u8) -> Vec<u8> {
        let b = bits_for_layers(layers) as usize;
        decode_text(&stream(layers, &encode_stream(msg, b)))
    }

    /// Oracle data words (docs/STATUS.md) -> payloads.
    #[test]
    fn decodes_oracle_data_words() {
        assert_eq!(decode_text(&stream(2, &[7])), b"B");
        assert_eq!(decode_text(&stream(2, &[60, 55])), b"4");
        assert_eq!(decode_text(&stream(2, &[60, 23])), b"0");
        assert_eq!(decode_text(&stream(2, &[60, 50, 31])), b"42");
        assert_eq!(decode_text(&stream(2, &[60, 33, 31])), b"20");
        // lowercase -> digit needs the explicit D/L latch
        assert_eq!(decode_text(&stream(1, &[56, 11, 51, 31])), b"a4");
        // 'é' = UTF-8 bytes via B/S with a stuff bit in the first window
        assert_eq!(decode_text(&stream(2, &[62, 5, 33, 53, 15])), "é".as_bytes());
    }

    /// Build message bits from (code, width) pairs — the encoder's input.
    fn bits_of(codes: &[(u16, usize)]) -> Vec<u8> {
        let mut msg = Vec::new();
        for &(code, w) in codes {
            for i in (0..w).rev() {
                msg.push((code >> i) as u8 & 1);
            }
        }
        msg
    }

    #[test]
    fn round_trips_latches_and_shifts() {
        let cases: &[(&str, Vec<(u16, usize)>)] = &[
            ("hello world", vec![(28, 5)] // L/L
                .into_iter()
                .chain("hello".bytes().map(|c| ((2 + c - b'a') as u16, 5)))
                .chain(vec![(1, 5)]) // SP
                .chain("world".bytes().map(|c| ((2 + c - b'a') as u16, 5)))
                .collect()),
            ("Code 128!", vec![
                (4, 5),   // C
                (28, 5),  // L/L
                (16, 5), (5, 5), (6, 5), // o d e
                (1, 5),   // SP
                (30, 5),  // D/L
                (3, 4), (4, 4), (10, 4), // 1 2 8
                (0, 4),   // P/S (digit-mode code width)
                (6, 5),   // !
            ]),
            ("a\r\n: ", vec![(28, 5), (2, 5), (0, 5), (2, 5), (0, 5), (5, 5)]),
            // x via L/L, DEL via M/L + mixed 27
            ("x\x7f", vec![(28, 5), (25, 5), (29, 5), (27, 5)]),
            // 99 bottles: digits, digit-SP, U/L, L/L, lowercase
            ("99 bottles", vec![
                (30, 5), (11, 4), (11, 4), (1, 4), (14, 4), (28, 5),
                (3, 5), (16, 5), (21, 5), (21, 5), (13, 5), (6, 5), (20, 5),
            ]),
        ];
        for (want, codes) in cases {
            assert_eq!(encoded_text(&bits_of(codes), 2), want.as_bytes(), "{want}");
        }
    }

    #[test]
    fn binary_length_extensions() {
        // 31 zero bytes: B/S, len 0, 11-bit extension 0.
        let mut msg = vec![1u8; 10]; // B/S + len 0
        msg.extend_from_slice(&[0u8; 11]); // ext = 0 -> 31 bytes
        msg.extend_from_slice(&[0u8; 31 * 8]);
        assert_eq!(encoded_text(&msg, 2), vec![0u8; 31]);
    }
}
