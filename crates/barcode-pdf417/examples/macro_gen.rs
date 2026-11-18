//! Emit a Macro PDF417 matrix for external verification.
//! Usage: macro_gen <payload> <segment_index> <last01> <file_id_csv> <name|-> <count|-> <ts|-> <size|-> <crc|->
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |s: &String| (s != "-").then(|| s.clone());
    let fid: Vec<u16> = a[3].split(',').map(|t| t.parse().unwrap()).collect();
    let m = barcode_pdf417::encode::encode_macro_full(
        &barcode_pdf417::encode::MacroFields {
            segment_index: a[1].parse().unwrap(),
            file_id: &fid,
            last: a[2] == "1",
            file_name: opt(&a[4]).as_deref(),
            sender: None,
            addressee: None,
            segment_count: opt(&a[5]).map(|s| s.parse().unwrap()),
            timestamp: opt(&a[6]).map(|s| s.parse().unwrap()),
            file_size: opt(&a[7]).map(|s| s.parse().unwrap()),
            checksum: opt(&a[8]).map(|s| s.parse().unwrap()),
        },
        a[0].as_bytes(),
        barcode_pdf417::encode::DEFAULT_LEVEL,
    )
    .unwrap();
    let mut out = std::io::stdout();
    for row in m.matrix.chunks(m.width) {
        let _ = out.write_all(b"\n");
        for &v in row {
            let _ = out.write_all(if v == 1 { b"#" } else { b"." });
        }
    }
    let _ = out.write_all(b"\n");
}
