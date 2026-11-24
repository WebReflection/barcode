//! Codeword stream layout: de-interleaving the blocks and RS-correcting.
//!
//! The stream is the data codewords (in order) followed by the ecc
//! codewords interleaved round-robin by block. When the data count does
//! not divide evenly by the block count the deal is ROTATED:
//! data[i] -> block (i − (D mod B)) mod B — the short blocks come first
//! in the ecc interleave. Discovered and verified on 144x144
//! (1558 = 10·155 + 8); with D % B == 0 this reduces to plain round-robin.

use crate::rs::{rs_correct_erasures, Gf};
use crate::tables::SymbolSpec;

/// Split the raw codeword stream into per-block (data ++ parity) words.
fn split_blocks(spec: &SymbolSpec, stream: &[u8]) -> Vec<Vec<u8>> {
    let b = spec.blocks;
    let d_total = spec.data;
    let r = d_total % b;
    let q = d_total / b;
    let mut blocks = vec![Vec::new(); b];
    for (i, v) in stream.iter().enumerate() {
        if i < d_total {
            // data[i] -> block (i − r) mod B
            blocks[(i + b - r) % b].push(*v);
        } else {
            // ecc[k*B + b] -> block b, k-th parity
            blocks[(i - d_total) % b].push(*v);
        }
    }
    debug_assert!(blocks
        .iter()
        .all(|bl| bl.len() == q + spec.nn() || bl.len() == q + 1 + spec.nn()));
    blocks
}

/// Reassemble the corrected data codewords back into stream order.
fn join_data(spec: &SymbolSpec, blocks: &[Vec<u8>]) -> Vec<u8> {
    let b = spec.blocks;
    let d_total = spec.data;
    let r = d_total % b;
    let mut out = Vec::with_capacity(d_total);
    let mut idx = vec![0usize; b];
    for i in 0..d_total {
        let blk = (i + b - r) % b;
        out.push(blocks[blk][idx[blk]]);
        idx[blk] += 1;
    }
    out
}

/// De-interleave, RS-correct every block and return the data codewords,
/// or None if any block is uncorrectable.
pub fn decode_stream(spec: &SymbolSpec, stream: &[u8]) -> Option<Vec<u8>> {
    decode_stream_with_erasures(spec, stream, &[])
}

/// Stream index → (block, offset within the block), matching
/// [`split_blocks`]. Erasure positions arrive from the module sampler in
/// stream order; RS works per block.
fn stream_position(spec: &SymbolSpec, i: usize) -> (usize, usize) {
    let b = spec.blocks;
    let d_total = spec.data;
    let r = d_total % b;
    let q = d_total / b;
    if i < d_total {
        let blk = (i + b - r) % b;
        let first = (blk + r) % b;
        (blk, (i - first) / b)
    } else {
        let k = i - d_total;
        let blk = k % b;
        let blk_data = q + usize::from(r > 0 && blk >= b - r);
        (blk, blk_data + k / b)
    }
}

/// As [`decode_stream`], with known erasure positions in **stream**
/// order. Each erasure is charged to its block, so the RS capacity is
/// `e + 2t <= nn` per block (vs `2t <= nn` for unknown errors).
pub fn decode_stream_with_erasures(
    spec: &SymbolSpec,
    stream: &[u8],
    erasures: &[usize],
) -> Option<Vec<u8>> {
    let gf = Gf::new();
    let nn = spec.nn();
    let mut blocks = split_blocks(spec, stream);
    let mut per_block: Vec<Vec<usize>> = vec![Vec::new(); spec.blocks];
    for &i in erasures {
        if i >= stream.len() {
            return None;
        }
        let (blk, off) = stream_position(spec, i);
        per_block[blk].push(off);
    }
    for (block, er) in blocks.iter_mut().zip(per_block.iter_mut()) {
        er.sort_unstable();
        er.dedup();
        rs_correct_erasures(&gf, block, nn, er)?;
    }
    Some(join_data(spec, &blocks))
}

