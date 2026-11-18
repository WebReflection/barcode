//! Codeword probe: read exact matrices, print their full streams.
//! Input file: symbols separated by a line starting with "SYM label".
//! Matrix rows are '#'/'.' lines (1 module per cell, '#' = ink).
//! Usage: cargo run -p barcode-pdf417 --example cwprobe -- <file>
use std::fs;

fn main() {
    let path = std::env::args().nth(1).expect("usage: cwprobe <file>");
    let text = fs::read_to_string(path).unwrap();
    let mut label = String::new();
    let mut rows: Vec<Vec<u8>> = Vec::new();
    let mut flush = |label: &str, rows: &mut Vec<Vec<u8>>| {
        if rows.is_empty() {
            return;
        }
        let h = rows.len();
        let w = rows[0].len();
        let flat: Vec<u8> = rows.iter().flatten().copied().collect();
        match barcode_pdf417::stream::read_exact(&flat, w, h) {
            Some(stream) => {
                let (desc, data, _ecc) = barcode_pdf417::stream::split_stream(&stream).unwrap();
                let data = barcode_pdf417::stream::strip_pads(data);
                println!(
                    "{} {}x{} desc={} n={}",
                    label,
                    w,
                    h,
                    desc,
                    data.len()
                );
                println!("  stream: {}", stream.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","));
                println!("  data:   {}", data.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","));
            }
            None => println!("{label} FAILED to read"),
        }
        rows.clear();
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("SYM ") {
            flush(&label, &mut rows);
            label = rest.trim().to_string();
        } else if line.starts_with('#') || line.starts_with('.') {
            rows.push(line.bytes().map(|b| (b == b'#') as u8).collect());
        }
    }
    flush(&label, &mut rows);
}
