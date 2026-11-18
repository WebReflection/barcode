//! Milestone 3: codeword extraction (ISO/IEC 18004 §8.7-§8.9).
//!
//! Data-encodable modules are visited in a zigzag: two-module-wide columns
//! from the bottom-right edge moving left, skipping the vertical timing
//! column, alternating upward/downward. Each visited pair is read
//! right-module-first, and the stream is the codewords' bits MSB-first
//! followed by version-dependent remainder bits (7 for v2-v6, 0 for v1).
//! Every visited module is XOR-masked with the format-declared mask pattern.
//!
//! The recovered codeword sequence is de-interleaved into Reed-Solomon
//! blocks (smaller group first), each block is error-corrected, and the
//! data codewords are returned in block order — the input to the segment
//! decoder (milestone 4).

use barcode_core::BitImage;

use crate::grid::{copy2, EcLevel, Grid, COPY1};
use crate::rs::{rs_correct_erasures, Gf};

/// One (version, EC level) block structure: EC codewords per block (uniform
/// across groups) and (block count, data codewords per block) per group.
pub struct BlockPlan {
    ec_per_block: u32,
    /// (count, data per block); the second group may be empty.
    groups: [(u32, u32); 2],
}

impl BlockPlan {
    /// Data codeword capacity for this (version, level).
    fn data_codewords(&self) -> u32 {
        self.groups.iter().map(|&(c, s)| c * s).sum()
    }

    fn total_codewords(&self) -> u32 {
        let data: u32 = self.groups.iter().map(|&(c, s)| c * s).sum();
        data + self.ec_per_block * (self.groups[0].0 + self.groups[1].0)
    }
}

/// Block structure tables for versions 1-40, transcribed from the
/// ISO/IEC 18004 tables, indexed [version-1][level] with levels in
/// raw-field order (M, L, H, Q); each row:
/// [ec_per_block, g1_count, g1_data, g2_count, g2_data].
#[rustfmt::skip]
const BLOCKS: [[[u32; 5]; 4]; 40] = [
  /* v1 */ [[10, 1, 16, 0, 0], [7, 1, 19, 0, 0], [17, 1, 9, 0, 0], [13, 1, 13, 0, 0]],
  /* v2 */ [[16, 1, 28, 0, 0], [10, 1, 34, 0, 0], [28, 1, 16, 0, 0], [22, 1, 22, 0, 0]],
  /* v3 */ [[26, 1, 44, 0, 0], [15, 1, 55, 0, 0], [22, 2, 13, 0, 0], [18, 2, 17, 0, 0]],
  /* v4 */ [[18, 2, 32, 0, 0], [20, 1, 80, 0, 0], [16, 4, 9, 0, 0], [26, 2, 24, 0, 0]],
  /* v5 */ [[24, 2, 43, 0, 0], [26, 1, 108, 0, 0], [22, 2, 11, 2, 12], [18, 2, 15, 2, 16]],
  /* v6 */ [[16, 4, 27, 0, 0], [18, 2, 68, 0, 0], [28, 4, 15, 0, 0], [24, 4, 19, 0, 0]],
  /* v7 */ [[18, 4, 31, 0, 0], [20, 2, 78, 0, 0], [26, 4, 13, 1, 14], [18, 2, 14, 4, 15]],
  /* v8 */ [[22, 2, 38, 2, 39], [24, 2, 97, 0, 0], [26, 4, 14, 2, 15], [22, 4, 18, 2, 19]],
  /* v9 */ [[22, 3, 36, 2, 37], [30, 2, 116, 0, 0], [24, 4, 12, 4, 13], [20, 4, 16, 4, 17]],
  /* v10 */ [[26, 4, 43, 1, 44], [18, 2, 68, 2, 69], [28, 6, 15, 2, 16], [24, 6, 19, 2, 20]],
  /* v11 */ [[30, 1, 50, 4, 51], [20, 4, 81, 0, 0], [24, 3, 12, 8, 13], [28, 4, 22, 4, 23]],
  /* v12 */ [[22, 6, 36, 2, 37], [24, 2, 92, 2, 93], [28, 7, 14, 4, 15], [26, 4, 20, 6, 21]],
  /* v13 */ [[22, 8, 37, 1, 38], [26, 4, 107, 0, 0], [22, 12, 11, 4, 12], [24, 8, 20, 4, 21]],
  /* v14 */ [[24, 4, 40, 5, 41], [30, 3, 115, 1, 116], [24, 11, 12, 5, 13], [20, 11, 16, 5, 17]],
  /* v15 */ [[24, 5, 41, 5, 42], [22, 5, 87, 1, 88], [24, 11, 12, 7, 13], [30, 5, 24, 7, 25]],
  /* v16 */ [[28, 7, 45, 3, 46], [24, 5, 98, 1, 99], [30, 3, 15, 13, 16], [24, 15, 19, 2, 20]],
  /* v17 */ [[28, 10, 46, 1, 47], [28, 1, 107, 5, 108], [28, 2, 14, 17, 15], [28, 1, 22, 15, 23]],
  /* v18 */ [[26, 9, 43, 4, 44], [30, 5, 120, 1, 121], [28, 2, 14, 19, 15], [28, 17, 22, 1, 23]],
  /* v19 */ [[26, 3, 44, 11, 45], [28, 3, 113, 4, 114], [26, 9, 13, 16, 14], [26, 17, 21, 4, 22]],
  /* v20 */ [[26, 3, 41, 13, 42], [28, 3, 107, 5, 108], [28, 15, 15, 10, 16], [30, 15, 24, 5, 25]],
  /* v21 */ [[26, 17, 42, 0, 0], [28, 4, 116, 4, 117], [30, 19, 16, 6, 17], [28, 17, 22, 6, 23]],
  /* v22 */ [[28, 17, 46, 0, 0], [28, 2, 111, 7, 112], [24, 34, 13, 0, 0], [30, 7, 24, 16, 25]],
  /* v23 */ [[28, 4, 47, 14, 48], [30, 4, 121, 5, 122], [30, 16, 15, 14, 16], [30, 11, 24, 14, 25]],
  /* v24 */ [[28, 6, 45, 14, 46], [30, 6, 117, 4, 118], [30, 30, 16, 2, 17], [30, 11, 24, 16, 25]],
  /* v25 */ [[28, 8, 47, 13, 48], [26, 8, 106, 4, 107], [30, 22, 15, 13, 16], [30, 7, 24, 22, 25]],
  /* v26 */ [[28, 19, 46, 4, 47], [28, 10, 114, 2, 115], [30, 33, 16, 4, 17], [28, 28, 22, 6, 23]],
  /* v27 */ [[28, 22, 45, 3, 46], [30, 8, 122, 4, 123], [30, 12, 15, 28, 16], [30, 8, 23, 26, 24]],
  /* v28 */ [[28, 3, 45, 23, 46], [30, 3, 117, 10, 118], [30, 11, 15, 31, 16], [30, 4, 24, 31, 25]],
  /* v29 */ [[28, 21, 45, 7, 46], [30, 7, 116, 7, 117], [30, 19, 15, 26, 16], [30, 1, 23, 37, 24]],
  /* v30 */ [[28, 19, 47, 10, 48], [30, 5, 115, 10, 116], [30, 23, 15, 25, 16], [30, 15, 24, 25, 25]],
  /* v31 */ [[28, 2, 46, 29, 47], [30, 13, 115, 3, 116], [30, 23, 15, 28, 16], [30, 42, 24, 1, 25]],
  /* v32 */ [[28, 10, 46, 23, 47], [30, 17, 115, 0, 0], [30, 19, 15, 35, 16], [30, 10, 24, 35, 25]],
  /* v33 */ [[28, 14, 46, 21, 47], [30, 17, 115, 1, 116], [30, 11, 15, 46, 16], [30, 29, 24, 19, 25]],
  /* v34 */ [[28, 14, 46, 23, 47], [30, 13, 115, 6, 116], [30, 59, 16, 1, 17], [30, 44, 24, 7, 25]],
  /* v35 */ [[28, 12, 47, 26, 48], [30, 12, 121, 7, 122], [30, 22, 15, 41, 16], [30, 39, 24, 14, 25]],
  /* v36 */ [[28, 6, 47, 34, 48], [30, 6, 121, 14, 122], [30, 2, 15, 64, 16], [30, 46, 24, 10, 25]],
  /* v37 */ [[28, 29, 46, 14, 47], [30, 17, 122, 4, 123], [30, 24, 15, 46, 16], [30, 49, 24, 10, 25]],
  /* v38 */ [[28, 13, 46, 32, 47], [30, 4, 122, 18, 123], [30, 42, 15, 32, 16], [30, 48, 24, 14, 25]],
  /* v39 */ [[28, 40, 47, 7, 48], [30, 20, 117, 4, 118], [30, 10, 15, 67, 16], [30, 43, 24, 22, 25]],
  /* v40 */ [[28, 18, 47, 31, 48], [30, 19, 118, 6, 119], [30, 20, 15, 61, 16], [30, 34, 24, 34, 25]],
];

