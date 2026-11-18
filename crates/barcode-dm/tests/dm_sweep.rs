use barcode_dm::decode_upright;

#[test]
fn sweep_lengths() {
    for n in 1..=60 {
        let p = vec![b'a' + (n % 20) as u8; n];
        match barcode_dm::encode::encode(&p) {
            Some(enc) => {
                let (r, c) = (enc.spec.rows, enc.spec.cols);
                let dec = decode_upright(&enc.matrix, r, c);
                let ok = dec.and_then(|d| d.payload).as_deref() == Some(p.as_slice());
                if !ok {
                    eprintln!("FAIL n={n} size={r}x{c}");
                }
            }
            None => eprintln!("none n={n}"),
        }
    }
    // digits + spaces + mixed case like the failing payload
    for p in [
        "DATA MATRIX LONGER PAYLOAD 42",
        "DATA MATRIX LONGER PAYLOAD 4",
        "DATA MATRIX LONGER PAYLOAD",
        "DATA MATRIX LONGER PAYLOAD 420",
    ] {
        match barcode_dm::encode::encode(p.as_bytes()) {
            Some(enc) => {
                let (r, c) = (enc.spec.rows, enc.spec.cols);
                let dec = decode_upright(&enc.matrix, r, c);
                let ok = dec.and_then(|d| d.payload).as_deref() == Some(p.as_bytes());
                eprintln!("{} -> {r}x{c} ok={ok}", p);
            }
            None => eprintln!("{p} -> none"),
        }
    }
}
