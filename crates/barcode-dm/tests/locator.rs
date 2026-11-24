//! L-finder detector tests: a synthetic L, then the real field fixture
//! (packages/web/test-codes/field/viva-dm-luma.json — ground truth in
//! docs/STATUS.md: 12x36 DMRE rotated 90° CCW, L corner at ~(462, 853),
//! kind BottomRight).

use barcode_dm::locator::{binarize, find_corners, CornerKind, Frame};

/// Minimal parser for the flat fixture {"w":N,"h":N,"luma":[...]}; no
/// serde dependency (crates stay dependency-free).
///
/// The field fixture is a real-world photo — PERSONAL DATA, never
/// committed. It lives locally next to the repo (gitignored); tests
/// that need it skip when it is absent so the committed suite passes
/// without it.
fn load_viva() -> Option<(usize, usize, Vec<u8>)> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/web/test-codes/field/viva-dm-luma.json"
    );
    let s = std::fs::read_to_string(path).ok()?;
    let dim = |key: &str| -> usize {
        let pat = format!("\"{}\":", key);
        let i = s.find(&pat).expect(key) + pat.len();
        let b = s.as_bytes();
        let mut j = i;
        while b[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut v = 0usize;
        while b[j].is_ascii_digit() {
            v = v * 10 + (b[j] - b'0') as usize;
            j += 1;
        }
        v
    };
    let (w, h) = (dim("w"), dim("h"));
    let li = s.find("\"luma\":").expect("luma") + "\"luma\":".len();
    let b = s.as_bytes();
    let mut j = li;
    while b[j] != b'[' {
        j += 1;
    }
    let mut luma = Vec::with_capacity(w * h);
    let mut v = 0usize;
    let mut in_num = false;
    while b[j] != b']' {
        match b[j] {
            b'0'..=b'9' => {
                v = v * 10 + (b[j] - b'0') as usize;
                in_num = true;
            }
            _ if in_num => {
                luma.push(v as u8);
                v = 0;
                in_num = false;
            }
            _ => {}
        }
        j += 1;
    }
    if in_num {
        luma.push(v as u8);
    }
    assert_eq!(luma.len(), w * h, "fixture luma size");
    Some((w, h, luma))
}

#[test]
fn finds_corner_on_synthetic_l() {
    // white 240x240, solid L: vertical stroke x[40,48) y[40,200),
    // horizontal x[40,140) y[192,200) -> corner BottomLeft at (40, 200)
    let (w, h) = (240usize, 240usize);
    let mut luma = vec![255u8; w * h];
    for y in 40..200 {
        for x in 40..48 {
            luma[y * w + x] = 0;
        }
    }
    for y in 192..200 {
        for x in 40..140 {
            luma[y * w + x] = 0;
        }
    }
    let frame = Frame { w, h, luma: &luma };
    let map = binarize(&frame);
    let corners = find_corners(&map);
    eprintln!("synthetic corners: {corners:?}");
    let hit = corners.iter().find(|c| {
        c.kind == CornerKind::BottomLeft && (c.x - 40.0).abs() <= 6.0 && (c.y - 200.0).abs() <= 6.0
    });
    assert!(
        hit.is_some(),
        "expected BottomLeft corner near (40,200), got {corners:?}"
    );
}

#[test]
fn finds_l_corner_on_viva_field_frame() {
    let Some((w, h, luma)) = load_viva() else {
        eprintln!("skipped: field fixture not present locally");
        return;
    };
    let frame = Frame { w, h, luma: &luma };
    let map = binarize(&frame);
    let corners = find_corners(&map);
    eprintln!("viva corners ({}):", corners.len());
    for c in &corners {
        eprintln!(
            "  ({:>5.1},{:>5.1}) {:?} h={} v={}",
            c.x, c.y, c.kind, c.hlen, c.vlen
        );
    }
    let hit = corners.iter().find(|c| {
        c.kind == CornerKind::BottomRight
            && (c.x - 462.0).abs() <= 10.0
            && (c.y - 853.0).abs() <= 10.0
    });
    let hit = hit.expect("expected BottomRight finder corner near (462,853)");
    // the horizontal stroke spans the symbol width: 12 modules * ~8.08px
    assert!(
        (90..=105).contains(&hit.hlen),
        "horizontal stroke {}px, expected ~97px (12 modules)",
        hit.hlen
    );
}

/// Embedded zint-generated canonical 12x36 matrix for b"DETECTOR-TEST-123"
/// (independent of the field frame; col 0 + row 11 are the finder L).
const TEST_MATRIX: [&str; 12] = [
    "#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.#.",
    "#.#.#.##....##..#######.####.#.#.#.#",
    "##.#...###.######.#.####...##..#.##.",
    "##.##..##.#.###.#########.##...#.###",
    "#.#..#...#..##....##..##.#..#.#.##..",
    "#.##...#.###.#...####..#..#.########",
    "##.######.#..##...#...#.....#.#...#.",
    "###.#.##......#.######....#...#..#.#",
    "##..#.#...#.##.##.##.####....##.##..",
    "#.........##..#####.#.#...#.###...##",
    "###..##.#..#####..###.####...#.#....",
    "####################################",
];
const TEST_PAYLOAD: &[u8] = b"DETECTOR-TEST-123";

