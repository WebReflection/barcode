//! Stream-feasibility batch: every grab frame through the DM tier,
//! reporting decode time (the 500ms stream law needs <500ms per frame).

use barcode_core::BitImage;

#[test]
fn grabbits_batch() {
    let dir = match std::env::var("GRABBITS") {
        Ok(d) => d,
        Err(_) => return, // skip when the local dataset is absent
    };
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map_or(false, |e| e == "bits"))
        .collect();
    names.sort();
    let mut feasible = 0;
    let mut fast = 0;
    for path in &names {
        let raw = std::fs::read(path).unwrap();
        let w = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        let h = u32::from_le_bytes(raw[4..8].try_into().unwrap());
        let img = BitImage::new(w, h, Box::leak(raw[16..].to_vec().into_boxed_slice()));
        let t = std::time::Instant::now();
        let hits = barcode_wasm::decode_datamatrix_mask_stream(&img, true);
        let ms = t.elapsed().as_millis();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !hits.is_empty() {
            if ms < 100 {
                fast += 1;
            }
            if ms < 500 {
                feasible += 1;
            }
        }
        eprintln!(
            "{} {} hits in {}ms{}",
            name,
            hits.len(),
            ms,
            if hits.is_empty() {
                "  MISS"
            } else if ms < 100 {
                "  FAST"
            } else if ms < 500 {
                "  ok"
            } else {
                "  SLOW"
            }
        );
    }
    eprintln!(
        "== {}/{} decoded, {}/{} fast (<100ms), {}/{} stream-feasible (<500ms)",
        names.len() - names.iter().filter(|p| std::fs::read(p).is_err()).count(),
        0,
        fast,
        names.len(),
        feasible,
        names.len()
    );
}
