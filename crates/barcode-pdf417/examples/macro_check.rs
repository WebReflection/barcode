//! Decode exact matrices (cwprobe input format) through the full
//! decode_exact path and print the payload text.
//! Usage: cargo run -p barcode-pdf417 --example macro_check -- <file>
use std::fs;

fn main() {
    let path = std::env::args().nth(1).expect("usage: macro_check <file>");
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
        match barcode_pdf417::decode_exact(&flat, w, h) {
            Some(bytes) => println!("{label} -> {:?}", String::from_utf8_lossy(&bytes)),
            None => println!("{label} -> FAILED"),
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
