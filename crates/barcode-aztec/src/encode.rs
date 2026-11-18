//! Aztec encoder (ISO/IEC 24778). Every structural rule here is pinned
//! against zint renders — see docs/STATUS.md aztec blocks,
//! tests/aztec_derive.rs (stream layout + RS over raw windows) and the
//! text.rs oracle tests (window-relative stuff model, reproduced
//! bit-exactly for the stuffed oracles 'Code 128!' and 'é').
//!
//! Pipeline: mode-encode the payload into a bit stream, partition it into
//! b-bit codeword windows (b = 6/8/10/12 by layer count), pick the
//! smallest symbol whose capacity holds D data + K check codewords, RS-
//! encode the check words over GF(2^b), then paint: bullseye, core ring
//! (fixed dashes + the mode message), reference grid (full sizes) and the
//! spiral data path.

use crate::gf::field_for;
use crate::rs::rs_encode;
use crate::tables::{all_specs, Spec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// No symbol size can hold the payload.
    PayloadTooLarge,
}

// ---------------------------------------------------------------------------
// Mode state machine (ISO/IEC 24778 character sets, mirroring text.rs)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Upper,
    Lower,
    Mixed,
    Digit,
}

/// Single-code direct representation of `byte` in `mode` (Punct is only
/// ever entered by shift, so it is not a machine state here).
fn direct(mode: Mode, byte: u8) -> Option<(u16, usize)> {
    let w5 = 5usize;
    match mode {
        Mode::Upper => match byte {
            b'A'..=b'Z' => Some((2 + (byte - b'A') as u16, w5)),
            b' ' => Some((1, w5)),
            _ => None,
        },
        Mode::Lower => match byte {
            b'a'..=b'z' => Some((2 + (byte - b'a') as u16, w5)),
            b' ' => Some((1, w5)),
            _ => None,
        },
        Mode::Mixed => match byte {
            0x01..=0x0d => Some((byte as u16 + 1, w5)),
            0x1b..=0x1f => Some((15 + (byte - 0x1b) as u16, w5)),
            b'@' => Some((20, w5)),
            b'\\' => Some((21, w5)),
            b'^' => Some((22, w5)),
            b'_' => Some((23, w5)),
            b'`' => Some((24, w5)),
            b'|' => Some((25, w5)),
            b'~' => Some((26, w5)),
            0x7f => Some((27, w5)),
            b' ' => Some((1, w5)),
            _ => None,
        },
        Mode::Digit => match byte {
            b'0'..=b'9' => Some((2 + (byte - b'0') as u16, 4)),
            b',' => Some((12, 4)),
            b'.' => Some((13, 4)),
            b' ' => Some((1, 4)),
            _ => None,
        },
    }
}

/// Punct set (entered by P/S shift, code 0 in Upper/Lower/Mixed/Digit).
fn direct_punct(byte: u8) -> Option<u16> {
    const PUNCT: &[u8] = b"!\"#$%&'()*+,-./:;<=>?[]{}";
    if byte == b'\r' {
        return Some(1);
    }
    PUNCT.iter().position(|&p| p == byte).map(|i| 6 + i as u16)
}

/// Latch code `from` -> `to`, if a single latch exists.
fn latch(from: Mode, to: Mode) -> Option<u16> {
    match (from, to) {
        (Mode::Upper, Mode::Lower) => Some(28),
        (Mode::Upper, Mode::Mixed) => Some(29),
        (Mode::Upper, Mode::Digit) => Some(30),
        (Mode::Lower, Mode::Mixed) => Some(29),
        (Mode::Lower, Mode::Digit) => Some(30),
        (Mode::Mixed, Mode::Lower) => Some(28),
        (Mode::Mixed, Mode::Upper) => Some(29),
        (Mode::Digit, Mode::Upper) => Some(14),
        _ => None,
    }
}

