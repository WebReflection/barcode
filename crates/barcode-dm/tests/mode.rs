//! Frozen mode-decoder vectors.
//!
//! Each pair is (payload hex, data-codeword-stream hex) generated with
//! zint, read through the validated placement (RS-checked), and verified
//! against both libdmtx and the reference decoder in docs/STATUS.md.
//! Generation recipe: `tools/dmtx_mode_probe.py` + the probe driver from
//! the 2026-10-16 session (see git history of docs/STATUS.md).

use barcode_dm::mode::decode_payload;

fn hb(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|k| u8::from_str_radix(&s[k..k + 2], 16).unwrap())
        .collect()
}

/// (payload, data-codeword stream)
const VECTORS: &[(&str, &str)] = &[
    // ASCII with a digit pair ('12' -> 142) and punctuation.
    ("414231322e43442c3f", "42438e2f44452d40"),
    // ASCII upper shift: 'A\x80B' -> [66, 235, 1, 67, 129].
    ("418042", "42eb014381"),
    // C40 basic 'ABCDEF'.
    ("414243444546", "e659e96d24"),
    // C40 with a leading digit '1AAAAA'.
    ("314141414141", "e6217f59bf"),
    // ASCII 'ABC123': digit pair then a trailing lone digit.
    ("414243313233", "4243448e34"),
    // C40 shift 1 ('\x01' raw).
    ("0141414141414141414141414141", "e6003759bf59bf59bf59bffe"),
    // C40 shift 2 ('!').
    ("2141414141414141414141414141", "e6064f59bf59bf59bf59bffe"),
    // C40 shift 3 ('`', affine 96+v).
    ("6041414141414141414141414141", "e60c8f59bf59bf59bf59bffe"),
    // C40 upper shift: shift2->30, shift1(0), raw.
    ("8041414141414141414141414141414141414141", "e60af1023f59bf59bf59bf59bf59bf59b1fe"),
    // Text shift 3 ('A' via table 64+v).
    ("4161616161616161616161616161", "ef0cb759bf59bf59bf59bffe"),
    // Text shift 2 ('!').
    ("2161616161616161616161616161", "ef064f59bf59bf59bf59bffe"),
    // Text upper shift.
    ("8061616161616161616161616161616161616161", "ef0af1023f59bf59bf59bf59bf59bf59b1fe"),
    // Text basic only, exact fill (no unlatch).
    ("6161616161616161616161616161", "ef59bf59bf59bf59bf59b1fe"),
    // X12 with '*' first and trailing 254 + 129.
    ("2a4141414141", "ee087f59bf"),
    // X12 exact fill ('AAAAA*', no 254/129).
    ("41414141412a", "ee59bf59b2"),
    // EDIFACT, payload exactly two groups.
    ("41422e43442c453f", "f0042b8312c17f81"),
    // EDIFACT-capable payload where zint ties to ASCII.
    ("41422e43442c45", "42432f44452d4681"),
    // EDIFACT two groups + leftover '?' riding as ASCII (cw 64).
    ("41422c43442e45463f", "f0042b0312e14640"),
    // ASCII dots ('A.B.C.D').
    ("412e422e432e44", "422f432f442f4581"),
    // Base256, segment at symbol start (len at p=2).
    ("c8c9cacbcccdcecfd0d1", "e7368920b64de47a11a73ed5"),
    // Base256 after nothing else: includes the wrapped stored value 0
    // (v=209, p=14) that pins the mod-256 rule, plus a 129 EOD after
    // the segment (zint quirk).
    ("4142c8c9cacbcccdcecfd0d1", "e7380299b44be2780fa53cd3690081ed"),
    // C40 (9 A's) then Base256 starting at 1-based p=9: absolute
    // position convention (3 A's + 10 bytes ride Base256, n=13).
    (
        "414141414141414141414141c8c9cacbcccdcecfd0d1",
        "e659bf59bf59bffee7e5af45daf78d24ba51e87e15ab42813bd168fe962d",
    ),
];

#[test]
fn oracle_vectors() {
    for (payload, stream) in VECTORS {
        let (p, s) = (hb(payload), hb(stream));
        assert_eq!(
            decode_payload(&s).as_deref(),
            Some(p.as_slice()),
            "stream {stream} for payload {payload}"
        );
    }
}

#[test]
fn upper_shift_also_applies_to_basic_values() {
    // [230, 10, 255] = values (shift2, 30, 14): upper shift then a basic
    // value 'A' -> 65 + 128. Encoders wrap the reduced byte in shift 1
    // (see the \x8e vector), but the flag semantics are uniform: the
    // next produced character gets +128 whatever encoded it.
    assert_eq!(decode_payload(&[230, 10, 255]).as_deref(), Some(&[0xC1][..]));
}

#[test]
fn edifact_explicit_unlatch() {
    // First 6-bit value 31 unlatches; the rest of the triplet is
    // ignored, then ASCII resumes (cw 66 = 'A').
    assert_eq!(
        decode_payload(&[240, 0x7c, 0x00, 0x00, 66, 129]).as_deref(),
        Some(&b"A"[..])
    );
}

#[test]
fn trailing_pad_zero_produces_no_char() {
    // [230, pair(14,14,0), 254, 129]: the trailing 0 value is padding;
    // a decoder that checks 254 first must not emit \x00 or \x0e.
    assert_eq!(
        decode_payload(&[230, 89, 177, 254, 129]).as_deref(),
        Some(&b"AA"[..])
    );
}

#[test]
fn reserved_codewords_are_rejected() {
    assert_eq!(decode_payload(&[0]), None);
    assert_eq!(decode_payload(&[255]), None);
    assert_eq!(decode_payload(&[236, 129]), None);
    assert_eq!(decode_payload(&[231, 200]), None); // length overrun
}
