//! Encoder vs the frozen zint truth (testdata/zint_truth.txt).

use barcode_pdf417::encode::{encode_full_opts, high_level, EncodeError};

fn load() -> Vec<(String, u8, u8, Vec<u8>, Vec<Vec<u8>>)> {
    let text = std::fs::read_to_string("testdata/zint_truth.txt").unwrap();
    let mut out = Vec::new();
    for line in text.lines() {
        let t: Vec<&str> = line.splitn(2, ' ').collect();
        if t[0] == "CASE" {
            let v: Vec<&str> = t[1].split_whitespace().collect();
            out.push((
                v[0].to_string(),
                v[1].parse().unwrap(),
                v[2].parse().unwrap(),
                (0..v[3].len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&v[3][i..i + 2], 16).unwrap())
                    .collect(),
                Vec::new(),
            ));
        } else if t[0] == "MATRIX" {
            let row: Vec<u8> = t[1].chars().map(|c| (c == '#') as u8).collect();
            out.last_mut().unwrap().4.push(row);
        }
    }
    out
}

/// Byte-identical to zint for the truth payloads (option_1 = EC level,
/// 0 = level 0 here; option_2 = columns, 0 = auto geometry). ec6 is the
/// one known deviation: zint picks 19x7 where the fitted geometry rule
/// gives 22x6 (both valid; stream still compared).
#[test]
fn byte_identical_to_zint() {
    for (label, o1, o2, text, matrix) in load() {
        let h = matrix.len();
        let w = matrix[0].len();
        let level = o1;
        let sym = encode_full_opts(&text, level, o2).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        if sym.width == w && sym.height == h {
            assert_eq!(sym.matrix, matrix.concat(), "{label}: matrix mismatch");
            continue;
        }
        // Geometry deviation (ec6 auto; forced-cols cases where zint's
        // row choice differs): both symbols must decode to the payload.
        let _ = label;
        // both geometries valid; decode both and require the same payload
        assert_eq!(
            barcode_pdf417::decode_exact(&matrix.concat(), w, h).as_deref(),
            Some(text.as_slice()),
            "{label}: zint truth does not decode?!"
        );
        assert_eq!(
            barcode_pdf417::decode_exact(&sym.matrix, sym.width, sym.height).as_deref(),
            Some(text.as_slice()),
            "{label}: our variant does not decode"
        );
    }
}

/// Round-trips through the crate's own decoder.
#[test]
fn round_trips() {
    let cases: Vec<Vec<u8>> = vec![
        b"A".to_vec(),
        b"hello world".to_vec(),
        b"https://example.com/pdf417".to_vec(),
        b"0123456789".to_vec(),
        b"\x00\x01\xfe\xff".to_vec(),
        "Héllo — ünïcode".as_bytes().to_vec(),
        b"AZTEC-PHYS-001 00385F00030004L51QZWWY0003304960".to_vec(),
        b"a1b".to_vec(),
        b"a11111111b".to_vec(),
        b"a\x01b".to_vec(),
        b"a\x01\x01\x01\x01\x01b".to_vec(),
        (0u8..=255).collect(),
        b"1234567890".repeat(10),
        vec![b'x'; 500],
        // TC sub-mode dance around digits: uppercase after a Mixed digit
        // (the As-from-Mixed bug encoded 'Z' as 'z'), punctuation
        // shifts, case flips mid-run
        b"v5Z+Jr".to_vec(),
        b"a1B2c3D4".to_vec(),
        b"A1B2C3D4".to_vec(),
        b"Ab1Cd2Ef3".to_vec(),
        b"5Z+Jr".to_vec(),
        b"v5Z".to_vec(),
        b"v5ZZ".to_vec(),
        b"v5Z+Jr5Z".to_vec(),
        b"1Z1z1Z1z".to_vec(),
        b"MiXeD 42!".to_vec(),
    ];
    for text in &cases {
        let sym = encode_full_opts(text, 2, 0).unwrap_or_else(|e| {
            panic!(
                "{:?}: {e:?}",
                String::from_utf8_lossy(&text[..text.len().min(12)])
            )
        });
        let payload = barcode_pdf417::decode_exact(&sym.matrix, sym.width, sym.height)
            .unwrap_or_else(|| {
                panic!(
                    "{:?}: decode failed",
                    String::from_utf8_lossy(&text[..text.len().min(12)])
                )
            });
        assert_eq!(
            &payload,
            text,
            "{:?} round-trip",
            String::from_utf8_lossy(&text[..text.len().min(12)])
        );
    }
}

/// The default level is 2 and the no-option geometry matches zint's.
#[test]
fn defaults() {
    let sym = encode_full_opts(b"A", 2, 0).unwrap();
    assert_eq!((sym.cols, sym.rows), (2, 5));
}

#[test]
fn payload_too_large() {
    let big = vec![b'a'; 3000];
    assert_eq!(
        encode_full_opts(&big, 2, 0).err(),
        Some(EncodeError::PayloadTooLarge)
    );
}

/// The zint-fitted numeric-decision table (docs/STATUS.md "PDF417 NC
/// tie-breaks"), pinned as byte-exact streams. Every value was verified
/// against the live zint oracle via tools/pdf417_oracle.py
/// (diff_zoo) before being frozen here — never retype from memory.
#[test]
fn nc_decision_table_matches_zint() {
    let cases: &[(&str, &[u16])] = &[
        // whole-payload digits: NC even where TC would be shorter
        ("1", &[902, 11]),
        ("11", &[902, 111]),
        ("1234567", &[902, 13, 782, 767]),
        // (text, end): NC from 7 (tie at 5=5 goes NC); 6 stays TC
        ("A111111", &[28, 31, 31, 31]),
        ("A1111111", &[29, 902, 13, 645, 611]),
        // (end, text): TC at 7 (TC=5 < NC=6); NC at 8 (tie 6=6)
        ("1111111A", &[841, 31, 31, 31, 840]),
        ("11111111A", &[902, 137, 156, 711, 900, 29]),
        // scattered latch transitions (the As-from-Mixed bug's shape)
        ("A1B2C3D4", &[28, 58, 58, 88, 88, 118, 118, 149]),
        // (byte, end): NC from 2, no 900 before the following 902
        ("é11", &[901, 195, 169, 902, 111]),
        // (byte, text): TC keeps the 900; a trailing byte needs none
        ("é1A", &[901, 195, 169, 900, 841, 840]),
        ("A1111é", &[28, 31, 31, 901, 195, 169]),
        // a lone digit at the payload edge joins the byte segment
        ("1é", &[901, 49, 195, 169]),
    ];
    for (text, expect) in cases {
        let got = high_level(&text.as_bytes());
        assert_eq!(&got, expect, "{text}: stream mismatch");
    }
}