/// Shortest latch path `from` -> `to` (the graph is tiny; BFS by hand via
/// repeated hops — all pairs are reachable within two latches).
fn latch_path(from: Mode, to: Mode) -> Vec<(u16, usize)> {
    let w = if from == Mode::Digit { 4 } else { 5 };
    if let Some(c) = latch(from, to) {
        return vec![(c, w)];
    }
    for mid in [Mode::Upper, Mode::Lower, Mode::Mixed, Mode::Digit] {
        if let (Some(a), Some(b)) = (latch(from, mid), latch(mid, to)) {
            return vec![(a, w), (b, 5)];
        }
    }
    unreachable!("all mode pairs are latch-reachable within two hops");
}

fn push_code(msg: &mut Vec<u8>, code: u16, width: usize) {
    for i in (0..width).rev() {
        msg.push((code >> i) as u8 & 1);
    }
}

/// Consecutive bytes from `i` with a direct code in `mode`.
fn run_len(text: &[u8], i: usize, mode: Mode) -> usize {
    text[i..]
        .iter()
        .take_while(|&&b| direct(mode, b).is_some())
        .count()
}

/// Encode the payload into the mode bit stream (greedy: direct codes,
/// shifts for single out-of-mode codes, latches for runs, B/S for bytes
/// no text mode carries).
fn mode_stream(text: &[u8]) -> Vec<u8> {
    let mut msg = Vec::new();
    let mut mode = Mode::Upper;
    let mut i = 0;
    while i < text.len() {
        let byte = text[i];
        if let Some((code, w)) = direct(mode, byte) {
            push_code(&mut msg, code, w);
            i += 1;
            continue;
        }
        // Punct-only byte: P/S shift covers one code from every state.
        if direct_punct(byte).is_some() && ![Mode::Upper, Mode::Lower, Mode::Mixed, Mode::Digit]
            .iter()
            .any(|&m| direct(m, byte).is_some())
        {
            let w = if mode == Mode::Digit { 4 } else { 5 };
            push_code(&mut msg, 0, w); // P/S
            push_code(&mut msg, direct_punct(byte).unwrap(), 5);
            i += 1;
            continue;
        }
        // Single out-of-mode code worth a shift: upper from Lower/Digit.
        if let Some((sc, w)) = match mode {
            Mode::Lower => Some((28u16, 5usize)),
            Mode::Digit => Some((15, 4)),
            _ => None,
        } {
            if direct(Mode::Upper, byte).is_some() && run_len(text, i, Mode::Upper) == 1 {
                push_code(&mut msg, sc, w); // U/S
                let (code, cw) = direct(Mode::Upper, byte).unwrap();
                push_code(&mut msg, code, cw);
                i += 1;
                continue;
            }
        }
        // Latch to the text mode with the longest direct run from here.
        let mut best: Option<(Mode, usize)> = None;
        for &m in &[Mode::Upper, Mode::Lower, Mode::Mixed, Mode::Digit] {
            if m == mode {
                continue;
            }
            let r = run_len(text, i, m);
            if r == 0 {
                continue;
            }
            if best.map(|(_, br)| r > br).unwrap_or(true) {
                best = Some((m, r));
            }
        }
        if let Some((m, _)) = best {
            for (code, w) in latch_path(mode, m) {
                push_code(&mut msg, code, w);
            }
            mode = m;
            let (code, w) = direct(mode, byte).unwrap();
            push_code(&mut msg, code, w);
            i += 1;
            continue;
        }
        // Binary run: bytes no text mode carries directly.
        let start = i;
        i += text[i..]
            .iter()
            .take_while(|&&b| {
                !direct(Mode::Upper, b).is_some()
                    && !direct(Mode::Lower, b).is_some()
                    && !direct(Mode::Mixed, b).is_some()
                    && !direct(Mode::Digit, b).is_some()
                    && direct_punct(b).is_none()
            })
            .count();
        let mut rest = &text[start..i];
        while !rest.is_empty() {
            // B/S from Punct would need U/S first, but Punct is never a
            // machine state here; every state reaches B/S with code 31.
            push_code(&mut msg, 31, 5);
            let n = rest.len().min(2078);
            if n < 32 {
                push_code(&mut msg, n as u16, 5);
            } else {
                push_code(&mut msg, 0, 5);
                push_code(&mut msg, (n - 31) as u16, 11);
            }
            for &b in &rest[..n] {
                push_code(&mut msg, b as u16, 8);
            }
            rest = &rest[n..];
        }
    }
    msg
}

