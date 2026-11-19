//! Clean-room GS1 DataBar decoder (GS1 DataBar Omnidirectional first).
//!
//! Verified symbol anatomy (see docs/databar-recon.md): 96 modules, 46 runs,
//! odd run indices ink, run 0 the leading paper module. Value chain:
//! `V = (((R4*1597 + R3)*2841 + R2)*1597 + R1)` with R1→pair3 (runs 23-30),
//! R2→S2 pair (runs 37-44), R3→pair2 (runs 15-22), R4→S1 pair (runs 1-9).
//! Six runs are always exactly 1 module wide (13, 14, 31, 32, 44, 45) —
//! a strong localization fingerprint. The finder triples (runs 10-12 /
//! 33-35) are redundant check carriers: validated optionally, never
//! required, for decode.
//!
//! Status: location/normalization complete; the residue→widths mapping
//! (`decode_omni`) lands with the oracle-extracted segment tables.

use barcode_core::{BitImage, DecodeResult, Point, Symbology, SymbologyDecoder};

const N_RUNS: usize = 46;
/// Modules spanned by runs 1..=45 (run 0 is the leading paper module m0).
const SYMBOL_MODULES: u32 = 95;
/// Runs that are exactly 1 module wide in every DataBar Omnidirectional.
const FIXED_ONES: [usize; 6] = [13, 14, 31, 32, 44, 45];

/// GS1 DataBar Omnidirectional decoder.
#[derive(Debug)]
pub struct DataBarOmniDecoder {
    min_rows: usize,
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    /// prefix[i] = sum of run widths before i; makes a window sum O(1).
    prefix: Vec<u32>,
    votes: std::collections::HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl DataBarOmniDecoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            runs: Vec::with_capacity(128),
            offsets: Vec::with_capacity(128),
            prefix: Vec::with_capacity(129),
            votes: std::collections::HashMap::new(),
        }
    }

    fn scan_row(&mut self, image: &BitImage, y: u32) {
        self.runs.clear();
        self.offsets.clear();
        let mut cur = false;
        let mut n = 0u32;
        let mut run_start = 0u32;
        let mut x = 0u32;
        while x < image.width {
            let ink = image.ink(x, y);
            if ink == cur {
                n += 1;
            } else {
                self.runs.push((cur, std::mem::replace(&mut n, 1)));
                self.offsets.push(run_start);
                run_start = x;
                cur = ink;
            }
            x += 1;
        }
        if n > 0 {
            self.runs.push((cur, n));
            self.offsets.push(image.width - n);
        }
        self.prefix.clear();
        self.prefix.push(0);
        let mut acc = 0u32;
        for &(_, w) in &self.runs {
            acc += w;
            self.prefix.push(acc);
        }
    }

    /// Normalize the 46-run window starting at `start` into module widths.
    ///
    /// Requires: window begins on a paper run, polarities alternate, and the
    /// pixel widths quantize cleanly to 95 modules with the six fixed
    /// 1-module runs in place. The unit is derived from the window itself
    /// (`sum(runs 1..) / 95`), so no module-size knowledge is needed.
    fn try_normalize(&self, start: usize) -> Option<[u32; N_RUNS]> {
        if start + N_RUNS > self.runs.len() || self.runs[start].0 {
            return None;
        }
        // Runs alternate by construction (`scan_row` only pushes on a
        // polarity change), so the paper start above already proves the
        // whole window alternates. The old per-run polarity loop was dead
        // work on every candidate.
        let s = self.prefix[start + N_RUNS] - self.prefix[start + 1];
        if s < SYMBOL_MODULES {
            return None; // not even 1 px/module
        }
        // Cheap rejection before quantizing all 45 runs, implied by the
        // final `mods[f] == 1` check: the six fixed 1-module runs must
        // round to exactly 1 module. This cannot reject a window the full
        // path would accept; it only skips the float loop for almost all
        // non-DataBar windows.
        let u = s as f32 / SYMBOL_MODULES as f32;
        for &f in &FIXED_ONES {
            let q = self.runs[start + f].1 as f32 / u;
            if q < 0.5 || q >= 1.5 {
                return None;
            }
        }
        let mut mods = [0u32; N_RUNS];
        mods[0] = 1;
        for i in 1..N_RUNS {
            let w = self.runs[start + i].1 as f32;
            if w < 0.5 * u {
                return None;
            }
            let m = (w / u).round() as i64;
            if m < 1 {
                return None;
            }
            mods[i] = m as u32;
        }
        if mods[1..].iter().sum::<u32>() != SYMBOL_MODULES {
            return None; // quantization drift: not a clean DataBar grid
        }
        for &f in &FIXED_ONES {
            if mods[f] != 1 {
                return None;
            }
        }
        Some(mods)
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::DataBar,
            text: text.to_string(),
            raw: text.as_bytes().to_vec(),
            position: [
                Point { x: x0 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y1 as f32 },
                Point { x: x0 as f32, y: y1 as f32 },
            ],
        }
    }

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        for y in 0..image.height {
            self.scan_row(image, y);
            if self.runs.len() < N_RUNS {
                continue;
            }
            debug_assert!(!self.runs[0].0, "scan_row must seed a paper run");
            // runs[0] is the leading paper run and polarity strictly
            // alternates, so paper starts are exactly the even indices.
            for start in (0..=(self.runs.len() - N_RUNS)).step_by(2) {
                let Some(mods) = self.try_normalize(start) else {
                    continue;
                };
                let Some(value) = decode_omni(&mods) else {
                    continue;
                };
                let x_start = self.offsets[start];
                let x_end = self.offsets[start + N_RUNS - 1]
                    + self.runs[start + N_RUNS - 1].1;
                let (count, x0, x1, y0, y1) = {
                    let e = self
                        .votes
                        .entry(value.clone())
                        .or_insert((0, x_start, x_end, y, y));
                    e.0 += 1;
                    e.1 = e.1.min(x_start);
                    e.2 = e.2.max(x_end);
                    e.3 = e.3.min(y);
                    e.4 = e.4.max(y);
                    *e
                };
                if count >= self.min_rows {
                    return Some(self.finish(&value, x0, x1, y0, y1));
                }
            }
        }
        self.votes
            .iter()
            .max_by_key(|(_, v)| v.0)
            .filter(|(_, v)| v.0 >= self.min_rows)
            .map(|(t, v)| self.finish(t, v.1, v.2, v.3, v.4))
    }
}

