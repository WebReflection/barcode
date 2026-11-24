//! Shared contracts for the barcode decoder crates.
//!
//! The GPU pipeline (TypeScript, in `packages/web`) hands us a binarized
//! frame: 1 bit per pixel, packed 8 pixels per byte, rows aligned to 4 bytes.
//! Everything downstream of the GPU operates on [`BitImage`] and produces
//! [`DecodeResult`]s. Keeping this contract tiny and dependency-free means
//! the same decoders compile to `wasm32-unknown-unknown`,
//! `aarch64-apple-ios` and `aarch64-linux-android` unchanged.

/// Shared planar geometry (homography fit, module-grid sampling) used by
/// every 2D detector: a symbol is a module grid seen through a projective
/// map, so straightening is one implementation, not one per symbology.
pub mod geometry;

/// A corner of a located symbol, in image pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Quadrilateral covering a detected symbol, ordered
/// top-left, top-right, bottom-right, bottom-left in symbol space.
pub type Quad = [Point; 4];

/// The supported symbologies — exactly the JS API's 19 canonical names.
/// A variant must not be added without a decoder, an encoder and a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbology {
    // ---- Milestone 1 ----
    QRCode,
    DataMatrix,
    Code128,
    EAN13,
    // ---- Next ----
    QRCodeMicro,
    EAN8,
    UPCA,
    UPCE,
    Code39,
    /// Code 32 (Italian Pharmacode): a Code 39 payload convention used on
    /// Italian medicine boxes. The printed human-readable form is "A" + a
    /// nine-digit AIC code; the carrier is plain Code 39.
    Code32,
    Code93,
    Codabar,
    Itf,
    DataBar,
    DataBarStacked,
    DataBarExpanded,
    DataBarLimited,
    PDF417,
    Aztec,
}

impl Symbology {
    /// The canonical lowercase name. This is the one vocabulary: the
    /// `formats` API, the live scheduler, `checked` and decode results
    /// all use these names (`"qrcode"`, `"microqr"`, `"databarstk"`, ...).
    pub fn name(self) -> &'static str {
        match self {
            Symbology::QRCode => "qrcode",
            Symbology::DataMatrix => "datamatrix",
            Symbology::Code128 => "code128",
            Symbology::EAN13 => "ean13",
            Symbology::QRCodeMicro => "microqr",
            Symbology::EAN8 => "ean8",
            Symbology::UPCA => "upca",
            Symbology::UPCE => "upce",
            Symbology::Code39 => "code39",
            Symbology::Code32 => "code32",
            Symbology::Code93 => "code93",
            Symbology::Codabar => "codabar",
            Symbology::Itf => "itf",
            Symbology::DataBar => "databar",
            Symbology::DataBarStacked => "databarstk",
            Symbology::DataBarExpanded => "databarexpanded",
            Symbology::DataBarLimited => "databarltd",
            Symbology::PDF417 => "pdf417",
            Symbology::Aztec => "aztec",
        }
    }
}

/// A successfully decoded symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeResult {
    pub symbology: Symbology,
    /// Decoded payload. Structured/annotated payloads (e.g. GS1 AI strings)
    /// are returned raw here; structured parsing is a separate concern.
    pub text: String,
    /// Exact payload bytes. `text` is a display string — binary payloads
    /// (QR byte mode, Aztec, DataMatrix, PDF417) survive only here; there
    /// they are mapped to `text` Latin-1-style (byte → U+0000..U+00FF).
    pub raw: Vec<u8>,
    pub position: Quad,
}

/// Byte → `char` (Latin-1 style) display mapping: the stable text form
/// of a binary payload (byte → U+0000..U+00FF). `DecodeResult::raw`
/// carries the exact bytes; every symbology maps `text` with this.
pub fn payload_text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|e| e.into_bytes().into_iter().map(|b| b as char).collect())
}

