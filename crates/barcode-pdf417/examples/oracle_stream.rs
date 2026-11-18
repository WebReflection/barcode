//! Zint-oracle stream reader for PDF417 differential testing.
//!
//! Usage 1 (zint side): `oracle_stream <matrix-file>` — stdin-file holds
//! SYM blocks (`SYM <label>` then '#'/'.' rows, see
//! tools/pdf417_oracle.py); prints the data-region codewords per block
//! (descriptor consumed, trailing pads stripped), one comma-separated
//! line per block.
//!
//! Usage 2 (our side): stdin lines = hex payloads; prints our
//! high_level stream per payload.
//!
//! Our decoder is the authority here: it is zint-verified via
//! tests/golden.rs, so reading zint's matrices through it replaces the
//! lost BMP/pattern-table probe tooling.

use std::io::{BufRead, Write};

fn main() {
    let out = std::io::stdout();
    let mut w = std::io::BufWriter::new(out.lock());
    if let Some(path) = std::env::args().nth(1) {
        let text = std::fs::read_to_string(path).unwrap();
        let mut rows: Vec<Vec<u8>> = Vec::new();
        let flush =
            |rows: &mut Vec<Vec<u8>>, w: &mut std::io::BufWriter<std::io::StdoutLock>| {
                if rows.is_empty() {
                    return;
                }
                let h = rows.len();
                let wd = rows[0].len();
                let flat: Vec<u8> = rows.iter().flatten().copied().collect();
                let stream = barcode_pdf417::stream::read_exact(&flat, wd, h).expect("stream");
                let (desc, data, _) = barcode_pdf417::stream::split_stream(&stream).unwrap();
                let d = barcode_pdf417::stream::strip_pads(&data[..desc as usize - 1]);
                writeln!(
                    w,
                    "{}",
                    d.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
                )
                .unwrap();
                rows.clear();
            };
        for line in text.lines() {
            if line.starts_with("SYM ") {
                flush(&mut rows, &mut w);
            } else if line.starts_with('#') || line.starts_with('.') {
                rows.push(line.bytes().map(|b| (b == b'#') as u8).collect());
            }
        }
        flush(&mut rows, &mut w);
        return;
    }
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        if line.is_empty() {
            continue;
        }
        let bytes: Vec<u8> = (0..line.len() / 2)
            .map(|i| u8::from_str_radix(&line[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        let stream = barcode_pdf417::encode::high_level(&bytes);
        writeln!(
            w,
            "{}",
            stream.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
        )
        .unwrap();
    }
}