/// GS1 check digit (GTIN-14) over a 13-digit body: weights 3,1,… from the
/// left (rightmost body digit ×3), check = (10 − sum mod 10) mod 10.
fn gtin14_check(body: &[u8; 13]) -> u8 {
    let sum: usize = body
        .iter()
        .enumerate()
        .map(|(i, &d)| (d - b'0') as usize * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    ((10 - sum % 10) % 10) as u8
}

mod tables;
mod expanded;
mod expanded_ai;
mod expanded_tables;

pub use expanded::DataBarExpandedDecoder;

/// Binary search over a generated `(KEYS, INDEX)` pair: eight module widths
/// (1..=8) packed into one u32, most-significant nibble first. The tables
/// are sorted by key; `INDEX` holds the segment residue.
fn sorted_index(keys: &[u32], index: &[u16], w: &[u32]) -> Option<u32> {
    debug_assert_eq!(w.len(), 8);
    let mut key = 0u32;
    for &x in w {
        if x > 15 {
            return None; // not a module width; keeps keys collision-free
        }
        key = (key << 4) | x;
    }
    let mut lo = 0usize;
    let mut hi = keys.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if keys[mid] < key {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo < keys.len() && keys[lo] == key {
        Some(index[lo] as u32)
    } else {
        None
    }
}

/// Segment residue lookups over the oracle-extracted tables. The decoder
/// only needs pattern -> residue, so the generated tables are sorted packed
/// keys plus indices (binary search) instead of the encoder's width rows.
/// `None` = pattern not in table (reject the candidate).
fn s1_pair_value(w: &[u32]) -> Option<u32> {
    sorted_index(&tables::S1_KEYS, &tables::S1_INDEX, w)
}
fn pair2_value(w: &[u32]) -> Option<u32> {
    sorted_index(&tables::PAIR2_KEYS, &tables::PAIR2_INDEX, w)
}
fn pair3_value(w: &[u32]) -> Option<u32> {
    sorted_index(&tables::PAIR3_KEYS, &tables::PAIR3_INDEX, w)
}
fn s2_pair_value(w: &[u32]) -> Option<u32> {
    sorted_index(&tables::S2_KEYS, &tables::S2_INDEX, w)
}

/// Optional-but-cheap spec validation: the finder triples must equal the
/// ISO 24724 rule applied to the decoded element widths. Same windows as
/// the encoder's `finder_triples` (see tools/gen_databar.py).
fn finders_consistent(runs: &[u32; N_RUNS]) -> bool {
    const POW3: [u32; 32] = {
        let mut p = [1u32; 32];
        let mut i = 1;
        while i < 32 {
            p[i] = p[i - 1] * 3 % 79;
            i += 1;
        }
        p
    };
    let e = runs[2..10].iter().copied()
        .chain((15..23).rev().map(|i| runs[i]))
        .chain((36..44).rev().map(|i| runs[i]))
        .chain(runs[23..31].iter().copied());
    let c = e.enumerate().map(|(k, x)| POW3[k] * x).sum::<u32>() % 79;
    let raw = c + u32::from(c >= 8) + u32::from(c >= 72);
    let (cl, cr) = ((raw / 9) as usize, (raw % 9) as usize);
    let fl = tables::FINDER_T4[cl];
    let fr = [tables::FINDER_T4[cr][2], tables::FINDER_T4[cr][1], tables::FINDER_T4[cr][0]];
    runs[10..13].iter().zip(fl.iter()).all(|(a, &b)| *a == u32::from(b))
        && runs[33..36].iter().zip(fr.iter()).all(|(a, &b)| *a == u32::from(b))
}

/// Segment mapping: 46 module runs → GS1 AI(01) element string
/// (`(01)` + 13-digit GTIN body + recomputed GTIN-14 check digit).
fn decode_omni(runs: &[u32; N_RUNS]) -> Option<String> {
    if !finders_consistent(runs) {
        return None;
    }
    let Some(r1) = pair3_value(&runs[23..31]) else { return None };
    let Some(r2) = s2_pair_value(&runs[37..45]) else { return None };
    let Some(r3) = pair2_value(&runs[15..23]) else { return None };
    let Some(r4) = s1_pair_value(&runs[1..9]) else { return None };
    // Chain capacity (≈1.0006e13) slightly exceeds the 13-digit domain;
    // forged symbols above it are not DataBar values.
    let v = r1 as u64 + 1597 * (r2 as u64 + 2841 * (r3 as u64 + 1597 * r4 as u64));
    if v >= 10_000_000_000_000 {
        return None;
    }
    let body = format!("{v:013}");
    let check = gtin14_check(body.as_bytes().try_into().ok()?);
    Some(format!("(01){body}{check}"))
}

impl SymbologyDecoder for DataBarOmniDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::DataBar
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        self.decode_impl(image).into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// GS1 DataBar Stacked / Stacked Omnidirectional.
//
// Layout (docs/databar-recon.md, oracle-verified): 50 modules wide; the
// linear symbol's 96-module string is split with a 4-module overlap —
// top row = linear[0:50], bottom row = linear[46:96]. Runs 0..22 always
// end by m47 (complete in the top row); runs 23..45 always start at m48
// (complete in the bottom row). The stitch is therefore exact:
//   linear runs = top.runs[0..23] ++ bottom.runs[last-23..]
// and feeds the unchanged `decode_omni`. Separator rows are never read.
// ---------------------------------------------------------------------------

/// A normalized half-row: 23 consecutive linear runs in module widths.
/// Top half = runs 0..22 (m0..m47); bottom half = runs 23..45 (m48..m95).
#[derive(Debug, Clone)]
struct HalfRow {
    runs: [u32; 23],
    /// Pixel x of the half's content start (m0 for top, m48 for bottom).
    x0: f32,
    x1: f32,
    y: u32,
    unit: f32,
}

const HALF_RUNS: usize = 23;
/// Modules covered by a half's complete runs (m0..m47 / m48..m95).
const HALF_MODULES: u32 = 48;
/// Candidate caps per frame: pairing is O(tops*bots), both are tiny.
const MAX_HALVES: usize = 24;

/// GS1 DataBar Stacked decoder (handles both the 13-row stacked and the
/// 69-row stacked-omnidirectional layouts; scanlines cannot tell them
/// apart and don't need to).
#[derive(Debug)]
pub struct DataBarStackedDecoder {
    min_pairs: usize,
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    votes: std::collections::HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl DataBarStackedDecoder {
    pub fn new(min_pairs: usize) -> Self {
        Self {
            min_pairs: min_pairs.max(1),
            runs: Vec::with_capacity(64),
            offsets: Vec::with_capacity(64),
            votes: std::collections::HashMap::new(),
        }
    }

    fn scan_row(&mut self, image: &BitImage, y: u32) {
        self.runs.clear();
        self.offsets.clear();
        let mut cur = false;
        let mut n = 0u32;
        let mut run_start = 0u32;
        for x in 0..image.width {
            let ink = image.ink(x, y);
            if ink == cur {
                n += 1;
            } else {
                self.runs.push((cur, std::mem::replace(&mut n, 1)));
                self.offsets.push(run_start);
                run_start = x;
                cur = ink;
            }
        }
        if n > 0 {
            self.runs.push((cur, n));
            self.offsets.push(image.width - n);
        }
    }

    /// Quantize run pixel widths to modules; None unless every run rounds
    /// to >= 1 module and the total is exactly `total`.
    fn quantize(runs: &[(bool, u32)], unit: f32, total: u32) -> Option<Vec<u32>> {
        let mut mods = Vec::with_capacity(runs.len());
        let mut sum = 0u32;
        for &(_ink, w) in runs {
            let m = (w as f32 / unit).round() as i64;
            if m < 1 {
                return None;
            }
            sum += m as u32;
            mods.push(m as u32);
        }
        if sum != total {
            return None;
        }
        Some(mods)
    }

    /// Top half: a paper run followed by 22 alternating runs (m1..m47),
    /// the first of which is the 1-module left guard bar. The unit comes
    /// from the 47 content modules m1..m47 — the merged quiet+m0 leading
    /// paper is excluded on purpose (camera quiet zones are unbounded).
    fn top_half(&self, y: u32) -> Option<HalfRow> {
        let first_ink = self.runs.iter().position(|&(ink, _)| ink)?;
        // Window = [paper (m0 + merged quiet)] + 22 content runs (m1..m47).
        if first_ink == 0 || first_ink + HALF_RUNS - 1 > self.runs.len() {
            return None;
        }
        let win = &self.runs[first_ink - 1..first_ink - 1 + HALF_RUNS];
        // window[0] paper, then ink/paper alternating; window[1..] = m1..m47
        // = 47 modules ending with the paper run 22 at m47.
        if win[0].0 {
            return None;
        }
        for (k, w) in win.iter().enumerate() {
            if w.0 != (k % 2 == 1) {
                return None;
            }
        }
        let px: u32 = win[1..].iter().map(|&(_, w)| w).sum();
        if px < HALF_MODULES {
            return None;
        }
        let unit = px as f32 / (HALF_MODULES - 1) as f32;
        let mods = Self::quantize(&win[1..], unit, HALF_MODULES - 1)?;
        if mods[0] != 1 {
            return None; // the left guard bar is always exactly 1 module
        }
        // The pair3 bar starts at m48: the run after the window must be
        // ink and show at most 2 modules (m48, maybe m49) of it — and
        // nothing else may follow but that ink and quiet paper. Anything
        // more means this row is the OTHER band wearing a top disguise.
        let after = first_ink - 1 + HALF_RUNS;
        match self.runs.get(after) {
            Some(&(true, w)) if (w as f32 / unit) < 2.5 => {}
            _ => return None,
        }
        if self.runs.len() - after > 2 {
            return None;
        }
        if let Some(&(ink, _)) = self.runs.get(after + 1) {
            if ink {
                return None;
            }
        }
        let x0 = self.offsets[first_ink] as f32 - unit;
        let mut runs = [1u32; HALF_RUNS]; // run 0 = the leading paper m0
        runs[1..].copy_from_slice(&mods);
        Some(HalfRow {
            runs,
            x0,
            x1: x0 + HALF_MODULES as f32 * unit,
            y,
            unit,
        })
    }

    /// Bottom half: the 23 runs ending at the final 1-module bar (m95),
    /// i.e. linear runs 23..45 (m48..m95). Any partial prefix runs (the
    /// m46/m47 overlap view) are ignored.
    fn bottom_half(&self, y: u32) -> Option<HalfRow> {
        let mut end = self.runs.len();
        // The final bar is ink; a trailing quiet run may follow it.
        if self.runs.last()?.0 == false {
            end -= 1;
        }
        if end < HALF_RUNS {
            return None;
        }
        let win = &self.runs[end - HALF_RUNS..end];
        // 23 runs starting ink, alternating, ending ink (the final bar).
        for (k, w) in win.iter().enumerate() {
            if w.0 != (k % 2 == 0) {
                return None;
            }
        }
        let px: u32 = win.iter().map(|&(_, w)| w).sum();
        if px < HALF_MODULES {
            return None;
        }
        let unit = px as f32 / HALF_MODULES as f32;
        let mods = Self::quantize(win, unit, HALF_MODULES)?;
        if *mods.last()? != 1 {
            return None; // the final bar is always exactly 1 module
        }
        // The run before the window (m47, a space) must be paper — and
        // the whole prefix before the window is at most the 2 overlap
        // modules m46/m47 (plus possibly quiet merged into the first
        // run). More content means this row is the OTHER band.
        if end > HALF_RUNS {
            match self.runs.get(end - HALF_RUNS - 1) {
                Some(&(false, _)) => {}
                _ => return None,
            }
        }
        let prefix_px: u32 = self.runs[..end - HALF_RUNS]
            .iter()
            .skip(1)
            .map(|&(_, w)| w)
            .sum();
        if prefix_px as f32 > 2.5 * unit {
            return None;
        }
        let x0 = self.offsets[end - HALF_RUNS] as f32;
        Some(HalfRow {
            runs: {
                let mut r = [1u32; HALF_RUNS];
                r.copy_from_slice(&mods);
                r
            },
            x0,
            x1: x0 + HALF_MODULES as f32 * unit,
            y,
            unit,
        })
    }

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        let mut tops: Vec<HalfRow> = Vec::new();
        let mut bots: Vec<HalfRow> = Vec::new();
        for y in 0..image.height {
            self.scan_row(image, y);
            if self.runs.len() >= HALF_RUNS + 1 {
                // One entry per DISTINCT run pattern: every scanline through
                // a band yields the same half, and a band can also masquerade
                // as the other half (its last 1-module ink run looks like a
                // final bar) — deduping collapses hundreds of rows to the
                // few real patterns and keeps the pair loop tiny.
                if tops.len() < MAX_HALVES {
                    if let Some(h) = self.top_half(y) {
                        if !tops.iter().any(|e| e.runs == h.runs) {
                            tops.push(h);
                        }
                    }
                }
                if bots.len() < MAX_HALVES {
                    if let Some(h) = self.bottom_half(y) {
                        if !bots.iter().any(|e| e.runs == h.runs) {
                            bots.push(h);
                        }
                    }
                }
            }
        }
        for t in &tops {
            for b in &bots {
                if t.y >= b.y {
                    continue;
                }
                // Both halves come from the same printed module size.
                let du = (t.unit - b.unit).abs();
                if du > 0.2 * t.unit.min(b.unit) {
                    continue;
                }
                let mut runs = [0u32; N_RUNS];
                runs[..HALF_RUNS].copy_from_slice(&t.runs);
                runs[HALF_RUNS..].copy_from_slice(&b.runs);
                let Some(text) = decode_omni(&runs) else {
                    continue;
                };
                let x0 = t.x0.min(b.x0) as u32;
                let x1 = t.x1.max(b.x1) as u32;
                let (count, vx0, vx1, vy0, vy1) = {
                    let e = self
                        .votes
                        .entry(text.clone())
                        .or_insert((0, x0, x1, t.y, b.y));
                    e.0 += 1;
                    e.1 = e.1.min(x0);
                    e.2 = e.2.max(x1);
                    e.3 = e.3.min(t.y);
                    e.4 = e.4.max(b.y);
                    *e
                };
                if count >= self.min_pairs {
                    return Some(self.finish(&text, vx0, vx1, vy0, vy1));
                }
            }
        }
        let best = self
            .votes
            .iter()
            .max_by_key(|(_, v)| v.0)
            .filter(|(_, v)| v.0 >= self.min_pairs)?;
        Some(self.finish(best.0, best.1 .1, best.1 .2, best.1 .3, best.1 .4))
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::DataBarStacked,
            text: text.to_string(),
            raw: text.as_bytes().to_vec(),
            position: [
                Point { x: x0 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y1 as f32 },
                Point { x: x0 as f32, y: y1 as f32 },
            ],
        }
    }
}

impl SymbologyDecoder for DataBarStackedDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::DataBarStacked
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        self.decode_impl(image).into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// GS1 DataBar Limited.
//
// Layout (docs/databar-recon.md, oracle-verified): 74 modules, 46 runs —
// [paper 1][guard 1][DATA1 = runs 2-15, 26 mod][CHECK = runs 16-27, 16 mod]
// [space 1][bar 1][DATA2 = runs 30-43, 26 mod][space 1][bar 1]. Value chain
// V = D1·2013571 + D2, each D a (26,7) character; the check character is the
// mod-89 checksum Σ 3^k·e_k over the 28 data elements rendered through a
// fixed 89-entry width table. Runs 1, 28, 29, 44, 45 are always exactly
// 1 module wide — the localization fingerprint. Limited encodes 13-digit
// values with indicator digit 0/1 only, so V < 2·10^12.
// ---------------------------------------------------------------------------

mod ltd_tables;

/// Modules spanned by runs 1..=45 (run 0 is the leading paper module m0).
const LTD_SYMBOL_MODULES: u32 = 73;
/// Runs that are exactly 1 module wide in every DataBar Limited.
const LTD_FIXED_ONES: [usize; 5] = [1, 28, 29, 44, 45];

/// (spaces_sum, bars_sum, GSUM, spaces_widest, bars_widest) per group; the
/// seven group spans partition 0..2013571 exactly.
const LTD_GROUPS: [(u32, u32, u32, u32, u32); 7] = [
    (17, 9, 0, 6, 3),
    (13, 13, 183064, 5, 4),
    (9, 17, 820064, 3, 6),
    (15, 11, 1000776, 5, 4),
    (11, 15, 1491021, 4, 5),
    (19, 7, 1979845, 8, 1),
    (7, 19, 1996939, 1, 8),
];

/// The (26,7) character capacity: the D2 domain and D1's multiplier.
const LTD_M: u64 = 2_013_571;

/// Number of compositions of `t` into `k` parts each in 1..=w (k ≤ 7).
fn ltd_compositions(k: usize, t: u32, w: u32) -> u64 {
    let mut dp = vec![0u64; t as usize + 1];
    dp[0] = 1;
    for _ in 0..k {
        let mut next = vec![0u64; t as usize + 1];
        for s in 1..=t as usize {
            for p in 1..=w.min(s as u32) {
                next[s] += dp[s - p as usize];
            }
        }
        dp = next;
    }
    dp[t as usize]
}

/// Number of compositions of `t` into `k` parts in 1..=w with at least one
/// part equal to 1 (the noNarrow rule, applied to the bar subsets only).
fn ltd_compositions_single(k: usize, t: u32, w: u32) -> u64 {
    if t < k as u32 {
        return 0;
    }
    if w > 1 {
        ltd_compositions(k, t, w) - ltd_compositions(k, t - k as u32, w - 1)
    } else {
        u64::from(t == k as u32)
    }
}

/// Lex-ascending rank of a composition (the inverse of the spec's unrank).
fn ltd_rank(parts: &[u32; 7], w: u32, mut need_one: bool) -> u64 {
    let mut t: u32 = parts.iter().sum();
    let mut r = 0u64;
    for (i, &p) in parts.iter().enumerate() {
        let rest = 6 - i;
        for q in 1..p {
            r += if need_one && q != 1 {
                ltd_compositions_single(rest, t - q, w)
            } else {
                ltd_compositions(rest, t - q, w)
            };
        }
        t -= p;
        need_one = need_one && p != 1;
    }
    r
}

/// Value of a (26,7) character from its interleaved space/bar widths:
/// VD = GSUM + V_spaces·T_bars + V_bars. None unless the pattern belongs
/// to exactly one group and respects that group's width caps and noNarrow.
fn ltd_rank_char(sp: &[u32; 7], br: &[u32; 7]) -> Option<u32> {
    let ss: u32 = sp.iter().sum();
    let sb: u32 = br.iter().sum();
    let &(_, _, gsum, spw, brw) =
        LTD_GROUPS.iter().find(|&&(s, b, ..)| s == ss && b == sb)?;
    if sp.iter().any(|&w| w > spw) || br.iter().any(|&w| w > brw) {
        return None;
    }
    if !br.contains(&1) {
        return None; // bars must contain at least one single-module element
    }
    let tb = ltd_compositions_single(7, sb, brw);
    let v = u64::from(gsum) + ltd_rank(sp, spw, false) * tb + ltd_rank(br, brw, true);
    u32::try_from(v).ok()
}

/// c = Σ 3^k·e_k mod 89 over the 28 data elements (char1 then char2).
fn ltd_checksum(a: &[u32; 14], c: &[u32; 14]) -> usize {
    (a.iter()
        .chain(c.iter())
        .enumerate()
        .map(|(k, &e)| 3u64.pow(k as u32) * u64::from(e))
        .sum::<u64>()
        % 89) as usize
}

/// Segment mapping: 46 module runs → GS1 AI(01) element string. The check
/// character is validated against its checksum-derived width table (exact,
/// like the omni finder rule — a valid symbol always matches).
fn decode_ltd(runs: &[u32; 46]) -> Option<String> {
    let chars = |off: usize| {
        let mut sp = [0u32; 7];
        let mut br = [0u32; 7];
        for (i, (s, b)) in sp.iter_mut().zip(br.iter_mut()).enumerate() {
            *s = runs[off + 2 * i];
            *b = runs[off + 2 * i + 1];
        }
        (sp, br)
    };
    let (sp1, br1) = chars(2);
    let (sp2, br2) = chars(30);
    let d1 = ltd_rank_char(&sp1, &br1)?;
    let d2 = ltd_rank_char(&sp2, &br2)?;
    let interleave = |(sp, br): ([u32; 7], [u32; 7])| {
        let mut out = [0u32; 14];
        for i in 0..7 {
            out[2 * i] = sp[i];
            out[2 * i + 1] = br[i];
        }
        out
    };
    let a = interleave((sp1, br1));
    let c = interleave((sp2, br2));
    let chk = ltd_checksum(&a, &c);
    if runs[16..28]
        .iter()
        .zip(ltd_tables::LTD_B[chk])
        .any(|(&r, w)| r != u32::from(w))
    {
        return None;
    }
    let v = u64::from(d1) * LTD_M + u64::from(d2);
    // The (26,7) pair could address ~4·10^12; Limited symbols stop at
    // 2·10^12 (indicator digit 0/1). Anything above is not a Limited value.
    if v >= 2_000_000_000_000 {
        return None;
    }
    let body = format!("{v:013}");
    let check = gtin14_check(body.as_bytes().try_into().ok()?);
    Some(format!("(01){body}{check}"))
}

/// GS1 DataBar Limited decoder.
#[derive(Debug)]
pub struct DataBarLtdDecoder {
    min_rows: usize,
    runs: Vec<(bool, u32)>,
    offsets: Vec<u32>,
    /// prefix[i] = sum of run widths before i; makes a window sum O(1).
    prefix: Vec<u32>,
    votes: std::collections::HashMap<String, (usize, u32, u32, u32, u32)>,
}

impl DataBarLtdDecoder {
    pub fn new(min_rows: usize) -> Self {
        Self {
            min_rows: min_rows.max(1),
            runs: Vec::with_capacity(128),
            offsets: Vec::with_capacity(128),
            prefix: Vec::with_capacity(129),
            votes: std::collections::HashMap::new(),
        }
    }

    fn scan_row(&mut self, image: &BitImage, y: u32) {
        self.runs.clear();
        self.offsets.clear();
        let mut cur = false;
        let mut n = 0u32;
        let mut run_start = 0u32;
        for x in 0..image.width {
            let ink = image.ink(x, y);
            if ink == cur {
                n += 1;
            } else {
                self.runs.push((cur, std::mem::replace(&mut n, 1)));
                self.offsets.push(run_start);
                run_start = x;
                cur = ink;
            }
        }
        if n > 0 {
            self.runs.push((cur, n));
            self.offsets.push(image.width - n);
        }
        self.prefix.clear();
        self.prefix.push(0);
        let mut acc = 0u32;
        for &(_, w) in &self.runs {
            acc += w;
            self.prefix.push(acc);
        }
    }

    /// Normalize the 46-run window starting at `start` into module widths.
    /// Unit derived from the window itself (`sum(runs 1..) / 73`); the
    /// leading paper run is excluded (camera quiet zones are unbounded).
    fn try_normalize(&self, start: usize) -> Option<[u32; 46]> {
        if start + 46 > self.runs.len() || self.runs[start].0 {
            return None;
        }
        // Runs alternate by construction; see the omni decoder. The window
        // sum is a prefix-table difference, not a 45-run pass.
        let s = self.prefix[start + 46] - self.prefix[start + 1];
        if s < LTD_SYMBOL_MODULES {
            return None;
        }
        // Same implied prefilter: the five fixed 1-module runs must round
        // to exactly 1 module (enforced again by the final check).
        let u = s as f32 / LTD_SYMBOL_MODULES as f32;
        for &f in &LTD_FIXED_ONES {
            let q = self.runs[start + f].1 as f32 / u;
            if q < 0.5 || q >= 1.5 {
                return None;
            }
        }
        let mut mods = [0u32; 46];
        mods[0] = 1;
        for i in 1..46 {
            let w = self.runs[start + i].1 as f32;
            if w < 0.5 * u {
                return None;
            }
            let m = (w / u).round() as i64;
            if m < 1 {
                return None;
            }
            mods[i] = m as u32;
        }
        if mods[1..].iter().sum::<u32>() != LTD_SYMBOL_MODULES {
            return None;
        }
        for &f in &LTD_FIXED_ONES {
            if mods[f] != 1 {
                return None;
            }
        }
        Some(mods)
    }

    fn decode_impl(&mut self, image: &BitImage) -> Option<DecodeResult> {
        self.votes.clear();
        for y in 0..image.height {
            self.scan_row(image, y);
            if self.runs.len() < 46 {
                continue;
            }
            debug_assert!(!self.runs[0].0, "scan_row must seed a paper run");
            // Paper starts are the even indices (see the omni decoder).
            for start in (0..=(self.runs.len() - 46)).step_by(2) {
                let Some(mods) = self.try_normalize(start) else {
                    continue;
                };
                let Some(value) = decode_ltd(&mods) else {
                    continue;
                };
                let x_start = self.offsets[start];
                let x_end = self.offsets[start + 45] + self.runs[start + 45].1;
                let (count, x0, x1, y0, y1) = {
                    let e = self
                        .votes
                        .entry(value.clone())
                        .or_insert((0, x_start, x_end, y, y));
                    e.0 += 1;
                    e.1 = e.1.min(x_start);
                    e.2 = e.2.max(x_end);
                    e.3 = e.3.min(y);
                    e.4 = e.4.max(y);
                    *e
                };
                if count >= self.min_rows {
                    return Some(self.finish(&value, x0, x1, y0, y1));
                }
            }
        }
        self.votes
            .iter()
            .max_by_key(|(_, v)| v.0)
            .filter(|(_, v)| v.0 >= self.min_rows)
            .map(|(t, v)| self.finish(t, v.1, v.2, v.3, v.4))
    }

    fn finish(&self, text: &str, x0: u32, x1: u32, y0: u32, y1: u32) -> DecodeResult {
        DecodeResult {
            symbology: Symbology::DataBarLimited,
            text: text.to_string(),
            raw: text.as_bytes().to_vec(),
            position: [
                Point { x: x0 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y0 as f32 },
                Point { x: x1 as f32, y: y1 as f32 },
                Point { x: x0 as f32, y: y1 as f32 },
            ],
        }
    }
}

impl SymbologyDecoder for DataBarLtdDecoder {
    fn symbology(&self) -> Symbology {
        Symbology::DataBarLimited
    }

    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult> {
        self.decode_impl(image).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip through the stacked layouts (5+1+7 and 33+3+33 row
    /// bands). The content rows must also match the zint-derived split
    /// (top = linear[0:50], bottom = linear[46:96]) bit-exactly.
    #[test]
    fn stacked_roundtrip_and_oracle_geometry() {
        // zint BARCODE_RSS14STACK / _OMNI render of payload 4006381333931
        // (probe_stack2.py, /tmp/opencode/dbar): content rows of the 50-wide
        // stacked layout.
        const ZINT_TOP: &str = "01001000100001101100011100000001010100100000111010";
        const ZINT_BOT: &str = "10100111001101000101111111110001011001111010000101";

        for &omni in &[false, true] {
            let v = 4006381333931u64;
            let runs = barcode_fixtures::databar_omni_runs(v);
            let canvas = barcode_fixtures::databar_stacked_canvas(&runs, 4, 2, omni);
            let (expect_h, top_rows, sep_rows) =
                if omni { (69 * 4, 33, 3) } else { (13 * 4, 5, 1) };
            assert_eq!(canvas.height, expect_h);

            // Oracle geometry: row 0 of the top band and row 0 of the
            // bottom band, sampled inside the 50-module content.
            let bits = canvas.bits();
            let row_str = |y: u32| -> String {
                (0..50u32)
                    .map(|m| {
                        if BitImage::new(canvas.width, canvas.height, &bits)
                            .ink(2 * 4 + m * 4, y)
                        { '1' } else { '0' }
                    })
                    .collect()
            };
            assert_eq!(row_str(0), ZINT_TOP, "top band mismatch (omni={omni})");
            assert_eq!(row_str((top_rows + sep_rows) * 4), ZINT_BOT, "bottom band mismatch");

            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut d = DataBarStackedDecoder::new(1);
            let got = d.decode(&img).into_iter().next().unwrap_or_else(|| panic!("stacked decode failed (omni={omni})"));
            assert_eq!(got.symbology, Symbology::DataBarStacked);
            assert_eq!(got.text, "(01)40063813339314");
        }
    }

    /// DataBar Limited: round-trip through the canvas pipeline, oracle
    /// geometry (bit-exact vs zint BARCODE_RSS_LTD module strings), and
    /// checksum rejection of a tampered data element.
    #[test]
    fn databar_ltd_roundtrip_oracle_and_rejection() {
        // zint BARCODE_RSS_LTD module strings (probe_ltd.py, /tmp/opencode/dbar)
        const ZINT_1: &str =
            "01010101010100000010000001110100101101011001010101010101000000110000001101";
        const ZINT_MAX: &str =
            "01001111001101101011011111010101011010110001010100001011100011010111100101";

        let cases: [(u64, &str); 2] =
            [(1, ZINT_1), (1_999_999_999_999, ZINT_MAX)];
        for (v, oracle) in cases {
            let runs = barcode_fixtures::databar_ltd_runs(v);
            assert_eq!(runs.iter().sum::<u32>(), 74);
            let canvas = barcode_fixtures::databar_ltd_canvas(&runs, 4, 40, 10);

            // Oracle geometry: the content modules at module pitch 4.
            let bits = canvas.bits();
            let row: String = (0..74u32)
                .map(|m| {
                    if BitImage::new(canvas.width, canvas.height, &bits).ink(10 * 4 + m * 4, 0) {
                        '1'
                    } else {
                        '0'
                    }
                })
                .collect();
            assert_eq!(row, oracle, "module string mismatch for v={v}");

            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut d = DataBarLtdDecoder::new(2);
            let got = d.decode(&img).into_iter().next().expect("decode");
            assert_eq!(got.symbology, Symbology::DataBarLimited);
            let body = format!("{v:013}");
            let want = format!("(01){body}{}", gtin14_check(body.as_bytes().try_into().unwrap()));
            assert_eq!(got.text, want);
        }

        // Tamper: swap two widths inside char1 (sum-preserving) — the data
        // chars re-rank to a different value whose checksum no longer maps
        // to the check character, so the candidate must be rejected.
        let mut runs = barcode_fixtures::databar_ltd_runs(0);
        runs.swap(12, 13);
        let canvas = barcode_fixtures::databar_ltd_canvas(&runs, 4, 40, 10);
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = DataBarLtdDecoder::new(1);
        assert!(d.decode(&img).is_empty(), "tampered symbol must not decode");
    }

    /// A structurally valid 46-run symbol: sum 96, fixed 1-module runs in
    /// place (segment contents are placeholders until the tables land).
    fn valid_runs() -> [u32; N_RUNS] {
        let mut runs = [1u32; N_RUNS];
        // Distribute the remaining 50 modules over non-fixed runs.
        for &i in &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 15, 17, 19, 21,
                    23, 25, 27, 29, 33, 35, 37, 39, 41, 43] {
            runs[i] += 2;
        }
        runs
    }

    fn normalize_canvas(
        canvas: &barcode_fixtures::Canvas,
        start: usize,
    ) -> Option<[u32; N_RUNS]> {
        let bits = canvas.bits();
        let img = BitImage::new(canvas.width, canvas.height, &bits);
        let mut d = DataBarOmniDecoder::new(2);
        d.scan_row(&img, canvas.height / 2);
        d.try_normalize(start)
    }

    /// Like `databar_omni_canvas` but without the 96-module sum assert, for
    /// building deliberately corrupt symbols.
    fn draw_runs(runs: &[u32; 46], module: u32, height: u32, quiet: u32) -> barcode_fixtures::Canvas {
        let width = (96 + 2 * quiet) * module;
        let mut canvas = barcode_fixtures::Canvas::new(width, height);
        let mut x = quiet * module;
        for (i, &w) in runs.iter().enumerate() {
            if i % 2 == 1 {
                canvas.fill(x, 0, w * module, height);
            }
            x += w * module;
        }
        canvas
    }

    #[test]
    fn normalizes_a_clean_symbol() {
        let runs = valid_runs();
        let canvas = barcode_fixtures::databar_omni_canvas(&runs, 3, 40, 10);
        let mods = normalize_canvas(&canvas, 0).expect("should normalize");
        assert_eq!(mods, runs);
    }

    #[test]
    fn rejects_shifted_or_corrupt_windows() {
        let runs = valid_runs();

        // Sum != 96: not a DataBar grid.
        let mut bad = runs;
        bad[9] += 1;
        let canvas = draw_runs(&bad, 3, 40, 10);
        assert!(normalize_canvas(&canvas, 0).is_none());

        // Fixed 1-module run dilated: fingerprint broken.
        let mut bad = runs;
        bad[13] = 2;
        bad[9] -= 1;
        let canvas = draw_runs(&bad, 3, 40, 10);
        assert!(normalize_canvas(&canvas, 0).is_none());

        // Blank image: nothing to find.
        let canvas = barcode_fixtures::Canvas::new(200, 40);
        assert!(normalize_canvas(&canvas, 0).is_none());
    }

    #[test]
    fn round_trips_synthetic_symbols() {
        for v in [0u64, 1, 4006381333930, 123_456_789_012, 9_999_999_999_999] {
            let runs = barcode_fixtures::databar_omni_runs(v);
            let canvas = barcode_fixtures::databar_omni_canvas(&runs, 4, 60, 8);
            let bits = canvas.bits();
            let img = BitImage::new(canvas.width, canvas.height, &bits);
            let mut d = DataBarOmniDecoder::new(1);
            let got = d.decode(&img).into_iter().next().expect("decode");
            assert_eq!(got.symbology, Symbology::DataBar);
            // (01) + 13 digits + GTIN-14 check: the check must make the
            // 14-digit string valid under GS1 weights 3,1,… from the left.
            assert!(got.text.starts_with("(01)"));
            let digits: Vec<u8> = got.text[4..].bytes().map(|b| b - b'0').collect();
            assert_eq!(digits.len(), 14);
            let expect: Vec<u8> = format!("{v:013}").bytes().map(|b| b - b'0').collect();
            assert_eq!(&digits[..13], &expect[..]);
            let sum: usize = digits.iter().enumerate()
                .map(|(i, &x)| x as usize * if i % 2 == 0 { 3 } else { 1 }).sum();
            assert_eq!(sum % 10, 0, "bad GTIN-14 check for v={v}: {}", got.text);
        }
    }

    /// Every generated index table must be strictly sorted (binary search
    /// relies on it) and map back to its own residue. Covers all 7416 rows.
    #[test]
    fn segment_index_tables_round_trip() {
        for (keys, index) in [
            (&tables::S1_KEYS[..], &tables::S1_INDEX[..]),
            (&tables::PAIR2_KEYS[..], &tables::PAIR2_INDEX[..]),
            (&tables::PAIR3_KEYS[..], &tables::PAIR3_INDEX[..]),
            (&tables::S2_KEYS[..], &tables::S2_INDEX[..]),
        ] {
            assert_eq!(keys.len(), index.len());
            assert!(keys.windows(2).all(|w| w[0] < w[1]), "keys must be sorted");
            assert!(index.iter().all(|&i| (i as usize) < keys.len()));
        }

        let check = |v: u64| {
            let runs = barcode_fixtures::databar_omni_runs(v);
            let body = format!("{v:013}");
            let want = format!(
                "(01){body}{}",
                gtin14_check(body.as_bytes().try_into().unwrap())
            );
            assert_eq!(decode_omni(&runs), Some(want), "residue round-trip for v={v}");
        };
        for i in 0..tables::S1_KEYS.len() as u64 {
            check(i * 7_245_711_969);
        }
        for i in 0..tables::PAIR2_KEYS.len() as u64 {
            check(i * 4_537_077);
        }
        for i in 0..tables::PAIR3_KEYS.len() as u64 {
            check(i);
        }
        for i in 0..tables::S2_KEYS.len() as u64 {
            check(i * 1597);
        }
    }

    #[test]
    fn gtin14_check_matches_gs1_weights() {
        // 0401234567890 → sum 89 → check 1.
        let body: Vec<u8> = "0401234567890".bytes().collect();
        assert_eq!(gtin14_check(body.as_slice().try_into().unwrap()), 1);
        // Chain max representable value below the 13-digit guard:
        let v = 1596u64 + 1597 * (2840 + 2841 * (1596 + 1597 * 1380));
        assert!(v > 9_999_999_999_999); // decoder must reject forged ≥10^13
    }
}
