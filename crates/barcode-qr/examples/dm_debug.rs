// Detector diagnostic: encode a fixture, run find_finders, dump what happens.

fn main() {
    std::env::set_var("QR_DBG", "1");
    let args: Vec<String> = std::env::args().collect();
    let version: u32 = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(40);
    let ec_bits: u8 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(3); // Q
    let mask: u8 = args
        .get(3)
        .map(|s| s.parse().unwrap())
        .unwrap_or((version + ec_bits as u32) as u8 % 8);

    let n = 17 + 4 * version;
    // use the decoder crate's own block plan, exactly like the round-trip test
    let ec = match ec_bits {
        0 => barcode_qr::grid::EcLevel::M,
        1 => barcode_qr::grid::EcLevel::L,
        2 => barcode_qr::grid::EcLevel::H,
        _ => barcode_qr::grid::EcLevel::Q,
    };
    let data_len = barcode_qr::codewords::data_capacity(version, ec).unwrap() as usize;
    // same payload as the round-trip test: LCG seeded 3+version
    let mut x = (3 + version) as u8;
    let data: Vec<u8> = (0..data_len)
        .map(|_| {
            x = x.wrapping_mul(133).wrapping_add(7);
            x
        })
        .collect();
    let canvas = barcode_fixtures::encode_qr(version, ec_bits, mask, &data, 4);
    let img = barcode_core::BitImage::new(canvas.width, canvas.height, canvas.bits());
    println!(
        "v{version} ec{ec_bits} mask{mask}: canvas {}x{}",
        canvas.width, canvas.height
    );
    let mut d = barcode_qr::QrDecoder::new();
    match d.find_finders(&img) {
        Some(fs) => {
            println!("FOUND:");
            for f in &fs {
                println!(
                    "  ({:.1}, {:.1}) m={:.2} strong={}",
                    f.x, f.y, f.module, f.strong
                );
            }
        }
        None => println!("NONE (expected the three true finders at m=4 apart 7m)"),
    }
    let _ = n;
}