// ---------------------------------------------------------------------------
// Codeword partition (zint-verified window-relative stuff model)
// ---------------------------------------------------------------------------

/// Partition the message bits into b-bit codeword windows. A non-final
/// window whose first b−1 bits are all equal carries the complementary
/// stuff bit as its b-th bit (the next message bit carries into the next
/// window); the final window is raw, padded with ones — an all-ones final
/// window has its last bit cleared. Bit-exact against zint for the
/// stuffed oracles (text.rs tests).
fn build_windows(msg: &[u8], b: usize) -> Vec<u16> {
    let mut windows: Vec<Vec<u8>> = Vec::new();
    let mut queue: Vec<u8> = msg.to_vec();
    while queue.len() > b {
        if queue[..b - 1].iter().all(|&x| x == queue[0]) {
            let mut w = queue[..b - 1].to_vec();
            w.push(1 - queue[0]);
            windows.push(w);
            queue.drain(..b - 1);
        } else {
            windows.push(queue[..b].to_vec());
            queue.drain(..b);
        }
    }
    while queue.len() < b {
        queue.push(1);
    }
    if queue.iter().all(|&x| x == 1) {
        let n = queue.len() - 1;
        queue[n] = 0;
    }
    windows.push(queue);
    windows
        .iter()
        .map(|w| w.iter().fold(0u16, |a, &x| (a << 1) | x as u16))
        .collect()
}

// ---------------------------------------------------------------------------
// Core ring layout (mirrors mode.rs read_mode exactly)
// ---------------------------------------------------------------------------

