use barcode_core::BitImage;
fn main() {
    let version = 2;
    let n = (17 + 4 * version) as f64;
    let data: Vec<u8> = (0..34u8).map(|i| b'A' + i).collect();
    let canvas = barcode_fixtures::encode_qr(version, 1, 3, &data, 4);
    let quad = [(140.0, 60.0), (560.0, 130.0), (620.0, 600.0), (50.0, 560.0)];
    let corners = [(0.0, 0.0), (n, 0.0), (n, n), (0.0, n)];
    let pairs: Vec<(f64, f64, f64, f64)> = corners.iter().zip(quad.iter())
        .map(|(&(u, v), &(x, y))| (u, v, x, y)).collect();
    let h = barcode_core::geometry::fit_homography(&pairs).unwrap();
    // same warp as the test
    let m = 4.0f64; let quiet = 6.0 * m;
    let inv = inv3(h);
    let (a, b, c, d, e, f, g, hh, ii) = (inv[0]*m, inv[1]*m, inv[2]*m + quiet, inv[3]*m, inv[4]*m, inv[5]*m + quiet, inv[6], inv[7], inv[8]);
    let hi = [a, b, c, d, e, f, g, hh, ii];
    let out = 700u32; let bpr = ((out + 31) / 32 * 4) as usize;
    let mut data1 = vec![0u8; bpr * out as usize];
    for y in 0..out { for x in 0..out {
        let (sx, sy) = barcode_core::geometry::apply_h(&hi, x as f64, y as f64);
        let (sxi, syi) = (sx.round() as i64, sy.round() as i64);
        let ink = sxi >= 0 && syi >= 0 && sxi < canvas.width as i64 && syi < canvas.height as i64
            && canvas.bits()[syi as usize * canvas.bytes_per_row as usize + (sxi as usize >> 3)] >> (sxi as usize & 7) & 1 == 1;
        if ink { data1[y as usize * bpr + (x as usize >> 3)] |= 1 << (x & 7); }
    }}
    std::fs::write("/tmp/persp.bin", &data1).unwrap();
    println!("w {} bpr {} saved", out, bpr);
    let img = BitImage::new(out, out, &data1);
    let mut d = barcode_qr::QrDecoder::new();
    println!("finders: {:?}", d.find_finders(&img).map(|f| f.iter().map(|q| (q.x, q.y, q.module)).collect::<Vec<_>>()));
}
fn inv3(mm: [f64; 9]) -> [f64; 9] {
    let (a, b, c, d, e, f, g, hh, i) = (mm[0], mm[1], mm[2], mm[3], mm[4], mm[5], mm[6], mm[7], mm[8]);
    let det = a * (e * i - f * hh) - b * (d * i - f * g) + c * (d * hh - e * g);
    [(e*i-f*hh)/det, (c*hh-b*i)/det, (b*f-c*e)/det, (f*g-d*i)/det, (a*i-c*g)/det, (c*d-a*f)/det, (d*hh-e*g)/det, (b*g-a*hh)/det, (a*e-b*d)/det]
}

// 30-degree fixture ray debug, appended as a second binary via args
