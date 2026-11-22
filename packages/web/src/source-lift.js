/**
 * Second exposure for dim stills.
 *
 * Both binarizers stretch a genuinely dim, low-contrast frame
 * (p98 < 0.65 and p98 - p2 < 0.4) to 0..1 before the Bradley threshold.
 * That is the right call for a faded receipt, but on a low-exposure
 * capture with few distinct levels (synthetic light=0.4: paper ~0.34,
 * print ~0.04) the re-quantization costs module edges and the row
 * scanners miss, while zxing still reads the grayscale. Re-drawing the
 * SOURCE brighter makes the gate skip, preserving the original level
 * spacing; the same frame then reads. Measured on the synthetic 1D
 * differential: 31 of the 34 zxing-only dim cells recover with one lift.
 *
 * Still-only by construction: the caller tries this after every other
 * still pass missed. A live frame cannot pay the extra draw+binarize.
 */

/** Probe size for the luma percentiles (the binarizers use 256). */
const PROBE = 64;

/** Longest edge of the probe; keeps a non-square source's aspect. */
/** @param {CanvasImageSource} source */
const probe = (source) => {
  const box = /** @type {{ width?: number, height?: number, videoWidth?: number, videoHeight?: number }} */ (
    /** @type {unknown} */ (source)
  );
  const w = Number(box.width ?? box.videoWidth ?? 0);
  const h = Number(box.height ?? box.videoHeight ?? 0);
  return { w, h };
};

/**
 * Brighten a dim source so the binarizer's auto-levels gate does not
 * fire. Returns null for a normal-exposure or high-contrast frame (the
 * gate's own condition), so callers can skip the extra binarize.
 * @param {CanvasImageSource} source
 * @returns {Promise<{ canvas: OffscreenCanvas, factor: number } | null>}
 */
export async function liftDim(source) {
  const { w, h } = probe(source);
  if (!w || !h) return null;
  const scale = PROBE / Math.max(w, h);
  const pw = Math.max(1, Math.round(w * scale));
  const ph = Math.max(1, Math.round(h * scale));
  const small = new OffscreenCanvas(pw, ph);
  const sctx = small.getContext("2d", { willReadFrequently: true });
  if (!sctx) return null;
  sctx.drawImage(source, 0, 0, pw, ph);
  const px = sctx.getImageData(0, 0, pw, ph).data;
  const hist = new Uint32Array(256);
  for (let i = 0; i < pw * ph; i++) {
    const luma =
      (0.299 * (px[i * 4] ?? 0) +
        0.587 * (px[i * 4 + 1] ?? 0) +
        0.114 * (px[i * 4 + 2] ?? 0)) /
      255;
    const bin = Math.min(255, Math.max(0, (luma * 255) | 0));
    hist[bin] = (hist[bin] ?? 0) + 1;
  }
  const n = pw * ph;
  const tLo = (n * 2) / 100;
  const tHi = (n * 98) / 100;
  let cum = 0;
  let loBin = 0;
  let hiBin = 255;
  let gotLo = false;
  for (let b = 0; b < 256; b++) {
    cum += hist[b] ?? 0;
    if (!gotLo && cum >= tLo) {
      loBin = b;
      gotLo = true;
    }
    if (cum >= tHi) {
      hiBin = b;
      break;
    }
  }
  const lo = loBin / 255;
  const hi = hiBin / 255;
  // Exactly the auto-levels gate (src/shaders/stats.wgsl, cpu-binarizer.js).
  if (!(hi < 0.65 && hi - lo < 0.4 && hi - lo > 2 / 255)) return null;
  // Land the paper at a normal exposure. The synthetic "light 1" scenes
  // put the label paper at 235/255 = 0.92, so that is the target: a
  // lower one (e.g. 0.85) measurably fails the same cells.
  const factor = Math.min(4, Math.max(1.5, 0.92 / Math.max(hi, 0.05)));
  const canvas = new OffscreenCanvas(w, h);
  // willReadFrequently: the lift reads and writes every pixel once.
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  ctx.drawImage(source, 0, 0, w, h);
  // Manual multiply: `ctx.filter = brightness()` is not a plain sRGB
  // multiply (measured ~2.3x effective for brightness(2.5) in Chromium),
  // and the gate threshold is defined on sRGB luma. One still-size
  // read/modify/write costs ~40-60ms on a 12MP source, stills only.
  const img = ctx.getImageData(0, 0, w, h);
  const data = img.data;
  for (let i = 0; i < data.length; i += 4) {
    data[i] = Math.min(255, (data[i] ?? 0) * factor);
    data[i + 1] = Math.min(255, (data[i + 1] ?? 0) * factor);
    data[i + 2] = Math.min(255, (data[i + 2] ?? 0) * factor);
  }
  ctx.putImageData(img, 0, 0);
  return { canvas, factor };
}
