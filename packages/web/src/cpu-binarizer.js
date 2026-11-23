/**
 * CPU fallback binarizer — same BinarizedFrame contract as the GPU path
 * (1bpp, bit (x,y) = data[y*bytesPerRow + (x>>3)] >> (x&7) & 1, bytesPerRow
 * a multiple of 4, set bit = ink). Slower (~30-60ms/frame) but runs on any
 * browser: canvas 2D + integral-image box mean + Bradley threshold.
 */

/** @typedef {import("./gpu/binarizer.js").BinarizedFrame} BinarizedFrame */

export class CpuBinarizer {
  #canvas;
  #ctx;
  #maxSize;
  #windowDivisor;
  #threshold;
  /** Cached luma + integral image for threshold sweeps on the same source. */
  /** @type {{ source: HTMLCanvasElement | ImageBitmap | OffscreenCanvas, w: number, h: number, luma: Float32Array, integ: Float64Array, floor: number } | null} */
  #cache = null;

  /** @param {Required<import("./gpu/binarizer.js").BinarizerTuning>} opts */
  constructor(opts) {
    this.#maxSize = opts.maxSize;
    this.#windowDivisor = opts.windowDivisor;
    this.#threshold = opts.threshold;
    this.#canvas = /** @type {HTMLCanvasElement | OffscreenCanvas} */ (
      typeof document === "undefined"
        ? new OffscreenCanvas(1, 1) // workers
        : document.createElement("canvas")
    );
    this.#ctx = /** @type {CanvasRenderingContext2D} */ (
      this.#canvas.getContext("2d", { willReadFrequently: true })
    );
  }

  /**
   * Override the Bradley threshold for subsequent process() calls (accuracy-
   * high pipeline sweeps a second threshold per frame).
   * @param {number} t
   */
  setThreshold(t) {
    this.#threshold = t;
  }

  /**
   * Fit src dims to multiples of 32 under maxSize — same contract as the
   * GPU path: sources that already fit are padded UP (drawn 1:1 into a
   * white margin; a down-squeeze costs pitch and softens exactly the small
   * codes the locators are starved for), oversized sources floor down.
   * @param {number} srcW
   * @param {number} srcH
   */
  #fitDst(srcW, srcH) {
    const scale = Math.min(1, this.#maxSize / Math.max(srcW, srcH));
    if (scale >= 1) {
      const snapUp = (/** @type {number} */ n) =>
        Math.min(this.#maxSize, Math.max(32, Math.ceil(n / 32) * 32));
      return { w: snapUp(srcW), h: snapUp(srcH) };
    }
    const snapDown = (/** @type {number} */ n) => Math.max(32, Math.floor((n * scale) / 32) * 32);
    return { w: snapDown(srcW), h: snapDown(srcH) };
  }

  /** Release cached luma/integral buffers so nothing lingers between frames. */
  clearCache() {
    this.#cache = null;
  }

  /**
   * Same source contract as the GPU binarizer: video elements or
   * canvases/ImageBitmaps. Repeated calls on the same immutable source
   * (threshold sweeps) reuse the cached luma + integral image.
   * @param {HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas} source
   * @returns {Promise<BinarizedFrame>}
   */
  async process(source) {
    const isVideo = "videoWidth" in source;
    /** @type {number} */ let w;
    /** @type {number} */ let h;
    /** @type {Float32Array} */ let luma;
    /** @type {Float64Array} */ let integ;
    /** @type {number} */ let floor;

    if (!isVideo && this.#cache && this.#cache.source === source) {
      ({ w, h, luma, integ, floor } = this.#cache);
    } else {
      const srcW =
        /** @type {HTMLVideoElement} */ (source).videoWidth ?? source.width;
      const srcH =
        /** @type {HTMLVideoElement} */ (source).videoHeight ?? source.height;
      if (!srcW || !srcH) throw new Error("Frame not ready.");

      ({ w, h } = this.#fitDst(srcW, srcH));
      if (w !== this.#canvas.width || h !== this.#canvas.height) {
        this.#canvas.width = w;
        this.#canvas.height = h;
      }
      if (w >= srcW && h >= srcH) {
        // true pad: 1:1 pixels, white margin (never resample a fitting source)
        this.#ctx.fillStyle = "#fff";
        this.#ctx.fillRect(0, 0, w, h);
        this.#ctx.drawImage(source, 0, 0);
      } else {
        this.#ctx.drawImage(source, 0, 0, w, h);
      }
      const px = this.#ctx.getImageData(0, 0, w, h).data;

      // Luma 0-1 (same weights as the GPU shader).
      luma = new Float32Array(w * h);
      for (let i = 0; i < w * h; i++) {
        luma[i] =
          (0.299 * (px[i * 4] ?? 0) +
            0.587 * (px[i * 4 + 1] ?? 0) +
            0.114 * (px[i * 4 + 2] ?? 0)) /
          255;
      }

      // Global auto-levels, same contract as the GPU path: p2/p98 over a
      // ~256x256 stride sample of the luma plane, applied only to genuinely
      // dim, low-contrast frames (see src/shaders/stats.wgsl).
      const sample = Math.max(1, Math.floor(Math.sqrt((w * h) / 65536)));
      const hist = new Uint32Array(256);
      let sampled = 0;
      for (let y = 0; y < h; y += sample) {
        for (let x = 0; x < w; x += sample) {
          const v = Math.min(1, Math.max(0, luma[y * w + x] ?? 0));
          hist[(v * 255) | 0] = (hist[(v * 255) | 0] ?? 0) + 1;
          sampled++;
        }
      }
      const tLo = (sampled * 2) / 100;
      const tHi = (sampled * 98) / 100;
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
      let lo = loBin / 255;
      let hi = hiBin / 255;
      if (hi - lo <= 2 / 255) {
        let minBin = 0;
        let maxBin = 255;
        for (let b = 0; b < 256; b++) {
          if (hist[b]) {
            minBin = b;
            break;
          }
        }
        for (let b = 255; b >= 0; b--) {
          if (hist[b]) {
            maxBin = b;
            break;
          }
        }
        lo = minBin / 255;
        hi = maxBin / 255;
      }
      if (hi < 0.65 && hi - lo < 0.4 && hi - lo > 2 / 255) {
        const span = hi - lo;
        for (let i = 0; i < w * h; i++) {
          luma[i] = Math.min(1, Math.max(0, ((luma[i] ?? 0) - lo) / span));
        }
      }

      // Integral image for the box mean (same math as the GPU blur passes).
      integ = new Float64Array((w + 1) * (h + 1));
      for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
          integ[(y + 1) * (w + 1) + x + 1] =
            (luma[y * w + x] ?? 0) +
            (integ[y * (w + 1) + x + 1] ?? 0) +
            (integ[(y + 1) * (w + 1) + x] ?? 0) -
            (integ[y * (w + 1) + x] ?? 0);
        }
      }

      // Global-mean floor (total is already the bottom-right corner of integ!).
      floor = 0.5 * ((integ[h * (w + 1) + w] ?? 0) / (w * h));
      this.#cache = isVideo
        ? null
        : {
            source: /** @type {HTMLCanvasElement | ImageBitmap | OffscreenCanvas} */ (source),
            w,
            h,
            luma,
            integ,
            floor,
          };
    }

    const radius = Math.max(1, Math.floor(Math.min(w, h) / (2 * this.#windowDivisor)));
    const bytesPerRow = Math.ceil(w / 8 / 4) * 4;
    const data = new Uint8Array(bytesPerRow * h);
    const thrScale = this.#threshold;
    for (let y = 0; y < h; y++) {
      const ya = Math.max(0, y - radius), yb = Math.min(h - 1, y + radius);
      const rowOff = y * bytesPerRow;
      for (let x = 0; x < w; x++) {
        const xa = Math.max(0, x - radius), xb = Math.min(w - 1, x + radius);
        const area = (yb - ya + 1) * (xb - xa + 1);
        const sum =
          (integ[(yb + 1) * (w + 1) + xb + 1] ?? 0) -
          (integ[ya * (w + 1) + xb + 1] ?? 0) -
          (integ[(yb + 1) * (w + 1) + xa] ?? 0) +
          (integ[ya * (w + 1) + xa] ?? 0);
        const l = luma[y * w + x] ?? 0;
        const mean = sum / area;
        if (l < thrScale * mean || (l < floor && l <= mean + 8 / 255)) {
          const byte = rowOff + (x >> 3);
          data[byte] = (data[byte] ?? 0) | (1 << (x & 7));
        }
      }
    }

    return { width: w, height: h, data, bytesPerRow };
  }

  destroy() {
    this.#cache = null;
  }
}