/// Alignment pattern center coordinates per version — used as BOTH row and
/// column coordinates; the three finder-corner combinations are skipped
/// (ISO 18004 annex E). Version 1 has none.
#[rustfmt::skip]
const ALIGNMENT: [&[u32]; 41] = [
  &[],
  &[],
  &[6, 18],
  &[6, 22],
  &[6, 26],
  &[6, 30],
  &[6, 34],
  &[6, 22, 38],
  &[6, 24, 42],
  &[6, 26, 46],
  &[6, 28, 50],
  &[6, 30, 54],
  &[6, 32, 58],
  &[6, 34, 62],
  &[6, 26, 46, 66],
  &[6, 26, 48, 70],
  &[6, 26, 50, 74],
  &[6, 30, 54, 78],
  &[6, 30, 56, 82],
  &[6, 30, 58, 86],
  &[6, 34, 62, 90],
  &[6, 28, 50, 72, 94],
  &[6, 26, 50, 74, 98],
  &[6, 30, 54, 78, 102],
  &[6, 28, 54, 80, 106],
  &[6, 32, 58, 84, 110],
  &[6, 30, 58, 86, 114],
  &[6, 34, 62, 90, 118],
  &[6, 26, 50, 74, 98, 122],
  &[6, 30, 54, 78, 102, 126],
  &[6, 26, 52, 78, 104, 130],
  &[6, 30, 56, 82, 108, 134],
  &[6, 34, 60, 86, 112, 138],
  &[6, 30, 58, 86, 114, 142],
  &[6, 34, 62, 90, 118, 146],
  &[6, 30, 54, 78, 102, 126, 150],
  &[6, 24, 50, 76, 102, 128, 154],
  &[6, 28, 54, 80, 106, 132, 158],
  &[6, 32, 58, 84, 110, 136, 162],
  &[6, 26, 54, 82, 110, 138, 166],
  &[6, 30, 58, 86, 114, 142, 170],
];

