//! High-level mode interpretation (spec 2.2.4/2.2.5): TC/BC/NC
//! segments -> payload bytes.
//!
//! Latches (900-928) can never be segment data: byte groups are
//! 6*255 < 900^5, numeric groups < 900, and TC codewords max out at
//! 29*30+29 = 899 — so segmenting is unambiguous.

use crate::text::TextState;

/// Decode a full data-codeword sequence (post-descriptor, pre-pads)
/// into the payload bytes. Text, byte and numeric segments may
/// interleave arbitrarily.
///
/// Parked (return None): macro PDF417 control blocks (922/923/928).
/// 921 (reader init) and GLI sequences (925-927 + operands) are
/// skipped; 913 byte shifts and 900 latches keep TC state alive.
pub fn interpret(cws: &[u16]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut st = TextState::new();
    let mut i = 0;
    while i < cws.len() {
        match cws[i] {
            900 | 921 => {
                st.reset();
                i += 1;
            }
            901 | 924 => {
                let j = segment_end(cws, i + 1);
                decode_bc(&cws[i + 1..j], cws[i], &mut out)?;
                i = j;
            }
            902 => {
                let j = segment_end(cws, i + 1);
                decode_nc(&cws[i + 1..j], &mut out)?;
                i = j;
            }
            913 => {
                // byte shift: the next codeword is a single byte, then
                // the previous mode (and TC sub-mode) resumes
                out.push(*cws.get(i + 1)? as u8);
                i += 2;
            }
            925..=927 => {
                // GLI sequence: leader + 1 or 2 operand codewords
                i += match cws[i] {
                    925 | 927 => 2,
                    _ => 3,
                };
            }
            928 => {
                // Macro PDF417 Control Block (Annex H): follows the data
                // block, counted in the length descriptor. Parse it for
                // well-formedness and emit only the segment payload
                // decoded above. Layout: [928] [segment index: 2 cws]
                // [file ID: raw codewords] ([923] [designator] [data])*
                // [922 if last segment].
                let rest = cws.get(i + 1..)?;
                if rest.len() < 2 {
                    return None; // segment index is exactly 2 codewords
                }
                let mut j = 2;
                while j < rest.len() {
                    match rest[j] {
                        922 => {
                            j += 1; // terminator: last segment only
                            break;
                        }
                        923 => {
                            // optional field: designator + data. Fixed-width
                            // fields (count 2, timestamp 4, checksum 2) are
                            // skipped by length; variable fields run to the
                            // next tag/terminator (923/922 are latches that
                            // cannot occur inside valid field data).
                            let (skip, _len) = match rest.get(j + 1) {
                                Some(1) => (1, 2),   // segment count
                                Some(2) => (1, 4),   // time stamp
                                Some(6) => (1, 2),   // checksum
                                Some(d) if (0..=9).contains(d) || (65..=90).contains(d) => (1, 0),
                                _ => return None,    // unknown designator
                            };
                            j += 1 + skip;
                            if _len == 0 {
                                while j < rest.len() && rest[j] != 923 && rest[j] != 922 {
                                    j += 1;
                                }
                            } else {
                                j += _len;
                            }
                        }
                        _ => j += 1, // file ID: raw codewords, taken verbatim
                    }
                }
                if j != rest.len() {
                    return None; // trailing garbage after the block
                }
                break;
            }
            922 | 923 => return None, // outside a control block: invalid
            0..=899 => {
                st.feed(cws[i], &mut out)?;
                i += 1;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// End of a byte/numeric segment: the next latch/shift codeword (>= 900)
/// or the end of the data.
fn segment_end(cws: &[u16], from: usize) -> usize {
    let mut j = from;
    while j < cws.len() && cws[j] < 900 {
        j += 1;
    }
    j
}

/// 6 bytes from 5 codewords (base 900 -> base 256), most significant
/// first (spec 2.2.4.5).
fn bc_group(cws: &[u16], out: &mut Vec<u8>) -> Option<()> {
    if cws.len() != 5 {
        return None;
    }
    let mut v: u64 = 0;
    for &c in cws {
        v = v * 900 + c as u64;
    }
    let mut digits = [0u8; 6];
    for d in digits.iter_mut().rev() {
        *d = (v % 256) as u8;
        v /= 256;
    }
    out.extend_from_slice(&digits);
    Some(())
}

/// Byte segment -> bytes. Latch 924: the byte count is a multiple of 6
/// (all groups full). Latch 901: the last 1-5 codewords are single
/// bytes — the tail size follows from the segment length, since a full
/// trailing group would have meant total%6==0 and the encoder would
/// have used 924.
fn decode_bc(seg: &[u16], latch: u16, out: &mut Vec<u8>) -> Option<()> {
    if latch == 924 {
        if seg.len() % 5 != 0 {
            return None;
        }
        for g in seg.chunks_exact(5) {
            bc_group(g, out)?;
        }
        return Some(());
    }
    let r = ((seg.len() + 4) % 5) + 1; // ((n-1) mod 5) + 1, in 1..=5
    if r > seg.len() {
        return None;
    }
    let (full, tail) = seg.split_at(seg.len() - r);
    for g in full.chunks_exact(5) {
        bc_group(g, out)?;
    }
    out.extend(tail.iter().map(|&c| c as u8));
    Some(())
}

/// Digits from one numeric group: base-900 value -> decimal, leading
/// '1' stripped (spec 2.2.4.6). 15 codewords <-> 44 digits. The value
/// (45 digits) exceeds u128, so the decimal digits are built directly
/// with multiply-add on a digit vector.
fn nc_group(cws: &[u16], out: &mut Vec<u8>) -> Option<()> {
    // zint does not split numeric groups at the recommended 44 digits /
    // 15 codewords (verified: 90 digits -> 31 codewords in one group),
    // and the leading-1 value recovers at any length; only guard against
    // pathological input.
    if cws.is_empty() || cws.len() > 64 {
        return None;
    }
    let mut dec: Vec<u8> = vec![0]; // decimal digits, most significant first
    for &c in cws {
        let mut carry = c as u32;
        for d in dec.iter_mut().rev() {
            let v = *d as u32 * 900 + carry;
            *d = (v % 10) as u8;
            carry = v / 10;
        }
        while carry > 0 {
            dec.insert(0, (carry % 10) as u8);
            carry /= 10;
        }
    }
    if *dec.first()? != 1 {
        return None; // the encoded value must have the leading 1
    }
    out.extend(dec[1..].iter().map(|d| b'0' + d)); // ASCII digits
    Some(())
}

/// Numeric segment -> ASCII digits. zint emits the whole digit run as
/// ONE group (verified: 45 digits -> 16 cws, 90 -> 31), so try that
/// first; fall back to the spec's recommended <= 44-digit / 15-cw
/// chunking for encoders that split.
fn decode_nc(seg: &[u16], out: &mut Vec<u8>) -> Option<()> {
    // Chunked-plausibility: every FULL 15-cw window must decode with the
    // leading 1 (true for spec-style encoders; zint's one-group streams
    // only pass this by luck, and then only for all-1 digit strings).
    let chunked_plausible = seg.len() >= 15
        && (0..seg.len() / 15).all(|w| {
            let mut probe = Vec::new();
            nc_group(&seg[w * 15..w * 15 + 15], &mut probe).is_some()
        });
    if !chunked_plausible && nc_group(seg, out).is_some() {
        return Some(());
    }
    let mut i = 0;
    while seg.len() - i >= 15 {
        nc_group(&seg[i..i + 15], out)?;
        i += 15;
    }
    if i < seg.len() {
        nc_group(&seg[i..], out)?;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec 2.2.4.5 worked example: bytes 01..06 -> [924, 1, 620, 89, 74, 846].
    #[test]
    fn spec_byte_example() {
        assert_eq!(interpret(&[924, 1, 620, 89, 74, 846]).unwrap(), [1u8, 2, 3, 4, 5, 6]);
    }

    /// Spec 2.2.4.6 worked example: 000213298174000.
    #[test]
    fn spec_numeric_example() {
        assert_eq!(
            interpret(&[902, 1, 624, 434, 632, 282, 200]).unwrap(),
            b"000213298174000".to_vec()
        );
    }

    /// Spec 2.2.5 sequence 1: byte shift inside text.
    #[test]
    fn byte_shift_inside_text() {
        // (819, 913, 6, 466) = the spec's sequence 1 (its "<P>" label
        // is a typo: the codewords spell lowercase p, q)
        assert_eq!(interpret(&[819, 913, 6, 466]).unwrap(), b"j\x06pq".to_vec());
    }

    /// Spec 2.2.5 sequence 2: the same data via a BC latch.
    #[test]
    fn byte_latch_inside_text() {
        assert_eq!(
            interpret(&[901, 106, 6, 112, 113]).unwrap(),
            b"j\x06pq".to_vec()
        );
    }

    #[test]
    fn partial_tail_from_segment_length() {
        // 11 bytes: one full group (5 cw) + 5 singles; a full trailing
        // group would have meant total%6==0 and the encoder used 924
        let seg = [1u16, 620, 89, 74, 846, 65, 65, 65, 65, 65];
        let mut out = Vec::new();
        decode_bc(&seg, 901, &mut out).unwrap();
        assert_eq!(out, [1u8, 2, 3, 4, 5, 6, 65, 65, 65, 65, 65]);
    }
}