/// A 1-bpp binarized image produced by the GPU pipeline.
///
/// Layout contract (must stay in sync with `packages/web/src/gpu/binarizer.ts`):
/// - bit at (x, y) is `data[y * bytes_per_row + (x >> 3)] >> (x & 7) & 1`
/// - `bytes_per_row` is a multiple of 4
/// - a set bit means *ink* (dark pixel)
#[derive(Debug, Clone, Copy)]
pub struct BitImage<'a> {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: usize,
    pub data: &'a [u8],
}

impl<'a> BitImage<'a> {
    /// # Panics
    /// If `data` is shorter than `height * bytes_per_row`.
    pub fn new(width: u32, height: u32, data: &'a [u8]) -> Self {
        let bytes_per_row = (width.div_ceil(8).div_ceil(4) * 4) as usize;
        assert!(data.len() >= height as usize * bytes_per_row);
        Self {
            width,
            height,
            bytes_per_row,
            data,
        }
    }

    #[inline]
    pub fn ink(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let idx = y as usize * self.bytes_per_row + (x >> 3) as usize;
        (self.data[idx] >> (x & 7)) & 1 == 1
    }

    /// Packed bits of the 180° rotation (same row stride). For stacked
    /// symbols an upside-down presentation swaps the bands AND reverses
    /// the runs — neither a horizontal mirror nor row-order tricks read
    /// that; rotating the bitmap does, with quad corners mapped back by
    /// the caller.
    pub fn rot180(&self) -> Vec<u8> {
        let bpr = self.bytes_per_row as usize;
        // 180° = mirror each row, then reverse the row order.
        let mut data = self.mirrored_x();
        let h = self.height as usize;
        for y in 0..h / 2 {
            let (a, b) = (y * bpr, (h - 1 - y) * bpr);
            for i in 0..bpr {
                data.swap(a + i, b + i);
            }
        }
        data
    }

    /// Packed bits of a horizontally flipped copy (same row stride). An
    /// upside-down linear barcode's run sequence is the reverse of the
    /// true one; decoding the mirror therefore yields the TRUE text with
    /// the check digit intact, and the caller mirrors the hit's x
    /// coordinates back. Cheap (bit shuffle) and symbology-agnostic — no
    /// decoder needs reverse-run support.
    pub fn mirrored_x(&self) -> Vec<u8> {
        let bpr = self.bytes_per_row as usize;
        let h = self.height as usize;
        let w = self.width as usize;
        let nbytes = w.div_ceil(8);
        let mut data = vec![0u8; bpr * h];
        // The fast path needs whole bytes: reversing a trailing partial
        // byte moves its padding holes into real pixel positions.
        if w % 8 == 0 {
            for y in 0..h {
                let src = &self.data[y * bpr..y * bpr + nbytes];
                let dst = &mut data[y * bpr..y * bpr + nbytes];
                mirror_row(src, dst);
            }
            return data;
        }
        for y in 0..h {
            for x in 0..w {
                if self.ink(x as u32, y as u32) {
                    let mx = w - 1 - x;
                    data[y * bpr + (mx >> 3)] |= 1 << (mx & 7);
                }
            }
        }
        data
    }