/// Encode data into the full codeword stream (data || interleaved ecc),
/// applying the uneven-block rotation. Test/generator side.
pub fn encode_stream(spec: &SymbolSpec, gf: &Gf, data: &[u8]) -> Vec<u8> {
    assert_eq!(data.len(), spec.data);
    let b = spec.blocks;
    let nn = spec.nn();
    let r = spec.data % b;
    // deal with the rotation
    let mut blocks = vec![Vec::with_capacity(spec.data / b + 1); b];
    for (i, v) in data.iter().enumerate() {
        blocks[(i + b - r) % b].push(*v);
    }
    let mut stream = data.to_vec();
    // parities per block, then the ecc section in interleaved order
    let parities: Vec<Vec<u8>> = blocks
        .iter()
        .map(|bl| crate::rs::rs_encode(gf, bl, nn))
        .collect();
    for k in 0..nn {
        for par in &parities {
            stream.push(par[k]);
        }
    }
    stream
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::spec_for;

    #[test]
    fn roundtrips_the_144_uneven_layout() {
        let spec = spec_for(144, 144).unwrap();
        let gf = Gf::new();
        let data: Vec<u8> = (0..spec.data).map(|i| (i * 7 + 13) as u8).collect();
        let stream = encode_stream(&spec, &gf, &data);
        assert_eq!(stream.len(), spec.codewords());
        let decoded = decode_stream(&spec, &stream).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrips_single_block() {
        let spec = spec_for(10, 10).unwrap();
        let gf = Gf::new();
        let data: Vec<u8> = (0..spec.data).map(|i| (i * 91 + 5) as u8).collect();
        let stream = encode_stream(&spec, &gf, &data);
        assert_eq!(decode_stream(&spec, &stream).unwrap(), data);
    }

    #[test]
    fn corrects_errors_across_blocks() {
        let spec = spec_for(52, 52).unwrap(); // 2 blocks, nn 42
        let gf = Gf::new();
        let data: Vec<u8> = (0..spec.data).map(|i| (i * 31 + 3) as u8).collect();
        let mut stream = encode_stream(&spec, &gf, &data);
        // 10 errors spread over both blocks (t = 21 per block)
        for &i in &[0usize, 5, 100, 203, 204, 210, 250, 260, 270, 287] {
            stream[i] ^= 0x81;
        }
        assert_eq!(decode_stream(&spec, &stream).unwrap(), data);
    }

    #[test]
    fn stream_position_matches_split_blocks() {
        // Every size, including the rotated uneven-block layouts.
        for spec in crate::tables::SYMBOLS {
            let gf = Gf::new();
            let data: Vec<u8> = (0..spec.data).map(|i| (i * 17 + 1) as u8).collect();
            let stream = encode_stream(spec, &gf, &data);
            let blocks = split_blocks(spec, &stream);
            for (i, &v) in stream.iter().enumerate() {
                let (blk, off) = stream_position(spec, i);
                assert_eq!(
                    blocks[blk][off], v,
                    "{}x{} stream {i}",
                    spec.rows, spec.cols
                );
            }
        }
    }

    #[test]
    fn erasures_are_charged_to_their_block() {
        let spec = spec_for(52, 52).unwrap(); // 2 blocks, nn 42
        let gf = Gf::new();
        let data: Vec<u8> = (0..spec.data).map(|i| (i * 13 + 9) as u8).collect();
        let clean = encode_stream(&spec, &gf, &data);
        // 26 erasures per block (nn 42) — more than the 21 error-only
        // capacity — spread over data and parity codewords of both blocks.
        let mut erased: Vec<usize> = Vec::new();
        let mut per = [0usize; 2];
        for i in 0..clean.len() {
            let (blk, _) = stream_position(&spec, i);
            if per[blk] < 26 {
                per[blk] += 1;
                erased.push(i);
            }
        }
        assert_eq!(per, [26, 26]);
        let mut damaged = clean.clone();
        for &i in &erased {
            damaged[i] ^= 0xa7;
        }
        assert_eq!(
            decode_stream(&spec, &damaged.clone()),
            None,
            "26/block > 21"
        );
        assert_eq!(
            decode_stream_with_erasures(&spec, &damaged, &erased).unwrap(),
            data
        );
        // The same damage with a stale (empty) list still fails.
        assert_eq!(decode_stream_with_erasures(&spec, &damaged, &[]), None);
    }
}
