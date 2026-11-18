"""Generate crates/barcode-wasm/tests/pdf417_tier.rs: a padded canvas
fixture scanned through the full wasm tier pipeline. The canvas is
validated with the Python detector before freezing.
"""

import sys

sys.path.insert(0, "/Users/ag/git/barcode/tools")
import pdf417_detect_probe as D  # noqa: E402

OUT = "/Users/ag/git/barcode/crates/barcode-wasm/tests/pdf417_tier.rs"
PAYLOAD = "A1"
LEVEL = 0
UP, QZ = 2, 8

NL = chr(92) + "n"  # the two characters \ n, as they must appear in Rust source


def main():
    m = D.render(PAYLOAD, level=LEVEL)
    canvas = D.paste(m, qz=QZ, noise_px=0, upscale=UP)
    got = D.detect(canvas)
    assert got == PAYLOAD.encode(), got

    rows = ["".join("#" if v == 1 else "." for v in row) for row in m]
    matrix_lit = NL.join(rows)

    test = f'''//! GENERATED wasm-tier fixture test (tools/pdf417_gen_fixture.py):
//! a padded PDF417 canvas scanned through the full tier pipeline.

use barcode_wasm::decode_pdf417_mask;

const MATRIX: &str = "{matrix_lit}";
const MW: usize = {len(m[0])};
const MH: usize = {len(m)};
const UP: usize = {UP};
const QZ: usize = {QZ};
const PAYLOAD: &str = "{PAYLOAD}";

#[test]
fn pdf417_tier_decodes_padded_canvas() {{
    let cw = MW * UP + 2 * QZ;
    let ch = MH * UP + 2 * QZ;
    let src: Vec<u8> = MATRIX.split("{NL}")
        .flat_map(|l| l.chars().map(|c| if c == '#' {{ 1u8 }} else {{ 0 }}))
        .collect();
    assert_eq!(src.len(), MW * MH);
    let mut canvas = vec![0u8; cw * ch];
    for my in 0..MH {{
        for mx in 0..MW {{
            let v = src[my * MW + mx];
            for dy in 0..UP {{
                for dx in 0..UP {{
                    canvas[(QZ + my * UP + dy) * cw + QZ + mx * UP + dx] = v;
                }}
            }}
        }}
    }}
    // pack to 1bpp (BitImage layout: bytes_per_row = ceil(w/8) padded to 4)
    let bpr = ((cw + 31) / 32 * 4) as usize;
    let mut packed = vec![0u8; bpr * ch];
    for y in 0..ch {{
        for x in 0..cw {{
            if canvas[y * cw + x] != 0 {{
                packed[y * bpr + x / 8] |= 1 << (x % 8);
            }}
        }}
    }}
    let img = barcode_core::BitImage::new(cw as u32, ch as u32, &packed);
    let hits = decode_pdf417_mask(&img);
    assert_eq!(hits.len(), 1, "one hit");
    assert_eq!(hits[0].symbology, barcode_core::Symbology::PDF417);
    assert_eq!(hits[0].text, PAYLOAD);
    let xs = [hits[0].position[0].x, hits[0].position[1].x,
              hits[0].position[2].x, hits[0].position[3].x];
    let ys = [hits[0].position[0].y, hits[0].position[1].y,
              hits[0].position[2].y, hits[0].position[3].y];
    assert!(xs.iter().cloned().fold(f32::MIN, f32::max) <= cw as f32);
    assert!(ys.iter().cloned().fold(f32::MIN, f32::max) <= ch as f32);
}}
'''
    open(OUT, "w").write(test)
    print("wrote", OUT)


if __name__ == "__main__":
    main()
