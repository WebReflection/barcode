//! Encoder vs the frozen zint truth (testdata/aztec_truth.txt): byte
//! identity at every size, size selection, and round-trips through the
//! crate's own decoder.

use barcode_aztec::encode::{encode, encode_in, EncodeError};
use barcode_aztec::tables::{all_specs, spec_for};

fn load_truth() -> Option<Vec<(u8, u16, u16, Vec<u8>)>> {
    let text = std::fs::read_to_string("testdata/aztec_truth.txt").ok()?;
    let mut out: Vec<(u8, u16, u16, Vec<u8>)> = Vec::new();
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.first() == Some(&"VERS") {
            out.push((
                t[1].parse().unwrap(),
                t[2].parse().unwrap(),
                t[3].parse().unwrap(),
                Vec::new(),
            ));
        } else if t.first() == Some(&"MATRIX") && t.len() > 1 {
            let row: Vec<u8> = t[1].chars().map(|ch| (ch == '#') as u8).collect();
            out.last_mut().unwrap().3.extend(row);
        }
    }
    Some(out)
}

/// GOLDEN: 'A' painted byte-identically to zint at ALL 36 sizes.
#[test]
fn byte_identical_to_zint_every_size() {
    let Some(truth) = load_truth() else {
        eprintln!("skipped: testdata/aztec_truth.txt absent");
        return;
    };
    assert_eq!(truth.len(), 36, "truth covers all 36 sizes");
    for (vers, rows, cols, matrix) in &truth {
        let spec = spec_for(*rows, *cols, *vers <= 4).expect("spec");
        let mine = encode_in(b"A", spec).unwrap_or_else(|e| panic!("vers {vers}: {e:?}"));
        assert_eq!(mine.len(), matrix.len(), "vers {vers}: len");
        assert_eq!(mine, *matrix, "vers {vers}: matrix mismatch");
    }
}

/// Size selection: smallest symbol that holds the payload.
#[test]
fn smallest_symbol_selection() {
    let (spec, _) = encode(b"A").unwrap();
    assert_eq!((spec.rows, spec.compact), (15, true));
    let (spec, _) = encode(b"hello world").unwrap();
    assert_eq!((spec.rows, spec.compact), (15, true));
}

/// Round-trips through the crate's own decoder.
#[test]
fn round_trips() {
    let cases: Vec<Vec<u8>> = vec![
        b"A".to_vec(),
        b"hello world".to_vec(),
        b"https://example.com/aztec".to_vec(),
        b"AZTEC-PHYS-001".to_vec(),
        b"1234567890".to_vec(),
        "Héllo — ünïcode ✓".as_bytes().to_vec(),
        b"Hello World 0123456789 - mixed case forces byte mode".to_vec(),
        (0u8..=255).collect(),
        vec![0xffu8; 40], // stuffing trigger
        vec![0x00u8; 40], // zero-run stuffing trigger
        vec![b'x'; 300],  // multi-B/S split
        (0u8..=255).cycle().take(2000).collect(),
    ];
    for text in &cases {
        let (spec, matrix) = encode(text).unwrap();
        let stream = barcode_aztec::read_stream(&matrix, spec).unwrap_or_else(|| {
            panic!(
                "{:?}: no stream ({}x{})",
                String::from_utf8_lossy(&text[..text.len().min(12)]),
                spec.rows,
                spec.layers
            )
        });
        assert_eq!(
            &barcode_aztec::text::decode_text(&stream),
            text,
            "{:?} via {}x{} L{}",
            String::from_utf8_lossy(&text[..text.len().min(12)]),
            spec.rows,
            spec.cols,
            spec.layers
        );
    }
}

/// Oversized payloads are refused, not mis-encoded.
#[test]
fn payload_too_large() {
    let big = vec![b'a'; 6000];
    assert_eq!(encode(&big), Err(EncodeError::PayloadTooLarge));
    // ...and every individual size refuses it too
    for spec in all_specs() {
        assert_eq!(encode_in(&big, spec), Err(EncodeError::PayloadTooLarge));
    }
}
