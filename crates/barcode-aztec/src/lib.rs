//! Aztec Code (ISO/IEC 24778) decoding — clean-room implementation
//! validated against zint oracles.
//!
//! Module map:
//! - [`gf`]: generic GF(2^b) fields (b = 4, 6, 8, 10, 12).
//! - [`rs`]: Reed-Solomon over those fields (roots α^1..α^nn, α = 2).
//! - [`tables`]: the 36 symbol sizes.
//! - [`placement`]: the data spiral, stream reading, bit destuffing.
//! - [`mode`]: the mode message in the core ring.
//!
//! Bit conventions (docs/STATUS.md aztec blocks): a stream bit 1 is a dark
//! module; input matrices are `row * cols + col`, nonzero = dark.

pub mod detect;
pub mod encode;
pub mod gf;
pub mod mode;
pub mod placement;
pub mod rs;
pub mod tables;
pub mod text;

/// Successful stream decode: the RS-corrected data codewords.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stream {
    pub layers: u8,
    /// Data codeword count from the mode message.
    pub data_cw: u16,
    /// Corrected data codeword values (b bits each, MSB first).
    pub data: Vec<u16>,
}

/// One decoded symbol found in a binarized frame.
#[derive(Clone, Debug)]
pub struct FrameHit {
    pub stream: Stream,
    /// Symbol corner module centers in frame pixels, order TL, TR, BR, BL.
    pub quad: [f32; 8],
}

/// Decode every Aztec symbol in a binarized frame (`dark[y * w + x]`,
/// nonzero = dark): locate bullseyes, orient from the corner marks,
/// extract the module grid for each candidate size — the mode message
/// arbitrates, since a wrong size guess fails its check nibbles — and
/// stream-decode. The bullseye's compact/full flag is only a hint; both
/// core classes are tried (a wrong class finds no orientation).
pub fn read_frame(dark: &[u8], w: usize, h: usize) -> Vec<FrameHit> {
    let mut hits = Vec::new();
    for b in detect::find_bullseyes(dark, w, h) {
        'classes: for full in [b.full, !b.full] {
            let half = detect::core_half(full);
            let Some(orient) = detect::find_orientation(dark, w, h, &b, half) else {
                continue;
            };
            let map = detect::FrameMap {
                x: b.x,
                y: b.y,
                scale: b.scale,
                orient,
            };
            for spec in tables::all_specs().iter().filter(|s| s.compact == !full) {
                let size = spec.rows as usize;
                let c = (size / 2) as i32;
                // Corner module centers; the symbol must fit the frame.
                let corners = [
                    map.pixel(-c, -c),
                    map.pixel(c, -c),
                    map.pixel(c, c),
                    map.pixel(-c, c),
                ];
                if corners
                    .iter()
                    .any(|&(x, y)| x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32)
                {
                    continue;
                }
                let mut matrix = vec![0u8; size * size];
                for r in 0..size {
                    for col in 0..size {
                        if map.module(dark, w, h, col as i32 - c, r as i32 - c) {
                            matrix[r * size + col] = 1;
                        }
                    }
                }
                let mut stream = read_stream(&matrix, spec);
                // Erasure retry (item 6c) only once the mode message has
                // arbitrated this size/orientation — wrong guesses must
                // not pay the 5x quality sampling.
                if stream.is_none() && mode::read_mode(&matrix, spec).is_some() {
                    let mut uncertain = vec![0u8; size * size];
                    for r in 0..size {
                        for col in 0..size {
                            let (_, u) =
                                map.module_quality(dark, w, h, col as i32 - c, r as i32 - c);
                            uncertain[r * size + col] = u as u8;
                        }
                    }
                    stream = read_stream_with_erasures(&matrix, spec, &uncertain);
                }
                if let Some(stream) = stream {
                    let mut quad = [0f32; 8];
                    for (i, &(x, y)) in corners.iter().enumerate() {
                        quad[2 * i] = x;
                        quad[2 * i + 1] = y;
                    }
                    hits.push(FrameHit { stream, quad });
                    break 'classes;
                }
            }
        }
    }
    hits
}

