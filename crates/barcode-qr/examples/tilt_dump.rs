use barcode_core::BitImage;
fn main() {
    let src_canvas = barcode_fixtures::encode_qr_text("TILT CHECK", 0, 2, 8).expect("fits");
    let side = src_canvas.width;
    let theta = 30.0f32.to_radians();
    let (c, s) = (theta.cos(), theta.sin());
    let dim = (side as f32 * 1.5) as u32;
    let mut dest = barcode_fixtures::Canvas::new(dim, dim);
    let sc = side as f32 / 2.0;
    let dc = dim as f32 / 2.0;
    for y in 0..dim {
        for x in 0..dim {
            let (dx, dy) = (x as f32 - dc, y as f32 - dc);
            let sx = c * dx + s * dy + sc;
            let sy = -s * dx + c * dy + sc;
            if sx >= 0.0 && sy >= 0.0 && sx < side as f32 && sy < side as f32 {
                if src_canvas.bits()[sy as usize * src_canvas.bytes_per_row as usize + (sx as usize >> 3)]
                    >> (sx as usize & 7)
                    & 1
                    == 1
                {
                    dest.fill(x, y, 1, 1);
                }
            }
        }
    }
    let img = BitImage::new(dest.width, dest.height, dest.bits());
    let mut d = barcode_qr::QrDecoder::new();
    // the failing cluster: (122,122) m_est 9.24, dir (-0.41, 0.91)
    // the true TL finder: (178,122), m_est 9.24, local Y axis at -30deg
    let (fcx, fcy) = (178.0f64, 122.0f64);
    let theta = -30.0f64.to_radians();
    let (ax, ay) = (theta.sin(), theta.cos());
    let m_est = 9.24f64;
    println!("TL finder rays (local Y = {ax:.2},{ay:.2}):");
    for (label, dx, dy) in [("Y+", ax, ay), ("Y-", -ax, -ay), ("X+", ay, -ax), ("X-", -ay, ax)] {
        let mut line = format!("  {label}: ");
        let step = 0.25 * m_est;
        let mut r = 0.5 * m_est;
        while r <= 4.5 * m_est {
            let px = (fcx + r * dx).round() as i64;
            let py = (fcy + r * dy).round() as i64;
            let ink = px >= 0 && py >= 0 && (px as u32) < img.width && (py as u32) < img.height && img.ink(px as u32, py as u32);
            line.push(if ink { '#' } else { '.' });
            r += step;
        }
        println!("{line}");
    }
}