    /// Packed bits of a 90° clockwise rotation (`dw = height`, `dh = width`,
    /// `dst(height - 1 - y, x) = src(x, y)`). Processes 8×8 bit blocks via
    /// SWAR transpose (12 bitwise ops per 64 pixels), with scalar fallback
    /// for edge strips when dimensions are not multiples of 8.
    pub fn rot90_cw(&self) -> Vec<u8> {
        let w = self.width as usize;
        let h = self.height as usize;
        let dbpr = h.div_ceil(8).div_ceil(4) * 4;
        let mut out = vec![0u8; dbpr * w];
        let bpr = self.bytes_per_row;
        let full_bx = w / 8;
        let full_by = h / 8;

        // Fast 8x8 SWAR bit-block transpose: covers all complete 8x8 blocks
        // aligned to destination bytes (dst_x = 0, 8, ... from src_y = h-1 down).
        for dx_blk in 0..full_by {
            let dx_base = dx_blk * 8;
            for dy_blk in 0..full_bx {
                let dy_base = dy_blk * 8;
                // Pack 8 source bytes from rows (h-1-dx_base) down to (h-8-dx_base)
                // at source byte column `dy_blk` into a u64 (byte r = row r).
                let mut x = 0u64;
                for r in 0..8 {
                    let sy = h - 1 - (dx_base + r);
                    x |= u64::from(self.data[sy * bpr + dy_blk]) << (r * 8);
                }
                let t = transpose8x8(x);
                for c in 0..8 {
                    out[(dy_base + c) * dbpr + dx_blk] = (t >> (c * 8)) as u8;
                }
            }
        }

        // Scalar tail for bottom strip (src_y < h % 8, i.e. dst_x >= full_by*8)
        let rem_h = h - full_by * 8;
        if rem_h > 0 {
            for y in 0..rem_h {
                let dx = h - 1 - y;
                for x in 0..w {
                    if self.ink(x as u32, y as u32) {
                        out[x * dbpr + (dx >> 3)] |= 1 << (dx & 7);
                    }
                }
            }
        }
        // Scalar tail for right strip (src_x >= full_bx*8) over the remaining rows
        let rem_x_start = full_bx * 8;
        if rem_x_start < w {
            for y in rem_h..h {
                let dx = h - 1 - y;
                for x in rem_x_start..w {
                    if self.ink(x as u32, y as u32) {
                        out[x * dbpr + (dx >> 3)] |= 1 << (dx & 7);
                    }
                }
            }
        }
        out
    }

    /// Packed bits of the bitmap rotated by `deg` degrees about the frame
    /// center (nearest neighbor, same dimensions): content appears rotated
    /// by +deg. Vehicle of the tilt-retry tier — the axis-aligned 2D
    /// locators (datamatrix/aztec/microqr) miss even mild in-plane roll,
    /// so an empty primary sweep gets a second chance on an uprighted
    /// mask. The caller maps returned quads back by rotating them -deg
    /// about the center.
    pub fn rotated(&self, deg: f64) -> Vec<u8> {
        let bpr = self.bytes_per_row as usize;
        let mut data = vec![0u8; bpr * self.height as usize];
        let (cx, cy) = (f64::from(self.width) / 2.0, f64::from(self.height) / 2.0);
        let (sin, cos) = deg.to_radians().sin_cos();
        // Dest-driven inverse mapping (no gaps): dest pixel p samples the
        // source at R(-deg) * (p - center) + center.
        for dy in 0..self.height as usize {
            for dx in 0..self.width as usize {
                let (x, y) = (dx as f64 - cx, dy as f64 - cy);
                let sx = (cos * x + sin * y + cx) as isize;
                let sy = (-sin * x + cos * y + cy) as isize;
                if sx >= 0
                    && sy >= 0
                    && (sx as u32) < self.width
                    && (sy as u32) < self.height
                    && self.ink(sx as u32, sy as u32)
                {
                    data[dy * bpr + (dx >> 3)] |= 1 << (dx & 7);
                }
            }
        }
        data
    }

    /// Iterate the run lengths of ink/background along a row, starting with
    /// a background run (possibly zero). The workhorse of every 1D decoder.
    /// Note: lazy — the trailing run is only emitted if the caller drains
    /// the iterator.
    pub fn row_runs(&self, y: u32) -> impl Iterator<Item = (bool, u32)> + '_ {
        let mut x = 0u32;
        let mut cur = false; // contract: runs start with background (possibly zero-length)
        let mut n = 0u32; // pixels of the current run counted so far
        std::iter::from_fn(move || {
            while x < self.width {
                let ink = self.ink(x, y);
                if ink == cur {
                    n += 1;
                    x += 1;
                } else {
                    // x belongs to the new run; it gets counted next call.
                    let out = (cur, std::mem::replace(&mut n, 0));
                    cur = ink;
                    return Some(out);
                }
            }
            (n > 0).then(|| (cur, std::mem::replace(&mut n, 0)))
        })
    }
}

