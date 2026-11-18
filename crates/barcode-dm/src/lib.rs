//! ECC 200 (Data Matrix) decoding — clean-room implementation validated
//! against libdmtx/zint oracles.
//!
//! Module map:
//! - [`tables`]: symbol characteristics (sizes, capacity split, RS blocks,
//!   data-region grids).
//! - [`placement`]: annex M bit-to-module assignment on the concatenated
//!   mapping matrix.
//! - [`rs`]: GF(256) Reed-Solomon (poly 0x12D, roots α^1..α^nn).
//! - [`deinterleave`]: stream → blocks → corrected data codewords.
//! - [`mode`]: corrected data codewords → payload bytes.
//! - [`locator`]: L-finder detection in grayscale frames.

pub mod deinterleave;
pub mod encode;
pub mod locator;
pub mod mode;
pub mod placement;
pub mod rs;
pub mod tables;

/// A decoded Data Matrix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    /// The corrected data codewords, in stream order.
    pub data: Vec<u8>,
    /// The decoded payload bytes, when the mode stream was consistent.
    pub payload: Option<Vec<u8>>,
}

/// Decode an upright binarized symbol: `matrix[row * cols + col] != 0` is
/// dark, with the L finder on the left+bottom and the clock track on the
/// top+right. Returns None when the size is unknown or a block is
/// uncorrectable.
pub fn decode_upright(matrix: &[u8], rows: usize, cols: usize) -> Option<Decoded> {
    decode_upright_with_erasures(matrix, rows, cols, &[])
}

/// As [`decode_upright`], with module-level uncertainty: `uncertain` is
/// indexed like `matrix` and non-zero marks a module whose samples
/// disagreed. Every codeword touching an uncertain module becomes a
/// **known erasure** for Reed–Solomon (item 6b): erasures cost one parity
/// symbol instead of an error's two, so a smeared print with ambiguous
/// modules can decode past the error-only capacity.
pub fn decode_upright_with_erasures(
    matrix: &[u8],
    rows: usize,
    cols: usize,
    uncertain: &[u8],
) -> Option<Decoded> {
    let spec = tables::spec_for(rows, cols)?;
    let stream = placement::read_codewords(matrix, rows, cols, spec, spec.codewords());
    let erasures = if uncertain.is_empty() {
        Vec::new()
    } else {
        placement::codeword_erasures(spec, uncertain)
    };
    let data = deinterleave::decode_stream_with_erasures(spec, &stream, &erasures)?;
    let payload = mode::decode_payload(&data);
    Some(Decoded { data, payload })
}
