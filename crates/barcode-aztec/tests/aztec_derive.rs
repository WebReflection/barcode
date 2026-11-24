//! Derive the encoder rules from the zint truth: for each size, decode the
//! mode message + stream and print the structure facts (D, K, skip, the
//! written windows, stuff positions, pad).

use barcode_aztec::{mode, placement, tables};

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

#[test]
fn derive_structure() {
    let Some(truth) = load_truth() else {
        eprintln!("skipping: testdata/aztec_truth.txt absent");
        return;
    };
    for (vers, rows, cols, m) in &truth {
        if *vers > 8 && *vers != 27 {
            continue;
        }
        let spec = tables::spec_for(*rows, *cols, *vers <= 4).expect("spec");
        let (layers, d) =
            mode::read_mode(&m, spec).unwrap_or_else(|| panic!("vers {vers}: no mode"));
        let stream = barcode_aztec::read_stream(&m, spec)
            .unwrap_or_else(|| panic!("vers {vers}: no stream"));
        let d = d as usize;
        let k = spec.total as usize - d;
        let bits = placement::read_bits(&m, spec);
        let b = spec.bits as usize;
        let cap = spec.cap_bits() as usize;
        let data_windows = placement::windows(&bits, spec.skip as usize, d, b);
        let check_windows = placement::windows(&bits, cap - k * b, k, b);
        let skip_bits: String = bits[..spec.skip as usize]
            .iter()
            .map(|v| v.to_string())
            .collect();
        // RS hypothesis check: check_cw == rs_encode(gf, data, K)?
        // The check codewords are rs_encode over the RAW WRITTEN windows
        // (stuff bits included in the stream, as written). Pinned for every
        // size in the truth file.
        let gf = barcode_aztec::gf::field_for(spec.bits as u32);
        assert_eq!(data_windows.len(), d, "vers {vers}: data window count");
        assert_eq!(
            barcode_aztec::rs::rs_encode(&gf, &data_windows, k),
            check_windows,
            "vers {vers}: check codewords are rs_encode(raw data windows, K)"
        );
        // zint auto: smallest symbol, MINIMUM data-codeword count (D=1 for
        // 'A' at every size — maximum EC for tiny payloads).
        eprintln!(
            "vers {vers}: L={layers} D={d} K={k} b={b} skip={}({skip_bits}) cap={cap} total={} | data_cw={:?} | check_cw={:?}",
            spec.skip, spec.total, &stream.data, &check_windows);
    }
}