/// Reverse one packed row: `dst[i] = src[n-1-i].reverse_bits()`. Only used
/// when the row width is a whole number of bytes, so the byte reversal
/// and the bit reversal cannot move padding bits into pixel positions.
///
/// The wasm arm opts THIS function into SIMD (16 bytes at a time) instead
/// of enabling `simd128` for the whole crate: a global target feature
/// lets LLVM re-vectorize the branchy run scanners too, which measured
/// slower than scalar. `#[target_feature]` keeps the opt-in local.
#[inline]
fn mirror_row(src: &[u8], dst: &mut [u8]) {
    #[cfg(target_arch = "wasm32")]
    {
        // The SDK's browser floor is WebGPU-capable engines, all of which
        // ship wasm SIMD; the opt-in is local to this function.
        mirror_row_simd(src, dst);
    }
    #[cfg(not(target_arch = "wasm32"))]
    for i in 0..src.len() {
        dst[src.len() - 1 - i] = src[i].reverse_bits();
    }
}

#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
fn mirror_row_simd(src: &[u8], dst: &mut [u8]) {
    use core::arch::wasm32::v128;
    let n = src.len();
    let mut i = 0usize;
    let mut j = n;
    while j >= i + 16 {
        // SAFETY: i and j-16 are in-bounds 16-byte blocks: the loop
        // keeps i <= j-16 and j <= n == src.len() == dst.len().
        let (ra, rb) = unsafe {
            let a = bitrev16(core::ptr::read_unaligned(src.as_ptr().add(i) as *const v128));
            let b = bitrev16(core::ptr::read_unaligned(
                src.as_ptr().add(j - 16) as *const v128
            ));
            // Reverse the byte order inside each 16-byte chunk as well:
            // a byte-reversed chunk is the mirrored row chunk.
            (u8x16_swizzle_reverse(a), u8x16_swizzle_reverse(b))
        };
        unsafe {
            core::ptr::write_unaligned(dst.as_mut_ptr().add(i) as *mut v128, rb);
            core::ptr::write_unaligned(dst.as_mut_ptr().add(j - 16) as *mut v128, ra);
        }
        i += 16;
        j -= 16;
    }
    // The unpaired middle (fewer than 16 bytes) still maps by the global
    // mirror index, not by an in-place swap.
    for k in i..j {
        dst[n - 1 - k] = src[k].reverse_bits();
    }
}

/// Reverse the bits of every byte in a 16-byte vector.
#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
#[inline]
fn bitrev16(v: core::arch::wasm32::v128) -> core::arch::wasm32::v128 {
    use core::arch::wasm32::*;
    // Swap nibbles, then pairs, then adjacent bits (the classic Hacker's
    // Delight sequence), all 16 bytes at once.
    let a = v128_or(
        v128_and(u8x16_shl(v, 4), u8x16_splat(0xf0)),
        v128_and(u8x16_shr(v, 4), u8x16_splat(0x0f)),
    );
    let b = v128_or(
        v128_and(u8x16_shl(a, 2), u8x16_splat(0xcc)),
        v128_and(u8x16_shr(a, 2), u8x16_splat(0x33)),
    );
    v128_or(
        v128_and(u8x16_shl(b, 1), u8x16_splat(0xaa)),
        v128_and(u8x16_shr(b, 1), u8x16_splat(0x55)),
    )
}

/// Reverse the order of the 16 bytes in a vector.
#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
#[inline]
fn u8x16_swizzle_reverse(v: core::arch::wasm32::v128) -> core::arch::wasm32::v128 {
    use core::arch::wasm32::*;
    i8x16_shuffle::<15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0>(v, v)
}

/// Transpose an 8×8 bit matrix packed as 8 little-endian rows in a `u64`:
/// input bit `(r, c)` at `8*r + c` moves to output bit `8*c + r`.
#[inline]
fn transpose8x8(mut x: u64) -> u64 {
    let mut t = (x ^ (x >> 7)) & 0x00AA_00AA_00AA_00AA;
    x ^= t ^ (t << 7);
    t = (x ^ (x >> 14)) & 0x0000_CCCC_0000_CCCC;
    x ^= t ^ (t << 14);
    t = (x ^ (x >> 28)) & 0x0000_0000_F0F0_F0F0;
    x ^= t ^ (t << 28);
    x
}

