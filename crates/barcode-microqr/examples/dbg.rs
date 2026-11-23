use barcode_core::BitImage;
use barcode_fixtures::Canvas;
use barcode_microqr::{decode, decoder, encode, Level, Version};

fn main() {
    let sym = encode("12345", Version::M1, Level::L, None).unwrap();
    let m = 4u32;
    let q = 4u32;
    let dim = (sym.size as u32 + 2 * q) * m;
    let mut c = Canvas::new(dim, dim);
    for y in 0..sym.size {
        for x in 0..sym.size {
            if sym.cells[y * sym.size + x] == 1 {
                c.fill((q + x as u32) * m, (q + y as u32) * m, m, m);
            }
        }
    }
    let img = BitImage::new(c.width, c.height, c.bits());
    println!("dim={} finder={:?}", dim, decoder::find_finder(&img));
    for j in 0..sym.size {
        println!(
            "{}",
            (0..sym.size)
                .map(|i| if sym.cells[j * sym.size + i] == 1 { '#' } else { '.' })
                .collect::<String>()
        );
    }
    println!("decode_grid: {:?}", decoder::decode_grid(&sym.cells, sym.size));
    println!("decode: {:?}", decode(&img));
}
