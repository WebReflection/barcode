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

  /**
   * Same source contract as the GPU binarizer: video elements or
   * canvases/ImageBitmaps.
   * @param {HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas} source
   * @returns {Promise<BinarizedFrame>}
   */
  async process(source) {
    const srcW =
      /** @type {HTMLVideoElement} */ (source).videoWidth ?? source.width;
    const srcH =
      /** @type {HTMLVideoElement} */ (source).videoHeight ?? source.height;
    if (!srcW || !srcH) throw new Error("Frame not ready.");

    const { w, h } = this.#fitDst(srcW, srcH);
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
    const luma = new Float32Array(w * h);
    for (let i = 0; i < w * h; i++) {
      // ImageData.data is Uint8ClampedArray<ArrayBufferLike> — under
      // noUncheckedIndexedAccess every read is `number | undefined`;
      // a missing byte means a transparent pixel, i.e. paper (0).
      luma[i] =
        (0.299 * (px[i * 4] ?? 0) +
          0.587 * (px[i * 4 + 1] ?? 0) +
          0.114 * (px[i * 4 + 2] ?? 0)) /
        255;
    }

    // Integral image for the box mean (same math as the GPU blur passes).
    const integ = new Float64Array((w + 1) * (h + 1));
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        integ[(y + 1) * (w + 1) + x + 1] =
          (luma[y * w + x] ?? 0) +
          (integ[y * (w + 1) + x + 1] ?? 0) +
          (integ[(y + 1) * (w + 1) + x] ?? 0) -
          (integ[y * (w + 1) + x] ?? 0);
      }
    }

    const radius = Math.max(1, Math.floor(Math.min(w, h) / (2 * this.#windowDivisor)));
    const bytesPerRow = Math.ceil(w / 8 / 4) * 4;
    const data = new Uint8Array(bytesPerRow * h);
    const thrScale = this.#threshold;
    // Global-mean floor (same rescue as the GPU shader): solid dark regions
    // larger than the Bradley window erase themselves under a purely local
    // threshold; anything darker than half the scene mean is ink regardless.
    // The rescue only applies where the NEIGHBORHOOD itself is darker than
    // the floor — in a frame with a big hand/table shadow the global mean
    // drops and an unconditional floor force-inks the shadow's light
    // modules too, shredding codes that cross the shadow boundary.
    let total = 0;
    for (let i = 0; i < w * h; i++) total += luma[i] ?? 0;
    const floor = 0.5 * (total / (w * h));
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
        // The floor may only agree with the local dark side: a pixel
        // brighter than its own neighborhood is local structure (a light
        // module inside a shadow), not part of a solid dark region. The
        // slack absorbs float noise in uniform regions (luma == mean
        // exactly on synthetic renders) — 8/255 of the 0-1 luma range,
        // well below the ~0.06 gap between a shadowed light module and
        // its neighborhood mean on the receipt frame.
        if (l < thrScale * mean || (l < floor && l <= mean + 8 / 255)) {
          const byte = rowOff + (x >> 3);
          data[byte] = (data[byte] ?? 0) | (1 << (x & 7));
        }
      }
    }

    return { width: w, height: h, data, bytesPerRow };
  }

  destroy() {
    // nothing persistent
  }
}