/// Extracted alternating `(ink, length)` runs of a single scanline, shared
/// across all active 1D decoders so pixel-level bit scanning runs once per
/// scanline instead of once per decoder per orientation.
///
/// Invariants:
/// - When `width > 0`, `runs[0]` is ALWAYS `(false, len)` (possibly 0 if
///   pixel 0 is ink), and polarities strictly alternate (`runs[i].0 == (i % 2 == 1)`).
/// - `offsets[i]` is the x coordinate where `runs[i]` begins.
/// - `prefix[i]` is the sum of `runs[0..i].1` (`prefix.len() == runs.len() + 1`).
/// - `last_ink_run` is the index of the last ink run (or `0` if none).
#[derive(Debug, Default, Clone)]
pub struct RowRuns {
    pub width: u32,
    pub runs: Vec<(bool, u32)>,
    pub offsets: Vec<u32>,
    pub prefix: Vec<u32>,
    pub last_ink_run: usize,
}

impl RowRuns {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            width: 0,
            runs: Vec::with_capacity(cap),
            offsets: Vec::with_capacity(cap),
            prefix: Vec::with_capacity(cap + 1),
            last_ink_run: 0,
        }
    }

    /// Extract the alternating runs of row `y` using 32-bit word scanning
    /// (`trailing_zeros`), skipping up to 32 identical pixels per iteration.
    /// Bit-for-bit identical to pixel-by-pixel scanning.
    pub fn scan_row(&mut self, img: &BitImage, y: u32) {
        self.runs.clear();
        self.offsets.clear();
        self.prefix.clear();
        self.prefix.push(0);
        self.width = img.width;
        self.last_ink_run = 0;

        let w = img.width;
        if w == 0 || y >= img.height {
            return;
        }
        let row = &img.data[y as usize * img.bytes_per_row..];
        let mut cur = false; // invariant: first run is always background (possibly 0)
        let mut run_start = 0u32;
        let mut x = 0u32;

        // `bytes_per_row` is a multiple of 4, so every word containing pixels
        // in `0..w` lies fully within `row`.
        let nwords = (w as usize).div_ceil(32);
        for wi in 0..nwords {
            let word_base = (wi as u32) << 5;
            let word_end = (word_base + 32).min(w);
            let raw = u32::from_le_bytes([
                row[wi * 4],
                row[wi * 4 + 1],
                row[wi * 4 + 2],
                row[wi * 4 + 3],
            ]);
            // Fast path: whole 32-pixel word matches `cur` (no transitions).
            if word_end - word_base == 32 && ((cur && raw == u32::MAX) || (!cur && raw == 0)) {
                x = word_end;
                continue;
            }
            while x < word_end {
                let bit_off = x - word_base;
                let rem = word_end - x;
                let bits = (if cur { !raw } else { raw }) >> bit_off;
                let matched = if bits == 0 {
                    rem
                } else {
                    bits.trailing_zeros().min(rem)
                };
                x += matched;
                if matched < rem {
                    let len = x - run_start;
                    if cur {
                        self.last_ink_run = self.runs.len();
                    }
                    self.runs.push((cur, len));
                    self.offsets.push(run_start);
                    self.prefix.push(x);
                    run_start = x;
                    cur = !cur;
                }
            }
        }
        if w > run_start {
            let len = w - run_start;
            if cur {
                self.last_ink_run = self.runs.len();
            }
            self.runs.push((cur, len));
            self.offsets.push(run_start);
            self.prefix.push(w);
        }
    }

    /// Populate `dst` with the exact horizontal mirror of `self` (as if
    /// `scan_row` had run on `img.mirrored_x()`), with zero bitmap copies.
    pub fn reverse_into(&self, dst: &mut RowRuns) {
        dst.runs.clear();
        dst.offsets.clear();
        dst.prefix.clear();
        dst.prefix.push(0);
        dst.width = self.width;
        dst.last_ink_run = 0;

        if self.runs.is_empty() || self.width == 0 {
            return;
        }
        // Drop a leading zero-length paper sentinel: after reversing it would
        // sit at the right edge where pixel-by-pixel scanning never emits one.
        let slice = if self.runs[0] == (false, 0) {
            &self.runs[1..]
        } else {
            &self.runs[..]
        };
        if slice.is_empty() {
            return;
        }
        // If the mirrored row starts with ink, seed the leading `(false, 0)`
        // sentinel so `dst.runs[0]` keeps the background-first invariant.
        if slice.last().is_some_and(|&(ink, _)| ink) {
            dst.runs.push((false, 0));
            dst.offsets.push(0);
            dst.prefix.push(0);
        }
        let mut acc = 0u32;
        for &(ink, len) in slice.iter().rev() {
            if ink {
                dst.last_ink_run = dst.runs.len();
            }
            dst.runs.push((ink, len));
            dst.offsets.push(acc);
            acc += len;
            dst.prefix.push(acc);
        }
    }
}

