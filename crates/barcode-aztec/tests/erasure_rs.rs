//! Item 6c: Aztec Reed–Solomon erasures, end-to-end through the frame
//! detector. Damage is *detectable*: smeared modules are drawn half
//! dark/half light, so the quadrant vote ties 2–2 and the affected
//! codeword becomes a known erasure. Without the erasure list the same
//! damage exceeds the error-only capacity.

use barcode_aztec::encode::encode;
use barcode_aztec::{mode, placement, read_frame, read_stream, read_stream_with_erasures, text};

const PAYLOAD: &[u8] = b"AZTEC-ERASURE-GATE-001";

/// First data-path cell of the raw `b`-bit codeword window `cw`.
fn window_cell(path: &[(u16, u16)], start: usize, cw: usize, b: usize) -> (u16, u16) {
    path[start + cw * b]
}

fn render(
    matrix: &[u8],
    size: usize,
    pitch: usize,
    quiet: usize,
    smear: &[(usize, usize)],
) -> Vec<u8> {
    let dim = (size + 2 * quiet) * pitch;
    let mut dark = vec![0u8; dim * dim];
    for r in 0..size {
        for c in 0..size {
            let x0 = (c + quiet) * pitch;
            let y0 = (r + quiet) * pitch;
            let ink = matrix[r * size + c] != 0;
            for dy in 0..pitch {
                for dx in 0..pitch {
                    let on = if smear.contains(&(r, c)) {
                        // left half dark, right half light: the quadrant
                        // samples tie 2–2 whatever the true bit was
                        dx < pitch / 2
                    } else {
                        ink
                    };
                    dark[(y0 + dy) * dim + x0 + dx] = on as u8;
                }
            }
        }
    }
    dark
}

#[test]
fn aztec_decodes_more_damage_with_known_erasures() {
    let (spec, matrix) = encode(PAYLOAD).expect("encode");
    let size = spec.rows as usize;
    let b = spec.bits as usize;
    let path = placement::data_path(spec);
    let (_, data_cw) = mode::read_mode(&matrix, spec).expect("clean mode message");
    let d = data_cw as usize;
    let k = spec.total as usize - d;

    // Damage e = K − 2 data codewords (leaving 2 syndrome checks), one raw
    // bit window each. e > K/2 for every K >= 6, so error-only must fail.
    let e = k.saturating_sub(2);
    assert!(e > k / 2, "payload needs K >= 6 check words (K = {k})");
    let damaged_cw: Vec<usize> = (0..e).collect();
    let mut damaged = matrix.clone();
    let mut uncertain = vec![0u8; size * size];
    let mut smear: Vec<(usize, usize)> = Vec::new();
    for &cw in &damaged_cw {
        let (r, c) = window_cell(&path, spec.skip as usize, cw, b);
        damaged[r as usize * size + c as usize] ^= 1;
        uncertain[r as usize * size + c as usize] = 1;
        smear.push((r as usize, c as usize));
    }

    assert!(
        read_stream(&damaged, spec).is_none(),
        "{e} damaged codewords > error-only capacity {k}/2"
    );
    let stream = read_stream_with_erasures(&damaged, spec, &uncertain).expect("erasure decode");
    assert_eq!(text::decode_text(&stream), PAYLOAD);

    // The same symbol rendered at camera scale with those modules smeared
    // is found and decoded by the detector's erasure retry.
    for pitch in [6usize, 8, 10] {
        let frame = render(&damaged, size, pitch, 4, &smear);
        let dim = (size + 8) * pitch;
        let hits = read_frame(&frame, dim, dim);
        assert!(
            hits.iter().any(|h| text::decode_text(&h.stream) == PAYLOAD),
            "smeared Aztec not decoded at pitch {pitch}"
        );
    }
}

#[test]
fn stuffed_windows_report_erasure_positions() {
    // Every uncertain bit must mark exactly its own codeword; a clean
    // stream must report none. The bit → codeword map under stuffing is
    // computed during the parse, so pin it at both parses' boundaries.
    let (spec, matrix) = encode(PAYLOAD).expect("encode");
    let b = spec.bits as usize;
    let cap = spec.cap_bits() as usize;
    let (_, data_cw) = mode::read_mode(&matrix, spec).expect("mode");
    let d = data_cw as usize;
    let bits = placement::read_bits(&matrix, spec);
    let clean = vec![0u8; cap];
    let (_, _, er) =
        placement::parse_stuffed_erasures(&bits, &clean, spec.skip as usize, b, d).expect("parse");
    assert!(er.is_empty());

    // Mark the first bit of the first codeword: exactly codeword 0.
    let mut unc = vec![0u8; cap];
    let pos = spec.skip as usize;
    unc[pos] = 1;
    let (_, _, er) = placement::parse_stuffed_erasures(&bits, &unc, pos, b, d).expect("parse");
    assert_eq!(er, vec![0]);

    // Windows at the raw offset: same index.
    assert_eq!(placement::window_erasures(&unc, pos, d, b), vec![0]);
    assert_eq!(placement::window_erasures(&clean, pos, d, b), Vec::new());
}
