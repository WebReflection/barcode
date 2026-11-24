//! Codeword stream extraction from an exact module matrix.
//!
//! Input: `dark[row * w + col]`, 1 = dark, top-down, matrix exactly
//! covering the symbol (no quiet zone) — the locator's job comes later.
//!
//! Row geometry (pixel-verified against zint, docs/STATUS.md):
//! `[start 17][left ind 17][data cols x 17][right ind 17][stop 18]`,
//! start = 8,1,1,1,1,1,1,3; stop = 7,1,1,3,1,1,1,2,1; codeword rows
//! render as uniform bands (zint default height 10 modules).

use crate::tables::{cluster_of_row, value_of};

pub const START: [u8; 8] = [8, 1, 1, 1, 1, 1, 1, 3];
pub const STOP: [u8; 9] = [7, 1, 1, 3, 1, 1, 1, 2, 1];

/// Symbol geometry derived from an exact matrix.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub rows: usize,
    pub cols: usize,
    pub band_h: usize,
}

/// Detect band height (first row from the top that differs from row 0)
/// and the column count from the row width.
pub fn detect_geometry(dark: &[u8], w: usize, h: usize) -> Option<Geometry> {
    let row = |y: usize| &dark[y * w..y * w + w];
    let band_h = (1..h).find(|&y| row(y) != row(0))?;
    if h % band_h != 0 {
        return None;
    }
    let rows = h / band_h;
    if (w - 18) % 17 != 0 {
        return None;
    }
    let cols = (w - 18) / 17 - 3;
    if cols == 0 || rows == 0 {
        return None;
    }
    Some(Geometry { rows, cols, band_h })
}

/// Run lengths of one matrix row, starting with the leading dark run.
fn runs_of(row: &[u8]) -> Option<Vec<u32>> {
    if row.first() != Some(&1) {
        return None; // must start with the start-pattern bar
    }
    let mut runs = vec![0u32];
    let mut cur_dark = true;
    for &d in row {
        let is_dark = d != 0;
        if is_dark == cur_dark {
            *runs.last_mut().unwrap() += 1;
        } else {
            runs.push(1);
            cur_dark = is_dark;
        }
    }
    Some(runs)
}

/// Split runs at cumulative cell boundaries; every cell starts dark.
fn split_cells(runs: &[u32], bounds: &[usize]) -> Option<Vec<Vec<u8>>> {
    let mut runs = runs.to_vec();
    let mut cells = Vec::with_capacity(bounds.len());
    let mut pos = 0usize;
    let mut ri = 0usize;
    let mut dark = true;
    for &end in bounds {
        let mut elems = Vec::new();
        let mut first_dark = dark;
        while pos < end {
            let take = (end - pos).min(runs.get(ri)?.to_owned() as usize) as u32;
            if elems.is_empty() {
                first_dark = dark;
            }
            elems.push(take as u8);
            pos += take as usize;
            if take == runs[ri] {
                ri += 1;
                dark = !dark;
            } else {
                runs[ri] -= take;
                dark = !dark;
            }
        }
        let _ = first_dark;
        if !first_dark {
            return None; // cells must start with a bar
        }
        cells.push(elems);
    }
    Some(cells)
}

/// Read the full data-region codeword stream (length descriptor, data,
/// pads, ecc) from an exact symbol matrix, row-major.
pub fn read_exact(dark: &[u8], w: usize, h: usize) -> Option<Vec<u16>> {
    read_with(dark, w, h, false).map(|(stream, _)| stream)
}

/// Like [`read_exact`], but a row or codeword the print damaged is emitted
/// as a zero placeholder instead of failing the whole symbol: RS treats
/// the wrong codewords as errors, and the symbol's EC level is chosen for
/// exactly that. The rectified detector uses this on camera frames — at
/// module resolution a real print still carries 1-2 malformed codewords
/// (the KLM boarding pass: 5 columns x 19 rows).
pub fn read_exact_tolerant(dark: &[u8], w: usize, h: usize) -> Option<Vec<u16>> {
    read_with(dark, w, h, true).map(|(stream, _)| stream)
}

/// [`read_exact_tolerant`] plus the stream indices of every codeword that
/// no cell could be decoded for (emitted as zero placeholders). Callers
/// gate on the count and pass the indices to RS as *erasures* (one slot
/// each instead of two). A mostly-placeholder grid is still rejected: it
/// is not a symbol, and RS must not be allowed to "correct" it into a
/// plausible payload.
pub fn read_exact_tolerant_with_stats(
    dark: &[u8],
    w: usize,
    h: usize,
) -> Option<(Vec<u16>, Vec<usize>)> {
    read_with(dark, w, h, true)
}

