use barcode_core::BitImage;
use barcode_fixtures::Canvas;
use barcode_microqr::{encode, decode, decoder, Level, Version};

fn main() {
    let sym = encode("5", Version::M1, Level::L, Some(0)).unwrap();
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
    for y in [24u32, 26, 28, 30] {
        let runs: Vec<(bool, u32)> = img.row_runs(y).collect();
        println!("row {y}: {:?} len={}", runs, runs.len());
    }
    let f = decoder::find_finder(&img);
    if let Some(f) = f {
        let cells = (0..11usize)
            .flat_map(|j| (0..11usize).map(move |i| (i, j)))
            .map(|(i, j)| {
                let px = f.x0 + (i as f32 + 0.5) * f.pitch;
                let py = f.y0 + (j as f32 + 0.5) * f.pitch;
                img.ink(px as u32, py as u32) as u8
            })
            .collect::<Vec<u8>>();
        for j in 0..11 {
            println!("{}", (0..11).map(|i| if cells[j * 11 + i] == 1 { '#' } else { '.' }).collect::<String>());
        }
        println!("decode: {:?}", decode(&img));
        // manual frame probe: M1-L num len 1
        let cells11: Vec<u8> = (0..11usize)
            .flat_map(|j| (0..11usize).map(move |i| (i, j)))
            .map(|(i, j)| img.ink((f.x0 as u32) + (i as u32 * 4 + 2), (f.y0 as u32) + (j as u32 * 4 + 2)) as u8)
            .collect();
        let mismatches: Vec<(usize, usize)> = (0..121)
            .filter(|&i| cells11[i] != sym.cells[i])
            .map(|i| (i % 11, i / 11))
            .collect();
        println!("cell mismatches vs encoder: {:?}", mismatches);
        for (key, frame) in barcode_microqr::frames::FRAMES.iter() {
            if key[0] != 1 || key[1] != 1 || key[3] != 1 { continue; }
            let n = frame.data.len();
            for (ki, k) in frame.ks.iter().enumerate() {
                let r: Vec<u8> = frame.data.iter().enumerate()
                    .map(|(i, &(x, y))| cells11[y as usize * 11 + x as usize] ^ k[i])
                    .collect();
                let fs = decoder::solve_gf2_pub(&frame.g, n, frame.cols, &r);
                println!("key {key:?} class {ki}: solve {:?}", fs.map(|f| f));
            }
        }
    }
}
