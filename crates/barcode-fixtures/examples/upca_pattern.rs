//! Print the 95-module UPC-A pattern for a 12-digit string (oracle check).
fn main() {
    let digits = std::env::args().nth(1).expect("12 digits");
    let c = barcode_fixtures::encode_upca(&digits, 1, 8, 4);
    let bit = |x: u32| c.bits()[(x as usize) / 8] >> (x % 8) & 1;
    let m: String = (0..95).map(|i| if bit(4 + i) == 1 { '1' } else { '0' }).collect();
    println!("{m}");
}