/// Data-codeword capacity for a (version, EC level).
pub fn data_capacity(version: u32, ec: EcLevel) -> Option<u32> {
    block_plan(version, ec).map(|p| p.data_codewords())
}

pub fn block_plan(version: u32, ec: EcLevel) -> Option<BlockPlan> {
    if !(1..=40).contains(&version) {
        return None;
    }
    // Raw 2-bit field order (M=0, L=1, H=2, Q=3) — see grid.rs.
    let ec_index = match ec {
        EcLevel::M => 0usize,
        EcLevel::L => 1,
        EcLevel::H => 2,
        EcLevel::Q => 3,
    };
    let row = &BLOCKS[(version - 1) as usize][ec_index];
    Some(BlockPlan {
        ec_per_block: row[0],
        groups: [(row[1], row[2]), (row[3], row[4])],
    })
}

/// The mask predicate (ISO 18004 §8.8.2): module (row, column) is toggled
/// when the pattern's condition holds.
fn mask_toggles(mask: u8, row: u32, col: u32) -> bool {
    match mask {
        0 => (row + col) % 2 == 0,
        1 => row % 2 == 0,
        2 => col % 3 == 0,
        3 => (row + col) % 3 == 0,
        4 => (row / 2 + col / 3) % 2 == 0,
        5 => (row * col) % 2 + (row * col) % 3 == 0,
        6 => ((row * col) % 2 + (row * col) % 3) % 2 == 0,
        7 => ((row + col) % 2 + (row * col) % 3) % 2 == 0,
        _ => false,
    }
}

/// Function-pattern occupancy: true where a module is NOT data-encodable
/// (finders + separators, timing, alignment, format info, dark module).
fn function_modules(n: u32) -> Vec<Vec<bool>> {
    let mut occ = vec![vec![false; n as usize]; n as usize];
    // Finder + separator regions: 8x8 at three corners.
    for &(fr, fc) in &[(0u32, 0u32), (0, n - 8), (n - 8, 0)] {
        for r in 0..8u32 {
            for c in 0..8u32 {
                occ[(fr + r) as usize][(fc + c) as usize] = true;
            }
        }
    }
    // Timing patterns: row 6 and column 6.
    for i in 0..n {
        occ[6][i as usize] = true;
        occ[i as usize][6] = true;
    }
    // Alignment patterns: every (row, col) combination of the version's
    // center coordinates except the three finder corners (the 5x5 region
    // is function area — dark ring + center, light between).
    let version = (n - 17) / 4;
    let coords = ALIGNMENT[version as usize];
    if !coords.is_empty() {
        let (first, last) = (coords[0], coords[coords.len() - 1]);
        for &ar in coords {
            for &ac in coords {
                if (ar, ac) == (first, first)
                    || (ar, ac) == (first, last)
                    || (ar, ac) == (last, first)
                {
                    continue;
                }
                for r in ar - 2..=ar + 2 {
                    for c in ac - 2..=ac + 2 {
                        occ[r as usize][c as usize] = true;
                    }
                }
            }
        }
    }
    // Version information blocks (v7+): 6x3 beside the top-right finder and
    // 3x6 beside the bottom-left finder.
    if version >= 7 {
        for k in 0..18u32 {
            occ[(k / 3) as usize][(n - 11 + k % 3) as usize] = true;
            occ[(n - 11 + k % 3) as usize][(k / 3) as usize] = true;
        }
    }
    // Both format-info copies and the dark module. The format tables are
    // in (x, y) = (column, row) — same convention as the placement walk.
    for (x, y) in COPY1 {
        occ[y as usize][x as usize] = true;
    }
    for (x, y) in copy2(n) {
        occ[y as usize][x as usize] = true;
    }
    occ[8][(n - 8) as usize] = true;
    occ
}

/// Data-encodable module positions (mx, my) in codeword-stream order.
pub fn placement_positions(n: u32) -> Vec<(u32, u32)> {
    let occ = function_modules(n);
    let mut out = Vec::new();
    let mut upward = true;
    let mut right = n as i32 - 1; // right column of the current pair
    while right > 0 {
        if right == 6 {
            right -= 1; // skip the vertical timing column
        }
        for i in 0..n {
            let row = if upward { n - 1 - i } else { i };
            for &co in &[0, -1] {
                let col = (right + co) as u32;
                if !occ[row as usize][col as usize] {
                    out.push((col, row));
                }
            }
        }
        upward = !upward;
        right -= 2;
    }
    out
}

/// Extract the data codewords from a symbol: read the zigzag, unmask,
/// de-interleave the Reed–Solomon blocks and error-correct each one.
/// Returns the data codewords in block order (the segment decoder's input).
///
/// When the plain pass cannot correct a block, the grid is re-sampled
/// with quadrant votes and the modules whose samples disagreed become
/// **known erasures** (item 6d): one parity slot instead of an error's
/// two. The retry only pays when the plain pass reached RS (a wrong
/// grid/labeling normally dies earlier, at the version/format read).
pub fn read_codewords(img: &BitImage, grid: &Grid, ec: EcLevel, mask: u8) -> Option<Vec<u8>> {
    if let Some(out) = read_codewords_with_erasures(img, grid, ec, mask, &[]) {
        return Some(out);
    }
    let n = grid.size;
    let version = (n - 17) / 4;
    if !(1..=40).contains(&version) {
        return None;
    }
    let Some(plan) = block_plan(version, ec) else {
        return None;
    };
    if placement_positions(n).len() < (plan.total_codewords() * 8) as usize {
        return None;
    }
    let uncertain = grid.uncertainty(img);
    read_codewords_with_erasures(img, grid, ec, mask, &uncertain)
}

