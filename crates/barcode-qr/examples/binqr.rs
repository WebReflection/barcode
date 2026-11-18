//! Emit a binary-payload QR matrix for external (zxing-cpp) verification.
//! Usage: binqr <hex payload> <version> <ec_bits>
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let data: Vec<u8> = (0..a[0].len() / 2)
        .map(|i| u8::from_str_radix(&a[0][i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    let version: u32 = a[1].parse().unwrap();
    let ec: u8 = a[2].parse().unwrap();
    let cap = barcode_fixtures::qr_data_codewords(version, ec) as usize;
    // byte-mode framing: mode 4 + count + data + EC/11 pads
    let count_bits = if version <= 9 { 8 } else { 16 };
    let mut bits: Vec<bool> = vec![false, true, false, false];
    for i in (0..count_bits).rev() {
        bits.push((data.len() >> i) & 1 == 1);
    }
    for &b in &data {
        for i in (0..8).rev() {
            bits.push((b >> i) & 1 == 1);
        }
    }
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut stream: Vec<u8> = bits
        .chunks(8)
        .map(|c| c.iter().fold(0u8, |a, &b| (a << 1) | b as u8))
        .collect();
    let mut pad = true;
    while stream.len() < cap {
        stream.push(if pad { 0xEC } else { 0x11 });
        pad = !pad;
    }
    let canvas = barcode_fixtures::encode_qr(version, ec, 2, &stream, 4);
    let bits = canvas.bits();
    let bpr = canvas.bytes_per_row as usize;
    let mut out = std::io::stdout();
    for y in 0..canvas.height as usize {
        let _ = out.write_all(b"\n");
        for x in 0..canvas.width as usize {
            let v = (bits[y * bpr + (x >> 3)] >> (x & 7)) & 1;
            let _ = out.write_all(if v == 1 { b"#" } else { b"." });
        }
    }
    let _ = out.write_all(b"\n");
}
