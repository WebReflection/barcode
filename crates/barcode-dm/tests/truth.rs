//! Integration tests against the banked libdmtx/zint ground truth
//! (crates/barcode-dm/testdata/dm_truth.txt — generated from
//! dm_truth.json by tools/dmtx_truth_txt.py; skipped when absent).

use barcode_dm::deinterleave;
use barcode_dm::placement;
use barcode_dm::tables;

struct Entry {
    rows: usize,
    cols: usize,
    code: Vec<u8>,
    matrix: Vec<u8>,
}

fn load_truth() -> Option<Vec<(String, Entry)>> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/dm_truth.txt");
    let src = std::fs::read_to_string(path).ok()?;
    let mut out = Vec::new();
    let mut lines = src.lines().filter(|l| !l.trim().is_empty());
    while let Some(size_line) = lines.next() {
        let mut it = size_line.split_whitespace();
        assert_eq!(it.next(), Some("SIZE"));
        let rows: usize = it.next().unwrap().parse().unwrap();
        let cols: usize = it.next().unwrap().parse().unwrap();
        let n_cw: usize = it.next().unwrap().parse().unwrap();
        let code_line = lines.next().unwrap();
        assert!(code_line.starts_with("CODE "));
        let code: Vec<u8> = code_line[5..]
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(code.len(), n_cw);
        let matrix_line = lines.next().unwrap();
        assert!(matrix_line.starts_with("MATRIX "));
        let matrix: Vec<u8> = matrix_line[7..]
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(matrix.len(), rows * cols);
        let key = format!("{}x{}", rows, cols);
        out.push((key, Entry { rows, cols, code, matrix }));
    }
    Some(out)
}

#[test]
fn placement_reads_libdmtx_codewords_for_every_size() {
    let Some(truth) = load_truth() else {
        eprintln!("skipping: testdata/dm_truth.txt not present (see tools/dmtx_truth_txt.py)");
        return;
    };
    assert_eq!(truth.len(), 30, "expected all 30 sizes in the truth bank");
    for (key, e) in &truth {
        let spec = tables::spec_for(e.rows, e.cols)
            .unwrap_or_else(|| panic!("{}: no spec for {}x{}", key, e.rows, e.cols));
        let got = placement::read_codewords(&e.matrix, e.rows, e.cols, spec, e.code.len());
        assert_eq!(got, e.code, "{}: placement read mismatch", key);
    }
}

#[test]
fn deinterleave_rs_validate_every_size() {
    let Some(truth) = load_truth() else {
        eprintln!("skipping: testdata/dm_truth.txt not present (see tools/dmtx_truth_txt.py)");
        return;
    };
    for (key, e) in &truth {
        let spec = tables::spec_for(e.rows, e.cols).unwrap();
        assert_eq!(e.code.len(), spec.codewords(), "{}", key);
        let data = deinterleave::decode_stream(spec, &e.code)
            .unwrap_or_else(|| panic!("{}: RS correction failed", key));
        assert_eq!(data.len(), spec.data, "{}", key);
        // 'A' payload: data starts with 66 ('A'+1), then 129 (terminator)
        assert_eq!(data[0], 66, "{}: payload", key);
        if spec.data > 1 {
            assert_eq!(data[1], 129, "{}: terminator", key);
        }
    }
}