/// As [`read_codewords`] with an explicit module-uncertainty mask
/// (`grid.size²`, `my * size + mx`). An empty mask is the plain
/// error-only pass.
pub fn read_codewords_with_erasures(
    img: &BitImage,
    grid: &Grid,
    ec: EcLevel,
    mask: u8,
    uncertain: &[u8],
) -> Option<Vec<u8>> {
    let n = grid.size;
    let version = (n - 17) / 4;
    if !(1..=40).contains(&version) {
        return None;
    }
    let plan = block_plan(version, ec)?;
    let total = plan.total_codewords();
    let positions = placement_positions(n);
    if positions.len() < (total * 8) as usize {
        return None;
    }
    let unc = if uncertain.len() == (n * n) as usize {
        uncertain
    } else {
        &[]
    };

    // Read the bit stream, undoing the mask. Bits past the codewords are
    // remainder bits (constant zero when encoded) — skipped. An uncertain
    // module erases its whole codeword.
    let mut bytes = vec![0u8; total as usize];
    let mut erased_byte = vec![false; total as usize];
    for (k, &(mx, my)) in positions.iter().enumerate() {
        if k >= (total * 8) as usize {
            break;
        }
        let raw = grid.sample(img, mx as i32, my as i32);
        if raw ^ mask_toggles(mask, my, mx) {
            bytes[k / 8] |= 0x80 >> (k % 8);
        }
        if !unc.is_empty() && unc[(my * n + mx) as usize] != 0 {
            erased_byte[k / 8] = true;
        }
    }

    // De-interleave: data codewords round-robin over the blocks (group 1
    // blocks first), then EC codewords the same way. Erasure positions
    // are collected in the same walk, in each block's local order
    // (data ++ EC).
    let (n1, s1) = plan.groups[0];
    let (n2, s2) = plan.groups[1];
    let nblocks = (n1 + n2) as usize;
    let ecn = plan.ec_per_block as usize;
    let mut data_blocks: Vec<Vec<u8>> = (0..nblocks)
        .map(|b| vec![0u8; if b < n1 as usize { s1 } else { s2 } as usize])
        .collect();
    let mut ec_blocks = vec![vec![0u8; ecn]; nblocks];
    let mut erased_blocks: Vec<Vec<usize>> = vec![Vec::new(); nblocks];
    {
        let mut stream = 0usize;
        for j in 0..s1.max(s2) as usize {
            for b in 0..nblocks {
                if j < data_blocks[b].len() {
                    data_blocks[b][j] = bytes[stream];
                    if erased_byte[stream] {
                        erased_blocks[b].push(j);
                    }
                    stream += 1;
                }
            }
        }
        for j in 0..ecn {
            for b in 0..nblocks {
                ec_blocks[b][j] = bytes[stream];
                if erased_byte[stream] {
                    erased_blocks[b].push(data_blocks[b].len() + j);
                }
                stream += 1;
            }
        }
        debug_assert_eq!(stream, total as usize);
    }

    // Correct each block; concatenate data in block order.
    let gf = Gf::new();
    let mut out = Vec::with_capacity(total as usize);
    for b in 0..nblocks {
        let mut block = std::mem::take(&mut data_blocks[b]);
        block.extend_from_slice(&ec_blocks[b]);
        if erased_blocks[b].is_empty() {
            rs_correct_erasures(&gf, &mut block, ecn, &[])?;
        } else {
            rs_correct_erasures(&gf, &mut block, ecn, &erased_blocks[b])?;
        }
        out.extend_from_slice(&block[..block.len() - ecn]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use barcode_core::{BitImage, SymbologyDecoder};
    use barcode_fixtures::Canvas;

    fn image_of(canvas: &Canvas) -> BitImage<'_> {
        BitImage::new(canvas.width, canvas.height, canvas.bits())
    }

    /// Byte-mode framing (mode 4 + count + bytes + EC/11 pads) into an
    /// exact data-codeword stream for `barcode_fixtures::encode_qr`.
    fn frame_byte_payload(version: u32, data: &[u8], capacity: usize) -> Vec<u8> {
        let count_bits = if version <= 9 { 8 } else { 16 };
        let mut bits: Vec<bool> = vec![false, true, false, false]; // mode 4
        for i in (0..count_bits).rev() {
            bits.push((data.len() >> i) & 1 == 1);
        }
        for &b in data {
            for i in (0..8).rev() {
                bits.push((b >> i) & 1 == 1);
            }
        }
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        let mut out: Vec<u8> = bits
            .chunks(8)
            .map(|c| c.iter().fold(0u8, |a, &b| (a << 1) | b as u8))
            .collect();
        let mut pad = true;
        while out.len() < capacity {
            out.push(if pad { 0xEC } else { 0x11 });
            pad = !pad;
        }
        assert_eq!(out.len(), capacity, "payload must fit the version");
        out
    }

    /// Binary QR: byte-mode payloads that are not valid UTF-8 must come
    /// back exact in `raw`; `text` carries the Latin-1-style mapping.
    #[test]
    fn decode_returns_exact_bytes_for_binary_payloads() {
        let cases: Vec<(&[u8], &str)> = vec![
            (&[0x00, 0x01, 0x1F, 0x7F], "\u{0}\u{1}\u{1F}\u{7F}"),
            (&[0x80, 0xFF, 0xFE], "\u{80}\u{FF}\u{FE}"),
            (&[0xC3, 0x28], "\u{C3}("), // broken UTF-8 sequence
        ];
        for (data, text) in &cases {
            let stream = frame_byte_payload(1, data, barcode_fixtures::qr_data_codewords(1, 1) as usize); // v1-L
            let canvas = barcode_fixtures::encode_qr(1, 1, 2, &stream, 4);
            let img = image_of(&canvas);
            let mut d = crate::QrDecoder::new();
            let results = d.decode(&img);
            assert_eq!(results.len(), 1, "decode {data:?}");
            assert_eq!(results[0].raw, *data, "raw {data:?}");
            assert_eq!(results[0].text, *text, "text {data:?}");
        }
        // Every byte value at once (v10-L fits 256 byte-mode bytes).
        let all: Vec<u8> = (0..=255u8).collect();
        let stream = frame_byte_payload(10, &all, barcode_fixtures::qr_data_codewords(10, 1) as usize); // v10-L
        let canvas = barcode_fixtures::encode_qr(10, 1, 2, &stream, 4);
        let img = image_of(&canvas);
        let mut d = crate::QrDecoder::new();
        let results = d.decode(&img);
        assert_eq!(results.len(), 1, "all-256 decode");
        assert_eq!(results[0].raw, all);
        assert_eq!(results[0].text, crate::segments::payload_text(&all));
        // 16-bit-count version at scale: 1022 bytes (also cross-checked
        // byte-exact in zxing-cpp, see docs/STATUS.md).
        let big: Vec<u8> = (0..1022u32).map(|i| (i * 7 & 0xFF) as u8).collect();
        let stream = frame_byte_payload(26, &big, barcode_fixtures::qr_data_codewords(26, 1) as usize);
        let canvas = barcode_fixtures::encode_qr(26, 1, 2, &stream, 4);
        let img = image_of(&canvas);
        let mut d = crate::QrDecoder::new();
        let results = d.decode(&img);
        assert_eq!(results.len(), 1, "1022-byte decode");
        assert_eq!(results[0].raw, big);
    }

    fn grid_of(canvas: &Canvas) -> Grid {
        let img = image_of(canvas);
        let mut d = crate::QrDecoder::new();
        let finders = d.find_finders(&img).expect("finders not found");
        // Finders come back UNLABELED. Prefer the labeling whose codewords
        // actually correct — a permissive BCH corrector can validate a
        // mirrored labeling's garbage, and only Reed-Solomon is a reliable
        // judge. Among format-valid labelings with no RS verdict (pattern-
        // only canvases), prefer the highest format confidence: a clean
        // bit-exact copy (2) beats corrected agreement (1) — mirrored
        // grids never sample a clean copy of a real symbol.
        let mut fallback: Option<(Grid, u8)> = None;
        for perm in crate::LABELINGS {
            let [tl, tr, bl] =
                [finders[perm[0]], finders[perm[1]], finders[perm[2]]];
            let Some(grid) = crate::grid::Grid::from_finders(&tl, &tr, &bl) else { continue };
            let Some((ec, mask, quality)) = crate::grid::read_format_ranked(&img, &grid)
            else {
                continue;
            };
            if crate::codewords::read_codewords(&img, &grid, ec, mask).is_some() {
                return grid;
            }
            if fallback.map_or(true, |(_, q)| quality > q) {
                fallback = Some((grid, quality));
            }
        }
        fallback.expect("no valid labeling").0
    }

    /// Deterministic pseudo-random payload so codeword bits are nontrivial.
    fn payload(len: usize, seed: u8) -> Vec<u8> {
        let mut x = seed;
        (0..len)
            .map(|_| {
                x = x.wrapping_mul(133).wrapping_add(7);
                x
            })
            .collect()
    }

    fn data_len(version: u32, ec: EcLevel) -> usize {
        let plan = block_plan(version, ec).unwrap();
        plan.groups.iter().map(|&(cnt, sz)| cnt * sz).sum::<u32>() as usize
    }

    #[test]
    fn placement_positions_match_codeword_capacity() {
        // v1 carries no remainder bits; v2-v6 carry 7; later versions vary.
        // Remainder bits (geometric truth — see the fixture's
        // qr_remainder_bits note): v35/v36 have NONE despite the common
        // "v28-v36 → 3" transcription.
        let expected_remainder = |v: u32| match v {
            1 => 0,
            2..=6 => 7,
            7..=13 => 0,
            14..=20 => 3,
            21..=27 => 4,
            28..=34 => 3,
            _ => 0,
        };
        for v in 1..=40u32 {
            let n = 17 + 4 * v;
            let total = block_plan(v, EcLevel::L).unwrap().total_codewords() as usize;
            assert_eq!(
                placement_positions(n).len(),
                total * 8 + expected_remainder(v),
                "v{v}"
            );
        }
    }

    #[test]
    fn round_trips_every_version_and_level() {
        // v1-v6 exhaustively (small canvases), then a spread of larger
        // versions across all levels — the placement-count assert inside
        // encode_qr validates the occupancy map for every one of them.
        let mut versions = vec![1u32, 2, 3, 4, 5, 6];
        versions.extend([8u32, 10, 14, 21, 27, 33, 40]);
        for v in versions {
            for ec in [EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H] {
                let len = data_len(v, ec);
                let data = payload(len, 3 + v as u8);
                let ec_bits = match ec {
                    EcLevel::M => 0,
                    EcLevel::L => 1,
                    EcLevel::H => 2,
                    EcLevel::Q => 3,
                };
                let mask_id = (v + ec_bits as u32) % 8;
                let canvas = barcode_fixtures::encode_qr(v, ec_bits, mask_id as u8, &data, 4);
                eprintln!("DBGTRY v{v} ec{ec_bits} mask{mask_id}");
                let grid = grid_of(&canvas);
                let img = image_of(&canvas);
                let (got_ec, mask) = crate::grid::read_format(&img, &grid).expect("format");
                assert_eq!(got_ec, ec, "v{v} ec level");
                assert_eq!(mask, mask_id as u8, "v{v} mask");
                if v >= 7 {
                    assert_eq!(crate::grid::read_version(&img, &grid), Some(v), "v{v} version info");
                } else {
                    assert_eq!(crate::grid::read_version(&img, &grid), None, "v{v} has no version info");
                }
                let got = read_codewords(&img, &grid, ec, mask)
                    .unwrap_or_else(|| panic!("v{v} {ec:?}: no codewords"));
                assert_eq!(got, data, "v{v} {ec:?} data codewords");
            }
        }
    }

    #[test]
    fn corrects_codeword_errors_through_the_grid() {
        // v1-M: one block, nn = 10 parity codewords, t = 5.
        let len = data_len(1, EcLevel::M);
        let data = payload(len, 9);
        let mut canvas = barcode_fixtures::encode_qr(1, 0, 2, &data, 4);
        let positions = placement_positions(21);
        // Corrupt one module (one bit) in each of 5 distinct codewords.
        for k in 0..5usize {
            let (mx, my) = positions[k * 8 + 3];
            let (x, y) = (24 + mx * 4 + 2, 24 + my * 4 + 2); // quiet = 6*4
            canvas.flip(x, y);
        }
        let grid = grid_of(&canvas);
        let img = image_of(&canvas);
        let (_, mask) = crate::grid::read_format(&img, &grid).expect("format");
        let got = read_codewords(&img, &grid, EcLevel::M, mask).expect("correctable");
        assert_eq!(got, data, "5 codeword errors corrected");
    }

    /// Flip the whole 4x4 pixel block of module (mx, my) in a quiet-6 /
    /// module-4 canvas: the quadrant vote stays unanimous, so this is an
    /// UNDETECTABLE bit error (no erasure help).
    fn flip_module(canvas: &mut barcode_fixtures::Canvas, mx: u32, my: u32) {
        for dx in 0..4u32 {
            for dy in 0..4u32 {
                canvas.flip(24 + mx * 4 + dx, 24 + my * 4 + dy);
            }
        }
    }

    #[test]
    fn rejects_beyond_the_correction_capacity() {
        // v2-M: nn = 16, t = 8.
        // (An earlier draft used v1-Q, which does not exist — the format
        // reader now rejects Q on v1 outright.)
        let len = data_len(2, EcLevel::M);
        let data = payload(len, 4);
        let positions = placement_positions(25);

        // (a) 9 *undetectable* codeword errors (whole module flipped, the
        // quadrant vote is unanimous) exceed the error-only capacity and
        // get no erasure help → reject.
        let mut canvas = barcode_fixtures::encode_qr(2, 0, 5, &data, 4);
        for k in 0..9usize {
            let (mx, my) = positions[k * 8];
            flip_module(&mut canvas, mx, my);
        }
        let grid = grid_of(&canvas);
        let img = image_of(&canvas);
        let (_, mask) = crate::grid::read_format(&img, &grid).expect("format");
        assert_eq!(read_codewords(&img, &grid, EcLevel::M, mask), None);

        // (b) 15 detectable smears exceed even the erasure capacity
        // (nn − 2 = 14): reject rather than invent a payload.
        let mut canvas = barcode_fixtures::encode_qr(2, 0, 5, &data, 4);
        for k in 0..15usize {
            let (mx, my) = positions[k * 8];
            for (dx, dy) in [(1u32, 1u32), (1, 2), (2, 1), (2, 2)] {
                canvas.flip(24 + mx * 4 + dx, 24 + my * 4 + dy);
            }
        }
        let grid = grid_of(&canvas);
        let img = image_of(&canvas);
        let (_, mask) = crate::grid::read_format(&img, &grid).expect("format");
        assert_eq!(read_codewords(&img, &grid, EcLevel::M, mask), None);
    }

    #[test]
    fn erasures_recover_detectable_smears_beyond_error_capacity() {
        // v2-M: nn = 16, t = 8. Nine centre-2x2 smears are 2–2 quadrant
        // ties, so the plain pass cannot correct them as errors.
        let len = data_len(2, EcLevel::M);
        let data = payload(len, 4);
        let mut canvas = barcode_fixtures::encode_qr(2, 0, 5, &data, 4);
        let positions = placement_positions(25);
        let smeared: Vec<(u32, u32)> = (0..9usize).map(|k| positions[k * 8]).collect();
        for &(mx, my) in &smeared {
            for (dx, dy) in [(1u32, 1u32), (1, 2), (2, 1), (2, 2)] {
                canvas.flip(24 + mx * 4 + dx, 24 + my * 4 + dy);
            }
        }
        let grid = grid_of(&canvas);
        let img = image_of(&canvas);
        let (_, mask) = crate::grid::read_format(&img, &grid).expect("format");

        // The uncertainty mask must mark exactly those modules.
        let unc = grid.uncertainty(&img);
        let marked: Vec<(u32, u32)> = (0..25u32)
            .flat_map(|my| (0..25u32).map(move |mx| (mx, my)))
            .filter(|&(mx, my)| unc[(my * 25 + mx) as usize] != 0)
            .collect();
        for &(mx, my) in &smeared {
            assert!(marked.contains(&(mx, my)), "smear {mx},{my} not detected");
        }

        // Error-only is still over capacity; the auto retry (which uses
        // the quadrant vote) recovers the data.
        assert_eq!(
            read_codewords_with_erasures(&img, &grid, EcLevel::M, mask, &[]),
            None
        );
        let got = read_codewords(&img, &grid, EcLevel::M, mask).expect("erasure-assisted");
        assert_eq!(got, data, "9 erasures (nn = 16)");
    }

    #[test]
    fn rejects_unsupported_versions() {
        let grid = Grid {
            size: 57,
            tl: (0.0, 0.0),
            step_x: (1.0, 0.0),
            step_y: (0.0, 1.0),
            h: None,
        };
        let img = BitImage::new(1, 1, &[0u8; 4]);
        assert_eq!(read_codewords(&img, &grid, EcLevel::L, 0), None);
    }

    #[test]
    fn decode_returns_text_for_every_mode() {
        // Full pipeline: encode_qr_text → canvas → decode() → text.
        let cases: [(&str, &str); 5] = [
            ("HELLO WORLD", "byte"),
            ("4006381333931", "numeric"),
            ("CODE 39 $%*+-./:", "alphanumeric"),
            ("Grüße aus Wien — UTF-8", "utf-8 byte"),
            ("https://example.com/p?a=1&b=2", "url byte"),
        ];
        for (text, kind) in cases {
            let canvas = barcode_fixtures::encode_qr_text(text, 0, 2, 4).expect(kind);
            let img = image_of(&canvas);
            let mut d = crate::QrDecoder::new();
            let results = d.decode(&img);
            assert_eq!(results.len(), 1, "{kind}: expected exactly one result");
            assert_eq!(results[0].symbology, barcode_core::Symbology::QRCode);
            assert_eq!(results[0].text, text, "{kind}");
            // The quad must be the symbol box: finder centers sit 3.5
            // modules (14px) inside each corner; the quiet zone is 6
            // modules (24px). Corners derive from the canvas size, since
            // longer payloads legitimately pick larger versions.
            let dim = canvas.width as f32;
            let edge = dim - 24.0;
            let quad = &results[0].position;
            for (cx, cy) in [(24.0f32, 24.0), (edge, 24.0), (edge, edge), (24.0, edge)] {
                let hit = quad.iter().any(|p| (p.x - cx).abs() < 2.0 && (p.y - cy).abs() < 2.0);
                assert!(hit, "{kind}: quad misses corner ({cx},{cy}): {quad:?}");
            }
        }
    }

    #[test]
    fn decode_returns_nothing_on_blank_frames() {
        let canvas = barcode_fixtures::Canvas::new(200, 200);
        let img = image_of(&canvas);
        let mut d = crate::QrDecoder::new();
        assert!(d.decode(&img).is_empty());
    }

    /// Reverse a canvas through 180° (the field failure: an upside-down
    /// code propped against a box).
    fn rotate180(canvas: &Canvas) -> Canvas {
        let img = image_of(canvas);
        let mut out = Canvas::new(canvas.width, canvas.height);
        for y in 0..canvas.height {
            for x in 0..canvas.width {
                if img.ink(x, y) {
                    out.fill(canvas.width - 1 - x, canvas.height - 1 - y, 1, 1);
                }
            }
        }
        out
    }

    #[test]
    fn decodes_upside_down() {
        let canvas = barcode_fixtures::encode_qr_text("HELLO WORLD", 0, 2, 4).expect("fits");
        let rotated = rotate180(&canvas);
        let img = image_of(&rotated);
        let mut d = crate::QrDecoder::new();
        let results = d.decode(&img);
        assert_eq!(results.len(), 1, "upside-down QR must decode");
        assert_eq!(results[0].text, "HELLO WORLD");
    }

    #[test]
    fn decodes_at_thirty_degrees() {
        // A true tilt (not a multiple of 90°): runs stretch by 1/cos(30°)
        // and the finder squares sit obliquely — the detector must handle
        // both. Rotated into a larger canvas so the corners survive.
        // Module 8: rotation jags on a 4px module are ±25% of a module —
        // harsher than any real camera; 8px matches handset reality.
        let src_canvas = barcode_fixtures::encode_qr_text("TILT CHECK", 0, 2, 8).expect("fits");
        let side = src_canvas.width;
        let src = image_of(&src_canvas);
        let theta = 30.0f32.to_radians();
        let (c, s) = (theta.cos(), theta.sin());
        let dim = (side as f32 * 1.5) as u32;
        let mut dest = Canvas::new(dim, dim);
        let sc = side as f32 / 2.0;
        let dc = dim as f32 / 2.0;
        for y in 0..dim {
            for x in 0..dim {
                let (dx, dy) = (x as f32 - dc, y as f32 - dc);
                let sx = c * dx + s * dy + sc;
                let sy = -s * dx + c * dy + sc;
                if sx >= 0.0 && sy >= 0.0 && sx < side as f32 && sy < side as f32 {
                    if src.ink(sx as u32, sy as u32) {
                        dest.fill(x, y, 1, 1);
                    }
                }
            }
        }
        let img = image_of(&dest);
        let mut d = crate::QrDecoder::new();
        let f = d.find_finders(&img);
        eprintln!("DBG30 finders: {:?}", f.as_ref().map(|x| x.iter().map(|q| (q.x, q.y, q.module)).collect::<Vec<_>>()));
        let results = d.decode(&img);
        eprintln!("DBG30 results: {:?}", results.iter().map(|r| &r.text).collect::<Vec<_>>());
        assert_eq!(results.len(), 1, "30°-tilted QR must decode");
        assert_eq!(results[0].text, "TILT CHECK");
    }

    #[test]
    fn decodes_at_arbitrary_rotations() {
        let canvas = barcode_fixtures::encode_qr_text("HELLO WORLD", 0, 2, 4).expect("fits");
        // 90° CW: pixel (x, y) -> (H-1-y, x) on a square canvas.
        let side = canvas.width;
        let src = image_of(&canvas);
        let mut rot90 = Canvas::new(side, side);
        for y in 0..side {
            for x in 0..side {
                if src.ink(x, y) {
                    rot90.fill(side - 1 - y, x, 1, 1);
                }
            }
        }
        let img = image_of(&rot90);
        let mut d = crate::QrDecoder::new();
        let results = d.decode(&img);
        assert_eq!(results.len(), 1, "90°-rotated QR must decode");
        assert_eq!(results[0].text, "HELLO WORLD");
    }

    #[test]
    fn picks_the_smallest_fitting_version() {
        // One byte of text must encode as version 1-M (16 data codewords),
        // not something larger.
        let canvas = barcode_fixtures::encode_qr_text("A", 0, 0, 4).expect("fits");
        assert_eq!(canvas.width, (21 + 12) * 4);
        // v1-M byte mode holds 14 bytes (16 codewords minus mode+count
        // overhead); a 15-char mixed-case string needs byte mode and must
        // step up to version 2.
        let s15 = "1234567890abcde";
        let canvas = barcode_fixtures::encode_qr_text(s15, 0, 0, 4).expect("fits");
        assert_eq!(canvas.width, (25 + 12) * 4);
        // And something beyond even v40-M capacity fails cleanly.
        let too_big = "x".repeat(2400);
        assert!(barcode_fixtures::encode_qr_text(&too_big, 0, 0, 4).is_none());
    }
}


/// Field diagnostic: try every (EC level, mask) pair and report which
/// yield Reed-Solomon-valid codewords, with the decoded text when the
/// segments parse. The format read should agree with the first entry.
pub fn brute_force_format(img: &BitImage, grid: &Grid) -> Vec<((EcLevel, u8), Option<String>)> {
    let n = grid.size;
    let version = (n - 17) / 4;
    let positions = placement_positions(n);
    let mut out = Vec::new();
    for &ec in &[EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H] {
        let Some(plan) = block_plan(version, ec) else { continue };
        let total = plan.total_codewords() as usize;
        if positions.len() < total * 8 {
            continue;
        }
        for mask in 0u8..8 {
            let mut bytes = vec![0u8; total];
            for (k, &(mx, my)) in positions.iter().enumerate() {
                if k >= total * 8 {
                    break;
                }
                let raw = grid.sample(img, mx as i32, my as i32);
                if raw ^ mask_toggles(mask, my, mx) {
                    bytes[k / 8] |= 0x80 >> (k % 8);
                }
            }
            let gf = crate::rs::Gf::new();
            let (n1, s1) = plan.groups[0];
            let (n2, s2) = plan.groups[1];
            let nblocks = (n1 + n2) as usize;
            let ecn = plan.ec_per_block as usize;
            let mut data_blocks: Vec<Vec<u8>> = (0..nblocks)
                .map(|b| vec![0u8; if b < n1 as usize { s1 } else { s2 } as usize])
                .collect();
            let mut ec_blocks = vec![vec![0u8; ecn]; nblocks];
            {
                let mut it = bytes.iter();
                let mut ok = true;
                for j in 0..s1.max(s2) as usize {
                    for b in 0..nblocks {
                        if j < data_blocks[b].len() {
                            match it.next() {
                                Some(v) => data_blocks[b][j] = *v,
                                None => { ok = false; break; }
                            }
                        }
                    }
                }
                if ok {
                    for j in 0..ecn {
                        for b in 0..nblocks {
                            match it.next() {
                                Some(v) => ec_blocks[b][j] = *v,
                                None => { ok = false; break; }
                            }
                        }
                    }
                }
                if !ok {
                    continue;
                }
            }
            let mut all_ok = true;
            for b in 0..nblocks {
                let mut cw = data_blocks[b].clone();
                cw.extend_from_slice(&ec_blocks[b]);
                let nn = cw.len();
                if crate::rs::rs_correct(&gf, &mut cw, nn).is_none() {
                    all_ok = false;
                    break;
                }
                data_blocks[b] = cw[..data_blocks[b].len()].to_vec();
            }
            if all_ok {
                let mut data = Vec::new();
                for b in 0..nblocks {
                    data.extend_from_slice(&data_blocks[b]);
                }
                let text = crate::segments::parse_segments(version, &data)
                    .as_deref()
                    .map(crate::segments::payload_text);
                out.push(((ec, mask), text));
            }
        }
    }
    out
}
