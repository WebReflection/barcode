//! Interpret codeword lists (comma-separated, one per line) via modes::interpret.
use std::io::Read;

fn main() {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cws: Vec<u16> = line
            .split(',')
            .filter_map(|t| t.trim().parse().ok())
            .collect();
        match barcode_pdf417::modes::interpret(&cws) {
            Some(bytes) => println!("{line}\n  -> {:?}", String::from_utf8_lossy(&bytes)),
            None => println!("{line}\n  -> <parked/none>"),
        }
    }
}
