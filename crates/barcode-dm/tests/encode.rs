//! Encoder validation: byte-diff the painter against every zint truth
//! matrix (testdata/dm_truth.txt, 'A' payload), and round-trip payloads
//! through our own decoder.

use barcode_dm::{decode_upright, encode, placement, tables};

/// Parse the frozen zint truth: SIZE rows cols nn / CODE ... / MATRIX ...
fn load_truth() -> Option<Vec<((usize, usize), Vec<u8>, Vec<u8>)>> {
    let text = std::fs::read_to_string("testdata/dm_truth.txt").ok()?;
    let mut out = Vec::new();
    let mut cur: Option<(usize, usize)> = None;
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        match t.first().copied() {
            Some("SIZE") => cur = Some((t[1].parse().unwrap(), t[2].parse().unwrap())),
            Some("CODE") if t.len() > 1 => {
                let code: Vec<u8> = t[1..].iter().map(|x| x.parse().unwrap()).collect();
                out.push((cur.unwrap(), code, Vec::new()));
            }
            Some("MATRIX") if t.len() > 1 => {
                let m: Vec<u8> = t[1..].iter().map(|x| x.parse().unwrap()).collect();
                out.last_mut().unwrap().2 = m;
            }
            _ => {}
        }
    }
    Some(out)
}

#[test]
fn decodes_zint_bitmaps_every_size() {
    let Some(truth) = load_truth() else {
        eprintln!("skipping: testdata/dm_truth.txt not present");
        return;
    };
    for ((rows, cols), _, matrix) in &truth {
        let dec = decode_upright(matrix, *rows, *cols)
            .unwrap_or_else(|| panic!("{rows}x{cols}: zint bitmap failed to decode"));
        assert_eq!(dec.payload.as_deref(), Some(b"A".as_slice()), "{rows}x{cols}");
    }
}

#[test]
fn painter_matches_zint_every_size() {
    let Some(truth) = load_truth() else {
        eprintln!("skipping: testdata/dm_truth.txt not present");
        return;
    };
    let mut checked = 0;
    for ((rows, cols), code, matrix) in &truth {
        let spec = tables::spec_for(*rows, *cols).unwrap();
        assert_eq!(code.len(), spec.codewords(), "{}x{} code len", rows, cols);
        let painted = paint_stream(spec, code);
        if painted != *matrix {
            let diffs: Vec<(usize, usize, u8, u8)> = painted
                .iter()
                .zip(matrix.iter())
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .take(12)
                .map(|(i, (a, b))| (i / cols, i % cols, *a, *b))
                .collect();
            panic!("{rows}x{cols}: {} cell diffs, first: {:?}", diffs.len(), diffs);
        }
        checked += 1;
    }
    assert!(checked >= 20, "only {checked} sizes checked");
}

fn paint_stream(spec: &tables::SymbolSpec, stream: &[u8]) -> Vec<u8> {
    // same painter as encode::paint — keep in sync (encode is private there,
    // so re-derive it here via the public path: encode with matching data is
    // not possible for arbitrary streams; paint inline).
    let (r, c) = (spec.rows, spec.cols);
    let mut m = vec![0u8; r * c];
    for i in 0..r {
        m[i * c] = 1;
    }
    for j in 0..c {
        m[(r - 1) * c + j] = 1;
        if j % 2 == 0 {
            m[j] = 1;
        }
    }
    for i in 0..r {
        if i % 2 == 1 {
            m[i * c + c - 1] = 1;
        }
    }
    for (k, &(sr, sc)) in placement::stream_positions(spec).iter().enumerate() {
        let bit = (stream[k / 8] >> (7 - (k % 8))) & 1;
        m[sr as usize * c + sc as usize] = bit;
    }
    if spec.kr == 1 && spec.kc == 1 && spec.map_rows() * spec.map_cols() > 8 * spec.codewords() {
        for &(mr, mc) in &[(spec.s_r - 2, spec.s_c - 2), (spec.s_r - 1, spec.s_c - 1)] {
            let (sr, sc) = spec.to_symbol(mr, mc);
            m[sr * c + sc] = 1;
        }
    }
    for i in 1..spec.kr {
        let y = i * spec.s_r + 2 * (i - 1) + 1;
        for j in 0..c { m[y * c + j] = 1; if j % 2 == 0 { m[(y + 1) * c + j] = 1; } }
    }
    for i in 1..spec.kc {
        let x = i * spec.s_c + 2 * (i - 1) + 1;
        for j in 0..r { if j % 2 == 1 { m[j * c + x] = 1; } m[j * c + x + 1] = 1; }
    }
    m
}

#[test]
fn roundtrips_own_decoder() {
    let payloads: Vec<Vec<u8>> = vec![
        b"A".to_vec(),
        b"00385F00030004L51QZWWY0003304960".to_vec(),
        b"Hello, world!".to_vec(),
        vec![0, 1, 2, 127],
        vec![128, 200, 255],
        b"https://vivaservizi.mef.gov.it/001/AVV/PAG/00385F/0003/0004?L51QZWWY".to_vec(),
        Vec::new(),
        b"0123456789".to_vec(),
    ];
    for p in &payloads {
        let enc = encode::encode(p).expect("encode");
        let (r, c) = (enc.spec.rows, enc.spec.cols);
        let dec = decode_upright(&enc.matrix, r, c)
            .unwrap_or_else(|| panic!("{p:?}: decode failed at {r}x{c}"));
        assert_eq!(
            dec.payload.as_deref(),
            Some(p.as_slice()),
            "{p:?}: payload mismatch at {r}x{c}"
        );
    }
}

#[test]
fn a_is_byte_identical_to_zint() {
    let Some(truth) = load_truth() else {
        eprintln!("skipping: testdata/dm_truth.txt not present");
        return;
    };
    let (_, _, matrix) = truth.iter().find(|((r, c), _, _)| (*r, *c) == (10, 10)).unwrap();
    let enc = encode::encode(b"A").unwrap();
    assert_eq!((enc.spec.rows, enc.spec.cols), (10, 10), "smallest size choice");
    assert_eq!(enc.matrix, *matrix, "10x10 'A' matrix");
}

#[test]
fn too_large_payload_is_none() {
    let big = vec![b'x'; 4000];
    assert!(encode::encode(&big).is_none());
}