/// Pick the row-vote winner under a PLURALITY rule: the value needs at
/// least `min_rows` agreeing rows and must agree on MORE rows than any
/// other candidate. `count` reads the vote count out of the stored value.
///
/// Why not simply `max_by_key`: the human-readable digits printed under a
/// 1D symbol binarize into a shifted, still check-valid window and can
/// win a few rows near the frame center (where the center-out row scan
/// starts). Measured on a real product box: `9182165783071` on 7 rows
/// under `0197105783072` on 28 rows — a strict plurality rejects the
/// decoy without weakening the check digit.
pub fn plurality<'a, K, V>(
    votes: &'a std::collections::HashMap<K, V>,
    count: impl Fn(&V) -> usize,
    min_rows: usize,
) -> Option<(&'a K, &'a V)> {
    let mut best: Option<(&'a K, &'a V)> = None;
    let mut second = 0usize;
    for (key, value) in votes {
        let votes_n = count(value);
        if votes_n < min_rows {
            continue;
        }
        match best {
            Some((_, best_value)) if count(best_value) >= votes_n => {
                second = second.max(votes_n);
            }
            Some((_, best_value)) => {
                second = second.max(count(best_value));
                best = Some((key, value));
            }
            None => best = Some((key, value)),
        }
    }
    best.filter(|(_, value)| count(value) > second)
}

/// True when a 1D hit's horizontal span covers (or is within 4% / 8px of)
/// the frame's horizontal center. Used to gate fast early-return in
/// `LinearDecoder::scan_line`: a centered exact hit stops scanning rows
/// immediately (2 rows, sub-ms), while a peripheral hit keeps scanning so
/// a centered barcode of the same symbology on the same or nearby rows is
/// never starved.
#[inline]
pub fn x_contains_center(x_start: u32, x_end: u32, width: u32) -> bool {
    if width == 0 {
        return true;
    }
    let cx = width / 2;
    let tol = (width / 25).max(8);
    cx.saturating_add(tol) >= x_start && cx <= x_end.saturating_add(tol)
}

/// A symbology decoder. Implementations must be allocation-frugal on the hot
/// path (reuse scratch buffers internally) and must never panic on hostile
/// input: barcodes come from an untrusted camera pointed at the world.
pub trait SymbologyDecoder {
    fn symbology(&self) -> Symbology;
    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult>;
}

