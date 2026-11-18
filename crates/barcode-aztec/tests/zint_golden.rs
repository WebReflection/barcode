//! Frozen zint-oracle matrices: full pipeline matrix -> corrected data
//! codewords. Generation recipe: `tools/aztec_probe.encode_matrix` (bit 1
//! = dark), expectations cross-checked by RS arbitration in the probe
//! session of 2026-10-17 (docs/STATUS.md aztec blocks).

use barcode_aztec::tables::spec_for;

/// ('#' = dark, matrix rows, spec size, expected layers, expected D,
///  expected corrected data codewords, expected text bytes)
const GOLDEN: &[(&str, &[&str], (u16, bool), u8, u16, &[u16], &[u8])] = &[
    (
        // 'B' vers 2: clean symbol, no corrections needed.
        "B v2",
        &[
            "..#.#.##.##..##..#.",
            ".#.#..##..###..#.#.",
            "##.#..#.#.#.#..###.",
            "..#..####.##.####.#",
            "..####.#......#...#",
            "#...############..#",
            "#....#.......#.###.",
            "######.#####.#.###.",
            "###..#.#...#.#.##.#",
            "#...##.#.#.#.#.##..",
            "##..##.#...#.###.#.",
            "#.#.##.#####.#.#..#",
            "...#.#.......####..",
            ".##..##########....",
            "##.....#...#....#..",
            ".#...#..##..####...",
            "#.######..#.###.#.#",
            "#.##...#####..#...#",
            "..####..#.#.##..##.",
        ],
        (19, true),
        2,
        1,
        &[7],
        b"B",
    ),
    (
        // '4' vers 2: zint zeroed the first codeword's leading bits;
        // candidate A (raw + RS) must correct them.
        "4 v2",
        &[
            "...#.###..###.#.##.",
            "...###.##.##.#.###.",
            "....#....##.#####.#",
            "###..#.##..######.#",
            ".#..##.#......##...",
            "###.###########.#.#",
            "..#.##.......##..##",
            "..#.##.#####.##...#",
            "####.#.#...#.#....#",
            ".#.###.#.#.#.###.##",
            ".##.##.#...#.#..#.#",
            ".#...#.#####.#..###",
            "..##.#.......#..#..",
            "#.##.##########.#..",
            "##.....###.#...#.##",
            "#.#....##.#.....###",
            "..###..##..#..###..",
            ".######..#......###",
            ".#..##.#.#.##..####",
        ],
        (19, true),
        2,
        2,
        &[60, 55],
        b"4",
    ),
    (
        // 'Code 128!' vers 2: contains a stuffed-looking window; the
        // strict parse would misalign, raw+RS is exact.
        "Code 128! v2",
        &[
            "...#..#.###.#.#####",
            "#...#.##..##.####.#",
            ".#....##.........##",
            "##....##.#...####.#",
            "..####.#..#...####.",
            "#.##############...",
            "..#.##.......##....",
            "..####.#####.#####.",
            ".#.#.#.#...#.##.#..",
            "#.#..#.#.#.#.#...##",
            "#.#..#.#...#.##....",
            ".#.###.#####.#....#",
            "#..#.#.......####..",
            "###..##########...#",
            "##.#....#..##....##",
            "...#..##.#.##...###",
            "..#..#.###..##.#.#.",
            ".#..##...#####.....",
            "#.##......#.#....#.",
        ],
        (19, true),
        2,
        10,
        &[9, 50, 1, 41, 47, 2, 26, 20, 1, 55],
        b"Code 128!",
    ),
    (
        // 'hello world' vers 1: 2 skip bits shift every raw window past
        // the correction budget; only the stuff-aware parse (candidate B)
        // decodes it.
        "hello world v1",
        &[
            "..##...##....#.",
            "##.##.##..#..##",
            "#.##....#...##.",
            "..###########.#",
            "#.##.......##..",
            ".#.#.#####.####",
            "...#.#...#.##..",
            "####.#.#.#.#..#",
            "..##.#...#.#...",
            "##.#.#####.#...",
            ".#.#.......#.#.",
            ".#.###########.",
            "#....##...#...#",
            "#....#....#.#.#",
            "#....##.#...##.",
        ],
        (15, true),
        1,
        10,
        &[56, 36, 51, 22, 48, 3, 34, 4, 54, 37],
        b"hello world",
    ),
    (
        // 'x\x7f' vers 1: skip + a 111110 final codeword; candidate A
        // corrects it (4 symbol errors, t = 6).
        "x\\x7f v1",
        &[
            "..##..#..###.##",
            "#####.#..#.####",
            "#.##......#.#..",
            ".#############.",
            "#.##.......##..",
            ".###.#####.###.",
            "##.#.#...#.##.#",
            "#..#.#.#.#.##.#",
            "##.#.#...#.#..#",
            "#.##.#####.###.",
            "####.......####",
            "##.###########.",
            "#.....#.#....##",
            "#.####..#.###..",
            "#....#.#.#.##.#",
        ],
        (15, true),
        1,
        4,
        &[57, 39, 46, 62],
        b"x\x7f",
    ),
];

fn to_matrix(rows: &[&str]) -> Vec<u8> {
    rows.iter()
        .flat_map(|r| r.bytes().map(|b| (b == b'#') as u8))
        .collect()
}

#[test]
fn zint_matrices_decode_to_expected_data() {
    for (name, rows, (size, compact), layers, d, want, want_text) in GOLDEN {
        let spec = spec_for(*size, *size, *compact).expect(name);
        let m = to_matrix(rows);
        let s = barcode_aztec::read_stream(&m, spec).unwrap_or_else(|| panic!("{name}: no decode"));
        assert_eq!(s.layers, *layers, "{name}");
        assert_eq!(s.data_cw, *d, "{name}");
        assert_eq!(s.data, *want, "{name}");
        assert_eq!(&barcode_aztec::text::decode_text(&s), want_text, "{name}");
    }
}

#[test]
fn dark_inverted_matrix_is_rejected() {
    // Polarity trap: with 1 = white the mode RS cannot validate.
    let (_, rows, (size, compact), _, _, _, _) = GOLDEN[0];
    let m: Vec<u8> = rows
        .iter()
        .flat_map(|r| r.bytes().map(|b| (b != b'#') as u8))
        .collect();
    let spec = spec_for(size, size, compact).unwrap();
    assert_eq!(barcode_aztec::read_stream(&m, spec), None);
}
