//! Text compaction decoding (spec 2.2.4.4).

use crate::text_tables::{Cell, ALPHA, LOWER, MIXED, PUNCT, TEXT_TABLES};

/// Persistent Text Compaction state: current sub-mode plus a pending
/// shift. Lives across 913 byte shifts (the sub-mode resumes after the
/// shifted byte).
#[derive(Clone, Copy, Debug)]
pub struct TextState {
    sub: usize,
    shift_back: Option<usize>,
}

impl Default for TextState {
    fn default() -> Self {
        TextState {
            sub: ALPHA,
            shift_back: None,
        }
    }
}

impl TextState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Latch back to TC / Alpha (codeword 900, also symbol start).
    pub fn reset(&mut self) {
        self.sub = ALPHA;
        self.shift_back = None;
    }

    /// Feed one TC data codeword (0..=899), appending decoded bytes.
    pub fn feed(&mut self, cw: u16, out: &mut Vec<u8>) -> Option<()> {
        if cw >= 900 {
            return None; // mode codewords are the caller's business
        }
        for &sc in &[cw / 30, cw % 30] {
            let sc = sc as usize;
            if let Some(back) = self.shift_back.take() {
                // shifted sub-value: interpret in the TARGET (current
                // sub-mode), then revert to the base
                out.push(match TEXT_TABLES[self.sub][sc] {
                    Cell::Ch(c) => c,
                    _ => return None, // shift followed by switch: illegal
                });
                self.sub = back;
                continue;
            }
            match TEXT_TABLES[self.sub][sc] {
                Cell::Ch(c) => out.push(c),
                Cell::Ll => self.sub = LOWER,
                Cell::Ml => self.sub = MIXED,
                Cell::Al => self.sub = ALPHA,
                Cell::Pl => self.sub = PUNCT,
                Cell::Ps => {
                    self.shift_back = Some(self.sub);
                    self.sub = PUNCT;
                }
                Cell::As => {
                    self.shift_back = Some(self.sub);
                    self.sub = ALPHA;
                }
            }
        }
        Some(())
    }
}

/// Interpret a text-only codeword sequence (stream starts in TC/Alpha).
/// Trailing pad sub-values (29) are dropped naturally: a dangling
/// shift pushes nothing. (Spec: "if ps is used as a pad immediately
/// preceding a Byte shift, the ps is ignored.")
pub fn decode_text(cws: &[u16]) -> Option<String> {
    let mut st = TextState::new();
    let mut out = Vec::new();
    for &cw in cws {
        match cw {
            900 => st.reset(),
            901..=928 => break,
            0..=899 => st.feed(cw, &mut out)?,
            _ => return None,
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a1_stream() {
        // 'A1': sub-chars A, ml, 1, pad -> 28, 59 (zint-verified)
        assert_eq!(decode_text(&[28, 59]).unwrap(), "A1");
    }

    #[test]
    fn trailing_pad_dropped() {
        // 'A' -> sub-chars A, pad-29
        assert_eq!(decode_text(&[29]).unwrap(), "A");
    }

    #[test]
    fn shift_reverts_to_base() {
        // 'aXb': ll, a, as, X, b -> pairs (27,0)(27,23)(1,29)
        assert_eq!(decode_text(&[810, 833, 59]).unwrap(), "aXb");
    }

    #[test]
    fn latch_route_through_mixed() {
        // 'aXYb': ll a ml al X Y ll b
        assert_eq!(decode_text(&[810, 868, 714, 811]).unwrap(), "aXYb");
    }
}