fn read_with(dark: &[u8], w: usize, h: usize, tolerant: bool) -> Option<(Vec<u16>, Vec<usize>)> {
    let g = detect_geometry(dark, w, h)?;
    // cumulative cell ends: start(17) left(17) data*cols right(17) stop(18)
    let mut bounds = vec![17usize, 34];
    for _ in 0..g.cols {
        bounds.push(bounds.last().unwrap() + 17);
    }
    bounds.push(bounds.last().unwrap() + 17); // right indicator
    bounds.push(bounds.last().unwrap() + 18); // stop pattern
    if *bounds.last().unwrap() != w {
        return None;
    }

    let mut stream = Vec::new();
    let mut placeholders: Vec<usize> = Vec::new();
    for b in 0..g.rows {
        let y = b * g.band_h + g.band_h / 2;
        let missing_row = || -> Option<Vec<u16>> {
            if tolerant {
                Some(vec![0u16; g.cols])
            } else {
                None
            }
        };
        let runs = match runs_of(&dark[y * w..y * w + w]) {
            Some(runs) => runs,
            None => {
                let row = missing_row()?;
                let start = stream.len();
                stream.extend(row);
                placeholders.extend(start..stream.len());
                continue;
            }
        };
        let cells = match split_cells(&runs, &bounds) {
            Some(cells) => cells,
            None => {
                let row = missing_row()?;
                let start = stream.len();
                stream.extend(row);
                placeholders.extend(start..stream.len());
                continue;
            }
        };
        // The start/stop patterns are *sync*, not data: at module
        // resolution a thermal print erodes their 1-module bars into
        // single pixels, so the exact run list is regularly
        // [7,6,1,3] / [7,1,1,7,2] while every data cell in the row is
        // intact. Tolerant mode therefore checks their module total and
        // leading bar only — zeroing the row used to turn an eroded edge
        // into 5 lost codewords (the KLM pass lost >8 codewords that way).
        let start = &cells[0];
        let start_ok = if tolerant {
            !start.is_empty()
                && start.iter().map(|&v| v as usize).sum::<usize>() == 17
                && start[0] >= 7
        } else {
            *start == START
        };
        if !start_ok {
            let row = missing_row()?;
            let start = stream.len();
            stream.extend(row);
            placeholders.extend(start..stream.len());
            continue;
        }
        // The stop pattern sits on the symbol's OUTER edge: print bleed
        // and quiet-zone blur regularly widen its final 1-module bar to 2
        // (the KLM boarding pass reads [7,1,1,3,1,1,1,1,2] on the
        // top/bottom rows). Validate its robust prefix and module total;
        // the RS pass is the real arbiter of the payload.
        let stop = &cells[g.cols + 3];
        let stop_ok = if tolerant {
            stop.first().is_some_and(|&v| (6..=9).contains(&v))
                && (17..=19).contains(&stop.iter().map(|&v| v as usize).sum::<usize>())
        } else {
            stop.len() == STOP.len()
                && stop[..7] == STOP[..7]
                && stop.iter().map(|&v| v as usize).sum::<usize>() == 18
        };
        if !stop_ok {
            let row = missing_row()?;
            let start = stream.len();
            stream.extend(row);
            placeholders.extend(start..stream.len());
            continue;
        }
        let cluster = cluster_of_row(b + 1); // symbol rows are 1-based
        for cell in &cells[2..2 + g.cols] {
            match value_of(cluster, cell) {
                Some(v) => stream.push(v),
                None if tolerant => {
                    placeholders.push(stream.len());
                    stream.push(0);
                }
                None => return None,
            }
        }
    }
    Some((stream, placeholders))
}

/// Split a full stream into (descriptor, data_with_pads, ecc):
/// descriptor = 1 + #data + #pads (ECC excluded, spec 2.3).
pub fn split_stream(stream: &[u16]) -> Option<(u16, &[u16], &[u16])> {
    let desc = *stream.first()?;
    if desc == 0 || desc as usize > stream.len() {
        return None;
    }
    Some((desc, &stream[1..desc as usize], &stream[desc as usize..]))
}

/// Strip trailing 900 (latch TC) pad codewords from the data region.
pub fn strip_pads(data: &[u16]) -> &[u16] {
    let end = data.iter().rposition(|&v| v != 900).map_or(0, |p| p + 1);
    &data[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_cells_concatenates_valid_cells() {
        // start cell + a [2,3,2,3] cell + a [4,3] cell (7 modules)
        let mut runs: Vec<u32> = START.iter().map(|&v| v as u32).collect();
        runs.extend([2, 3, 2, 3]);
        runs.extend([4, 3]);
        let cells = split_cells(&runs, &[17, 27, 34]).unwrap();
        assert_eq!(cells[0], START.to_vec());
        assert_eq!(cells[1], vec![2, 3, 2, 3]);
        assert_eq!(cells[2], vec![4, 3]);
    }

    #[test]
    fn split_cells_rejects_when_a_run_would_cross() {
        // cells end light and start dark (4 bars + 4 spaces), so a run
        // crossing a boundary forces the next cell to start light
        let mut runs: Vec<u32> = START.iter().map(|&v| v as u32).collect();
        runs.extend([2, 3, 5, 4, 3]); // the 5-run spans x=22..27
        assert!(split_cells(&runs, &[17, 27, 34]).is_none());
    }

    #[test]
    fn strip_pads_trailing_900s() {
        assert_eq!(strip_pads(&[3, 28, 59, 900]), &[3, 28, 59]);
        assert_eq!(strip_pads(&[3, 28, 59]), &[3, 28, 59]);
        assert_eq!(strip_pads(&[900, 900]), &[] as &[u16]);
    }
}