/// Read the RS-corrected data codewords from a binarized symbol.
///
/// The check words are the last `K = total − D` codewords of the stream
/// (D from the mode message), so the data occupies the leading cells. Two
/// parse candidates are RS-arbitrated (docs/STATUS.md "DECODER"):
/// raw `b`-bit windows (handles zint's zeroing quirks and small skips —
/// a shifted window is just one wrong symbol), then the stuff-aware parse
/// landing exactly on the check region.
pub fn read_stream(matrix: &[u8], spec: &tables::Spec) -> Option<Stream> {
    read_stream_with_erasures(matrix, spec, &[])
}

/// As [`read_stream`], with module-level uncertainty (`uncertain` is
/// indexed like `matrix`, non-zero = the module's samples disagreed).
/// Every codeword touching an uncertain bit becomes a **known erasure**
/// (item 6c): one parity slot instead of an error's two, so a symbol whose
/// modules were smeared/merged can decode past the error-only capacity.
pub fn read_stream_with_erasures(
    matrix: &[u8],
    spec: &tables::Spec,
    uncertain: &[u8],
) -> Option<Stream> {
    let (layers, data_cw) = mode::read_mode(matrix, spec)?;
    let d = data_cw as usize;
    let b = spec.bits as usize;
    let cap = spec.cap_bits() as usize;
    let k = spec.total as usize - d;

    let bits = placement::read_bits(matrix, spec);
    debug_assert_eq!(bits.len(), cap);
    // The uncertainty mask arrives in symbol-module coordinates; the RS
    // sees stream bits along the placement path.
    let unc: Vec<u8> = if uncertain.len() == matrix.len() {
        placement::data_path(spec)
            .into_iter()
            .map(|(r, c)| uncertain[r as usize * spec.cols as usize + c as usize])
            .collect()
    } else {
        Vec::new()
    };
    let check = placement::windows(&bits, cap - k * b, k, b);
    let check_erased: Vec<usize> = placement::window_erasures(&unc, cap - k * b, k, b);

    let gf = gf::field_for(b as u32);
    let attempt = |mut seq: Vec<u16>, erased: &[usize]| -> Option<Vec<u16>> {
        let ok = if erased.is_empty() {
            rs::rs_correct(&gf, &mut seq, k).is_some()
        } else {
            rs::rs_correct_erasures(&gf, &mut seq, k, erased).is_some()
        };
        ok.then(|| seq[..d].to_vec())
    };

    // Candidate A: the data windows at their true offset (the stream is
    // [skip zeros][D data windows][K check words] — aztec_derive pinned
    // check == rs_encode(windows@skip)). Reading from 0 instead would turn
    // every skip-carrying symbol into D RS-corrections for nothing.
    let raw = placement::windows(&bits, spec.skip as usize, d, b);
    let raw_erased: Vec<usize> = placement::window_erasures(&unc, spec.skip as usize, d, b);
    let mut raw_erased_all = raw_erased;
    raw_erased_all.extend(check_erased.iter().map(|&j| d + j));
    if let Some(data) = attempt([&raw[..], &check].concat(), &raw_erased_all) {
        return Some(Stream {
            layers,
            data_cw,
            data,
        });
    }

    // Candidate B: stuff-aware parse from every legal skip offset.
    for skip in 0..=(cap % b) {
        let parsed = if unc.is_empty() {
            placement::parse_stuffed(&bits, skip, b, d).map(|(v, p)| (v, p, Vec::new()))
        } else {
            placement::parse_stuffed_erasures(&bits, &unc, skip, b, d)
        };
        if let Some((vals, pos, erased)) = parsed {
            if pos == cap - k * b {
                let mut er = erased;
                er.extend(check_erased.iter().map(|&j| d + j));
                if let Some(data) = attempt([vals, check.clone()].concat(), &er) {
                    return Some(Stream {
                        layers,
                        data_cw,
                        data,
                    });
                }
            }
        }
    }
    None
}