/// The core-ring perimeter, clockwise from the top-left corner.
fn ring(size: usize, half: usize) -> Vec<(usize, usize)> {
    let (lo, hi) = (size / 2 - half, size / 2 + half);
    let mut ring = Vec::with_capacity(8 * half);
    for x in lo..=hi {
        ring.push((lo, x));
    }
    for y in lo + 1..=hi {
        ring.push((y, hi));
    }
    for x in (lo..hi).rev() {
        ring.push((hi, x));
    }
    for y in (lo + 1..hi).rev() {
        ring.push((y, lo));
    }
    ring
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Encode into a specific symbol size (also the byte-diff oracle path).
pub fn encode_in(text: &[u8], spec: &'static Spec) -> Result<Vec<u8>, EncodeError> {
    let msg = mode_stream(text);
    let b = spec.bits as usize;
    let windows = build_windows(&msg, b);
    let d = windows.len() as u16;
    let ld = if spec.compact { 6 } else { 11 };
    if d as u32 > 1 << ld || d + 2 > spec.total {
        return Err(EncodeError::PayloadTooLarge);
    }
    let k = spec.total - d;

    // Stream: [skip zeros][data windows][check windows] == cap_bits.
    let cap = spec.cap_bits() as usize;
    let mut bits = vec![0u8; cap];
    let mut pos = spec.skip as usize;
    for &w in &windows {
        for i in (0..b).rev() {
            bits[pos] = (w >> i) as u8 & 1;
            pos += 1;
        }
    }
    let gf = field_for(spec.bits as u32);
    for &cw in rs_encode(&gf, &windows, k as usize).iter() {
        for i in (0..b).rev() {
            bits[pos] = (cw >> i) as u8 & 1;
            pos += 1;
        }
    }
    debug_assert_eq!(pos, cap);

    Ok(paint(spec, &bits, d))
}

/// Encode into the smallest symbol that holds the payload.
pub fn encode(text: &[u8]) -> Result<(&'static Spec, Vec<u8>), EncodeError> {
    for spec in all_specs() {
        // Cheap reject before building windows: the mode-message D field
        // bounds D, and K >= 2 keeps at least single-error correction.
        let ld = if spec.compact { 6 } else { 11 };
        let max_d = (1usize << ld).min(spec.total as usize - 2);
        if spec.total < 3 {
            continue;
        }
        let windows = build_windows(&mode_stream(text), spec.bits as usize);
        if windows.len() <= max_d {
            return Ok((spec, encode_in(text, spec).unwrap()));
        }
    }
    Err(EncodeError::PayloadTooLarge)
}

/// Paint the symbol matrix for the already-packed stream `bits`
/// (`cap_bits` long, in placement order).
fn paint(spec: &Spec, bits: &[u8], d: u16) -> Vec<u8> {
    let size = spec.rows as usize;
    let c = size / 2;
    let mut m = vec![0u8; size * size];

    // Bullseye: alternating rings, dark at even Chebyshev distance.
    let hb = (spec.bullseye() / 2) as i32;
    for dy in -hb..=hb {
        for dx in -hb..=hb {
            let dist = dy.abs().max(dx.abs());
            m[(c as i32 + dy) as usize * size + (c as i32 + dx) as usize] = (dist % 2 == 0) as u8;
        }
    }

    // Core ring: fixed dashes/corners + the mode message.
    let half = (spec.core() / 2) as usize;
    let per = if spec.compact { 10 } else { 14 };
    let ring = ring(size, half);

    // Mode message: (L-1, D-1) packed big-endian, then GF(16) check
    // nibbles — the exact inverse of read_mode's arbitration.
    let ld = if spec.compact { 6usize } else { 11 };
    let lw = if spec.compact { 2 } else { 4 };
    let ncheck = if spec.compact { 5 } else { 6 };
    let mw = ((spec.layers as u32 - 1) << ld) | (d as u32 - 1);
    let mut nibs: Vec<u16> = (0..lw).map(|i| ((mw >> (4 * (lw - 1 - i))) & 0xF) as u16).collect();
    nibs.extend(rs_encode(&field_for(4), &nibs, ncheck));
    let mut msg_bits = Vec::with_capacity(nibs.len() * 4);
    for &n in &nibs {
        for i in (0..4).rev() {
            msg_bits.push((n >> i) as u8 & 1);
        }
    }
    // Per side: compact 7 message bits at offsets 2..9, full 5+5 at
    // 2..7 and 8..13 (the offset-7 tick is fixed).
    let msg_offsets: Vec<usize> = if spec.compact {
        (2..9).collect()
    } else {
        (2..7).chain(8..13).collect()
    };
    let stride = if spec.compact { 7 } else { 10 };

    for side in 0..4 {
        let k = side * per;
        // fixed cells
        let fixed: &[(usize, u8)] = if spec.compact {
            match side {
                0 => &[(0, 1), (1, 1), (9, 0)],
                1 => &[(0, 1), (1, 1), (9, 1)],
                2 => &[(0, 0), (1, 0), (9, 0)],
                _ => &[(0, 0), (1, 0), (9, 1)],
            }
        } else {
            match side {
                0 => &[(0, 1), (1, 1), (7, 0), (13, 0)],
                1 => &[(0, 1), (1, 1), (7, 0), (13, 1)],
                2 => &[(0, 0), (1, 0), (7, 0), (13, 0)],
                _ => &[(0, 0), (1, 0), (7, 0), (13, 1)],
            }
        };
        for &(off, dark) in fixed {
            let (r, cc) = ring[k + off];
            m[r * size + cc] = dark;
        }
        // message bits (continuous stream across sides)
        for (j, &off) in msg_offsets.iter().enumerate() {
            let bit = msg_bits[side * stride + j];
            let (r, cc) = ring[k + off];
            m[r * size + cc] = bit;
        }
    }

    // Reference grid (full): every 16th final row/col outside the core,
    // dark where (row + col) is even.
    if !spec.compact {
        let (lo, hi) = (c - half, c + half);
        for r in 0..size {
            for cc in 0..size {
                let in_core = lo <= r && r <= hi && lo <= cc && cc <= hi;
                if !in_core && (r % 16 == c % 16 || cc % 16 == c % 16) {
                    m[r * size + cc] = ((r + cc) % 2 == 0) as u8;
                }
            }
        }
    }

    // Data path: stream bit k lands on path cell k.
    for (i, &(r, cc)) in crate::placement::data_path(spec).iter().enumerate() {
        m[r as usize * size + cc as usize] = bits[i];
    }
    m
}