/// Draw TEST_MATRIX into a white canvas in the given orientation: the L
/// corner lands at `corner`, axes per barcode_dm::locator::axes, pitch 8.
fn draw_symbol(kind: CornerKind, corner: (f32, f32), w: usize, h: usize) -> Vec<u8> {
    draw_symbol_pitch(kind, corner, w, h, 8.0)
}

fn draw_symbol_pitch(kind: CornerKind, corner: (f32, f32), w: usize, h: usize, p: f32) -> Vec<u8> {
    use barcode_dm::locator::axes;
    let (rd, cd, _) = axes(kind);
    let (rows, cols) = (TEST_MATRIX.len(), TEST_MATRIX[0].len());
    let mut luma = vec![255u8; w * h];
    for r in 0..rows {
        for c in 0..cols {
            if TEST_MATRIX[r].as_bytes()[c] != b'#' {
                continue;
            }
            let dr = r as f32 + 0.5 - rows as f32;
            let x = corner.0 + dr * p * rd.0 + (c as f32 + 0.5) * p * cd.0;
            let y = corner.1 + dr * p * rd.1 + (c as f32 + 0.5) * p * cd.1;
            let x0 = (x - p / 2.0).round() as isize;
            let x1 = (x + p / 2.0).round() as isize;
            let y0 = (y - p / 2.0).round() as isize;
            let y1 = (y + p / 2.0).round() as isize;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    if xx >= 0 && yy >= 0 && xx < w as isize && yy < h as isize {
                        luma[yy as usize * w + xx as usize] = 0;
                    }
                }
            }
        }
    }
    luma
}

/// Rotate a canvas about its centre (bilinear-free — nearest sample is
/// enough for a synthetic symbol), filling outside with white.
fn rotate_luma(w: usize, h: usize, src: &[u8], deg: f64) -> Vec<u8> {
    let rad = deg.to_radians();
    let (sin, cos) = (rad.sin(), rad.cos());
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let mut out = vec![255u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let sx = (cos * dx + sin * dy + cx).round() as isize;
            let sy = (-sin * dx + cos * dy + cy).round() as isize;
            if sx >= 0 && sy >= 0 && sx < w as isize && sy < h as isize {
                out[y * w + x] = src[sy as usize * w + sx as usize];
            }
        }
    }
    out
}

/// Regression for the 2026-11-18 invoice DMRE: a 12x36 symbol at the
/// smallest supported pitch (4px) tilted ~4° in the frame. The solid
/// finder border is then cut into per-scan-line segments that each shift
/// along the line; the old single-cursor bar tracker kept only one of
/// them (it reported a 146px leg as 93px), ranked the true corner behind
/// hundreds of text corners and never decoded. 0/2° passed before, so the
/// tilt is what this pins.
#[test]
fn locates_a_small_pitch_symbol_tilted_in_the_frame() {
    let (w, h) = (420usize, 600usize);
    for deg in [0.0, 2.0, 4.0, -4.0] {
        let body = draw_symbol_pitch(CornerKind::BottomRight, (360.0, 540.0), w, h, 4.0);
        let luma = rotate_luma(w, h, &body, deg);
        let frame = Frame { w, h, luma: &luma };
        let locs = barcode_dm::locator::locate(&frame);
        let hit = locs.iter().find(|l| {
            l.rows == 12 && l.cols == 36 && l.decoded.payload.as_deref() == Some(TEST_PAYLOAD)
        });
        assert!(hit.is_some(), "{deg}° tilted 12x36 at pitch 4 not located");
    }
}

#[test]
fn locates_synthetic_symbol_in_all_four_orientations() {
    // upright 12x36 is 96px wide x 288px tall; corners chosen so the body
    // fits the canvas for each orientation (BL body extends up+right,
    // BR up+left, TR down+left, TL down+right)
    for (kind, corner) in [
        (CornerKind::BottomLeft, (60.0, 350.0)),
        (CornerKind::BottomRight, (350.0, 350.0)),
        (CornerKind::TopRight, (350.0, 60.0)),
        (CornerKind::TopLeft, (60.0, 60.0)),
    ] {
        let (w, h) = (420usize, 420usize);
        let luma = draw_symbol(kind, corner, w, h);
        let frame = Frame { w, h, luma: &luma };
        let locs = barcode_dm::locator::locate(&frame);
        eprintln!("{kind:?} -> {:?} locations", locs.len());
        for l in &locs {
            eprintln!(
                "   {}x{} pitch {} payload {:?}",
                l.rows, l.cols, l.pitch, l.decoded.payload
            );
        }
        let hit = locs.iter().find(|l| {
            l.rows == 12 && l.cols == 36 && l.decoded.payload.as_deref() == Some(TEST_PAYLOAD)
        });
        assert!(hit.is_some(), "{kind:?}: synthetic symbol not located");
        assert_eq!(
            hit.unwrap().kind,
            kind,
            "{kind:?}: wrong corner kind reported"
        );
    }
}

