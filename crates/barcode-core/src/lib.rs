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

/// All symbologies the library intends to support, eventually.
/// Start small: implement the `Milestone1` ones first.
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
    Code93,
    Codabar,
    Itf,
    DataBar,
    DataBarStacked,
    DataBarExpanded,
    DataBarLimited,
    PDF417,
    Aztec,
    MaxiCode,
    DotCode,
    HanXin,
    CodablockF,
    Pharmacode,
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
            Symbology::Code93 => "code93",
            Symbology::Codabar => "codabar",
            Symbology::Itf => "itf",
            Symbology::DataBar => "databar",
            Symbology::DataBarStacked => "databarstk",
            Symbology::DataBarExpanded => "databarexpanded",
            Symbology::DataBarLimited => "databarltd",
            Symbology::PDF417 => "pdf417",
            Symbology::Aztec => "aztec",
            Symbology::MaxiCode => "maxicode",
            Symbology::DotCode => "dotcode",
            Symbology::HanXin => "hanxin",
            Symbology::CodablockF => "codablockf",
            Symbology::Pharmacode => "pharmacode",
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
        Self { width, height, bytes_per_row, data }
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
        let mut data = vec![0u8; bpr * self.height as usize];
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                if self.ink(x as u32, y as u32) {
                    let mx = self.width as usize - 1 - x;
                    let my = self.height as usize - 1 - y;
                    data[my * bpr + (mx >> 3)] |= 1 << (mx & 7);
                }
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
        let mut data = vec![0u8; bpr * self.height as usize];
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                if self.ink(x as u32, y as u32) {
                    let mx = self.width as usize - 1 - x;
                    data[y * bpr + (mx >> 3)] |= 1 << (mx & 7);
                }
            }
        }
        data
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
                if sx >= 0 && sy >= 0 && (sx as u32) < self.width && (sy as u32) < self.height
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

/// A symbology decoder. Implementations must be allocation-frugal on the hot
/// path (reuse scratch buffers internally) and must never panic on hostile
/// input: barcodes come from an untrusted camera pointed at the world.
pub trait SymbologyDecoder {
    fn symbology(&self) -> Symbology;
    fn decode(&mut self, image: &BitImage) -> Vec<DecodeResult>;
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
}
