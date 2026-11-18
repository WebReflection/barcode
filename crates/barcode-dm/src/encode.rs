//! ECC 200 encoder: payload -> codewords -> upright module matrix.
//!
//! Clean-room: the pad-codeword rule and the painter (solid L left+bottom,
//! dashed clock top+right) were derived empirically from zint output
//! (testdata/dm_truth.txt) and are byte-diffed against every size's
//! 'A' matrix in `tests/encode.rs`. Mode encodation mirrors [`crate::mode`]
//! decode-side; v1 is ASCII + upper shift (correct for every payload,
//! not maximally dense — C40/TEXT/X12/EDIFACT/Base256 later).

use crate::deinterleave::encode_stream;
use crate::rs::Gf;
use crate::tables::{SymbolSpec, SYMBOLS};

/// An encoded symbol: its spec and the upright module matrix
/// (`matrix[row * cols + col] != 0` is dark), borders included.
pub struct Encoded {
    pub spec: &'static SymbolSpec,
    pub matrix: Vec<u8>,
}

/// Encode a payload into the smallest symbol that holds it.
///
/// Returns None when even the largest symbol cannot hold the payload.
pub fn encode(payload: &[u8]) -> Option<Encoded> {
    let n = ascii_codewords(payload).len();
    // smallest spec whose data capacity fits the stream (+ terminator);
    // SYMBOLS is not capacity-ordered, so pick by (data, area) among fits.
    let spec = SYMBOLS
        .iter()
        .filter(|s| s.data >= n + 1)
        .min_by_key(|s| (s.data, s.rows * s.cols))?;
    encode_into(payload, spec)
}

/// Encode into an explicit symbol size. Fixtures need the printed layout:
/// the invoice DataMatrix is a 12x36 DMRE even though the same payload
/// also fits the smaller 20x20 square that [`encode`] would pick.
pub fn encode_as(payload: &[u8], rows: usize, cols: usize) -> Option<Encoded> {
    encode_into(payload, crate::tables::spec_for(rows, cols)?)
}

fn encode_into(payload: &[u8], spec: &'static SymbolSpec) -> Option<Encoded> {
    let data = ascii_codewords(payload);
    let n = data.len();
    if n + 1 > spec.data {
        return None;
    }
    let mut padded = data;
    padded.push(129); // end of data
    for p in n + 2..=spec.data {
        // pads live at 1-based positions in the data stream
        padded.push(((130 + (149 * p as u32) % 253) % 254) as u8);
    }
    debug_assert_eq!(padded.len(), spec.data);

    let gf = Gf::new();
    let stream = encode_stream(spec, &gf, &padded);
    Some(Encoded { spec, matrix: paint(spec, &stream) })
}

/// Payload -> data codewords, ASCII mode: digit pairs packed, 0..=127
/// direct (+1), 128..=255 via upper shift (235, v-127).
fn ascii_codewords(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() / 2 + 1);
    let mut i = 0;
    while i < payload.len() {
        let b = payload[i];
        if b.is_ascii_digit() && i + 1 < payload.len() && payload[i + 1].is_ascii_digit() {
            out.push(130 + 10 * (b - b'0') + (payload[i + 1] - b'0'));
            i += 2;
        } else if b <= 127 {
            out.push(b + 1);
            i += 1;
        } else {
            out.push(235);
            out.push(b - 127);
            i += 1;
        }
    }
    out
}

/// Paint the codeword stream onto the symbol: solid L (left+bottom),
/// dashed clock (top+right, even cols / odd rows dark), data modules via
/// the placement sweep. Region separators and the 2-4 leftover cells are
/// painted dark per the empirical rule below (byte-diffed vs zint).
fn paint(spec: &SymbolSpec, stream: &[u8]) -> Vec<u8> {
    let (r, c) = (spec.rows, spec.cols);
    let mut m = vec![0u8; r * c];
    for i in 0..r {
        m[i * c] = 1; // solid left
    }
    for j in 0..c {
        m[(r - 1) * c + j] = 1; // solid bottom
        if j % 2 == 0 {
            m[j] = 1; // dashed top
        }
    }
    for i in 0..r {
        if i % 2 == 1 {
            m[i * c + c - 1] = 1; // dashed right
        }
    }
    // data modules over the inner area
    let positions = crate::placement::stream_positions(spec);
    for (k, &(sr, sc)) in positions.iter().enumerate() {
        let bit = (stream[k / 8] >> (7 - (k % 8))) & 1;
        m[sr as usize * c + sc as usize] = bit;
    }
    // leftover stream cells (12x12/16x16/20x20/24x24): the last 2x2 map
    // corner gets the fixed diagonal-dark pattern
    if spec.kr == 1 && spec.kc == 1 && spec.map_rows() * spec.map_cols() > 8 * spec.codewords() {
        for &(mr, mc) in &[
            (spec.s_r - 2, spec.s_c - 2),
            (spec.s_r - 1, spec.s_c - 1),
        ] {
            let (sr, sc) = spec.to_symbol(mr, mc);
            m[sr * c + sc] = 1;
        }
    }
    // region separators: a solid line + a dashed one in the phase of the
    // outer border it runs parallel to (top even-dark, right odd-dark)
    for i in 1..spec.kr {
        let y = i * spec.s_r + 2 * (i - 1) + 1;
        for j in 0..c {
            m[y * c + j] = 1;
            if j % 2 == 0 { m[(y + 1) * c + j] = 1; }
        }
    }
    for i in 1..spec.kc {
        let x = i * spec.s_c + 2 * (i - 1) + 1;
        for j in 0..r {
            if j % 2 == 1 { m[j * c + x] = 1; }
            m[j * c + x + 1] = 1;
        }
    }
    m
}