/// Row-oriented 1D decoder interface. Allows callers (e.g. `barcode-wasm`) to
/// extract each scanline's [`RowRuns`] ONCE and feed the same slice to every
/// active 1D decoder — and to reverse runs in-place for 180° / 270° without
/// copying bitmaps.
pub trait LinearDecoder: SymbologyDecoder {
    fn reset(&mut self);
    fn scan_line(&mut self, row: &RowRuns, y: u32) -> Option<DecodeResult>;
    fn finish_votes(&self) -> Option<DecodeResult>;
    /// All distinct validated candidates collected during the scan. When a
    /// frame contains multiple symbols of the same symbology (e.g. an EAN-13
    /// and a UPC-A on the same box label) and none triggered centered early
    /// exit, returning every validated candidate lets the ranker pick the
    /// centered one instead of whichever symbol happened to be tallest.
    fn finish_all(&self) -> Vec<DecodeResult> {
        self.finish_votes().into_iter().collect()
    }
    /// Total votes accumulated since `reset()` (0 in O(1) when empty).
    /// Used by strided live scanning to trigger immediate adjacent-row
    /// (`y ± 1`) corroboration the moment any row records a vote.
    fn vote_count(&self) -> usize;
}

/// Run a single [`LinearDecoder`] over every row of `image` using the shared
/// word-level [`RowRuns`] extractor.
pub fn decode_linear_single<D: LinearDecoder>(dec: &mut D, image: &BitImage) -> Vec<DecodeResult> {
    let mut row = RowRuns::with_capacity(256);
    dec.reset();
    for y in 0..image.height {
        row.scan_row(image, y);
        if let Some(hit) = dec.scan_line(&row, y) {
            return vec![hit];
        }
    }
    dec.finish_votes().into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_layout_matches_gpu_contract() {
        // 33 pixels wide -> ceil(33/8)=5 bytes -> padded to 8.
        let mut data = vec![0u8; 2 * 8];
        // Set (7,0) and (32,1) before borrowing the buffer.
        data[0] |= 1 << 7;
        data[1 * 8 + 4] |= 1 << 0;
        let img = BitImage::new(33, 2, &data);
        assert_eq!(img.bytes_per_row, 8);
        assert!(img.ink(7, 0));
        assert!(!img.ink(6, 0));
        assert!(img.ink(32, 1));
        assert!(!img.ink(32, 0));
    }

    /// Pixel-by-pixel reference implementations for the packed transforms.
    fn reference_mirror(img: &BitImage) -> Vec<u8> {
        let mut data = vec![0u8; img.bytes_per_row * img.height as usize];
        for y in 0..img.height {
            for x in 0..img.width {
                if img.ink(x, y) {
                    let mx = img.width - 1 - x;
                    data[y as usize * img.bytes_per_row + (mx >> 3) as usize] |= 1 << (mx & 7);
                }
            }
        }
        data
    }

    fn reference_rot180(img: &BitImage) -> Vec<u8> {
        let mut data = vec![0u8; img.bytes_per_row * img.height as usize];
        for y in 0..img.height {
            for x in 0..img.width {
                if img.ink(x, y) {
                    let (mx, my) = (img.width - 1 - x, img.height - 1 - y);
                    data[my as usize * img.bytes_per_row + (mx >> 3) as usize] |= 1 << (mx & 7);
                }
            }
        }
        data
    }

    #[test]
    fn packed_transforms_match_pixel_reference() {
        // Whole-byte widths take the bit-parallel path; 13/33 keep the
        // partial-byte fallback honest. Tall enough for 16-byte chunks.
        for &(w, h) in &[
            (8u32, 1u32),
            (13, 3),
            (33, 5),
            (64, 16),
            (576, 9),
            (1056, 2),
        ] {
            let bpr = (w.div_ceil(8).div_ceil(4) * 4) as usize;
            let mut data = vec![0u8; bpr * h as usize];
            let mut seed = 0x9e3779b9u32 ^ w;
            for y in 0..h as usize {
                for x in 0..w {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    if seed & 0x40 != 0 {
                        data[y * bpr + (x >> 3) as usize] |= 1 << (x & 7);
                    }
                }
            }
            let img = BitImage::new(w, h, &data);
            assert_eq!(img.mirrored_x(), reference_mirror(&img), "mirror {w}x{h}");
            assert_eq!(img.rot180(), reference_rot180(&img), "rot180 {w}x{h}");
        }
    }

    #[test]
    fn row_runs_alternates() {
        // 8 px: bg 0, ink 2, bg 1, ink 3, bg 2
        // bits 0,1 ink; bit 2 bg; bits 3,4,5 ink; bits 6,7 bg -> 0b0011_1011
        let data = [0b0011_1011, 0, 0, 0];
        let img = BitImage::new(8, 1, &data);
        let runs: Vec<_> = img.row_runs(0).collect();
        assert_eq!(
            runs,
            vec![(false, 0), (true, 2), (false, 1), (true, 3), (false, 2)]
        );
    }

    fn reference_rot90_cw(img: &BitImage) -> Vec<u8> {
        let w = img.width as usize;
        let h = img.height as usize;
        let dbpr = h.div_ceil(8).div_ceil(4) * 4;
        let mut out = vec![0u8; dbpr * w];
        for y in 0..h {
            for x in 0..w {
                if img.ink(x as u32, y as u32) {
                    let dx = h - 1 - y;
                    out[x * dbpr + (dx >> 3)] |= 1 << (dx & 7);
                }
            }
        }
        out
    }

    #[test]
    fn word_row_runs_reverse_and_rot90_match_reference() {
        for &(w, h) in &[
            (1u32, 1u32),
            (7, 3),
            (8, 8),
            (13, 5),
            (31, 9),
            (32, 16),
            (33, 17),
            (64, 32),
            (105, 41),
            (576, 64),
        ] {
            let bpr = (w.div_ceil(8).div_ceil(4) * 4) as usize;
            let mut data = vec![0u8; bpr * h as usize];
            let mut seed = 0x9e3779b9u32 ^ (w << 8) ^ h;
            for y in 0..h as usize {
                for x in 0..w {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    // Cover all-0, all-1, and mixed rows.
                    let ink = match y {
                        0 => false,
                        1 if h > 2 => true,
                        _ => (seed & 0x3) == 0,
                    };
                    if ink {
                        data[y * bpr + (x >> 3) as usize] |= 1 << (x & 7);
                    }
                }
            }
            let img = BitImage::new(w, h, &data);
            assert_eq!(img.rot90_cw(), reference_rot90_cw(&img), "rot90_cw {w}x{h}");

            let mirrored = img.mirrored_x();
            let mimg = BitImage::new(w, h, &mirrored);
            let mut row = RowRuns::with_capacity(64);
            let mut rev = RowRuns::with_capacity(64);
            let mut mrow = RowRuns::with_capacity(64);
            for y in 0..h {
                row.scan_row(&img, y);
                let expected: Vec<_> = img.row_runs(y).collect();
                assert_eq!(row.runs, expected, "scan_row {w}x{h} y={y}");
                // Check offsets, prefix, last_ink_run invariants.
                let mut acc = 0u32;
                let mut last_ink = 0usize;
                assert_eq!(row.prefix.len(), row.runs.len() + 1);
                assert_eq!(row.prefix[0], 0);
                for (i, &(ink, len)) in row.runs.iter().enumerate() {
                    assert_eq!(row.offsets[i], acc);
                    acc += len;
                    assert_eq!(row.prefix[i + 1], acc);
                    if ink {
                        last_ink = i;
                    }
                }
                assert_eq!(acc, w);
                assert_eq!(row.last_ink_run, last_ink);

                // Check reverse_into against scanning the mirrored bitmap.
                row.reverse_into(&mut rev);
                mrow.scan_row(&mimg, y);
                assert_eq!(rev.runs, mrow.runs, "reverse runs {w}x{h} y={y}");
                assert_eq!(rev.offsets, mrow.offsets, "reverse offsets {w}x{h} y={y}");
                assert_eq!(rev.prefix, mrow.prefix, "reverse prefix {w}x{h} y={y}");
                assert_eq!(
                    rev.last_ink_run, mrow.last_ink_run,
                    "reverse last_ink {w}x{h} y={y}"
                );
            }
        }
    }
}