#[test]
fn locates_viva_field_symbol() {
    // Personal-data fixture: present only locally (gitignored). Skip the
    // test silently in its absence — the committed suite must stay green.
    let Some((w, h, luma)) = load_viva() else {
        eprintln!("skipped: field fixture not present locally");
        return;
    };
    let frame = Frame { w, h, luma: &luma };
    let locs = barcode_dm::locator::locate(&frame);
    eprintln!("viva locations: {}", locs.len());
    for l in &locs {
        eprintln!(
            "  {}x{} {:?} pitch {:.2} angle {:.2} payload {:?}",
            l.rows, l.cols, l.kind, l.pitch, l.angle, l.decoded.payload
        );
    }
    // The expected payload lives in a gitignored sidecar next to the
    // fixture; without it, assert the geometry only.
    let expected = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/web/test-codes/field/viva-dm-expected.txt"
    ))
    .map(|v| {
        v.into_iter()
            .filter(|b| !b.is_ascii_whitespace())
            .collect::<Vec<u8>>()
    })
    .ok();
    let hit = locs
        .iter()
        .find(|l| match &expected {
            Some(exp) => l.decoded.payload.as_deref() == Some(exp.as_slice()),
            None => l.rows == 12 && l.cols == 36,
        })
        .expect("viva symbol not located");
    assert_eq!((hit.rows, hit.cols), (12, 36));
    if expected.is_some() {
        assert_eq!(hit.kind, CornerKind::BottomRight);
    }
}

/// Regression: the bar-cluster scan indexed non-square maps with swapped
/// dimensions — out-of-bounds on landscape frames, transposed garbage on
/// portrait ones. Every fixture was square, so it hid until a real
/// camera aspect hit it. A non-square map must scan without panicking.
#[test]
fn collect_bars_survives_nonsquare_maps() {
    use barcode_dm::locator::{locate_binary, BinaryMap};
    // 216x52 of noise-ish stripes: exercise both scan directions OOB edge.
    let (w, h) = (216usize, 52usize);
    let mut dark = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            dark[y * w + x] = (x / 3 + y / 5) % 4 == 0;
        }
    }
    let map = BinaryMap {
        w,
        h,
        dark: dark.clone(),
    };
    let _ = locate_binary(&map); // must not panic; hits unlikely and unasserted
                                 // And a portrait one.
    let map_t = BinaryMap {
        w: h,
        h: w,
        dark: dark.iter().map(|&d| !d).collect(),
    };
    let _ = locate_binary(&map_t);
}

/// A crease or print defect can cut the solid finder leg into fragments
/// that each fall under `MIN_RUN`/`MIN_STROKE`. The real GS1 DataMatrix
/// this came from (18x18 at ~5px/module) reported 6..29px pieces of an
/// 88px leg, so no finder-L corner formed and the symbol never decoded.
/// The sub-module run bridge and the collinear merge must reassemble it.
#[test]
fn reassembles_a_creased_finder_leg() {
    use barcode_dm::encode::encode;
    use barcode_dm::locator::locate_binary;

    let payload = b"HELLO";
    let enc = encode(payload).expect("encode");
    let (spec, matrix) = (enc.spec, enc.matrix);
    let (rows, cols) = (spec.rows as usize, spec.cols as usize);
    let pitch = 5usize;
    let (w, h) = (cols * pitch + 40, rows * pitch + 40);
    let mut luma = vec![255u8; w * h];
    for r in 0..rows {
        for c in 0..cols {
            if matrix[r * cols + c] == 0 {
                continue;
            }
            for y in 0..pitch {
                for x in 0..pitch {
                    luma[(20 + r * pitch + y) * w + 20 + c * pitch + x] = 0;
                }
            }
        }
    }
    // The standard finder L is the left column + bottom row. Cut a 2px
    // slit into the bottom leg, one module in from its corner.
    for y in (20 + (rows - 1) * pitch + 1)..(20 + (rows - 1) * pitch + 3) {
        for x in (20 + 2 * pitch)..(20 + 7 * pitch) {
            luma[y * w + x] = 255;
        }
    }
    let frame = Frame { w, h, luma: &luma };
    let map = binarize(&frame);
    let hits = locate_binary(&map);
    assert!(
        hits.iter()
            .any(|l| l.decoded.payload.as_deref() == Some(payload)),
        "no decode from {w}x{h} creased symbol"
    );
}
