/**
 * GPU binarization pipeline.
 *
 * Per camera frame: downscale+luma -> horizontal box mean -> vertical box
 * mean -> Bradley threshold -> packed 1bpp mask, read back to CPU.
 *
 * The readback is the only GPU->CPU transfer and it is tiny: a 1280x720
 * 1-bit frame is ~115 KB, comfortable to mapAsync at 30fps. The 8-bit
 * luma plane never leaves the GPU.
 *
 * Mask layout contract (see crates/barcode-core/src/lib.rs):
 *   bit (x, y) = data[y * bytesPerRow + (x >> 3)] >> (x & 7) & 1
 *   bytesPerRow is a multiple of 4; a set bit is ink.
 * Destination widths are snapped to multiples of 32, so the flat bitstream
 * the shader writes is identical to row-aligned packing (bytesPerRow = w/8).
 */

import lumaShader from "../shaders/luma.wgsl?raw";
import blurShader from "../shaders/blur.wgsl?raw";
import binarizeShader from "../shaders/binarize.wgsl?raw";
import statsShader from "../shaders/stats.wgsl?raw";

/** @typedef {Object} BinarizedFrame
 * @property {number} width
 * @property {number} height
 * @property {Uint8Array} data Packed 1bpp ink mask, `bytesPerRow`-aligned rows.
 * @property {number} bytesPerRow
 */

/** @typedef {Object} BinarizerTuning
 * @property {number} [maxSize] Longest edge of the binarized frame. 1D symbologies want >= 1000.
 * @property {number} [windowDivisor] Bradley window diameter = min(w, h) / windowDivisor.
 * @property {number} [threshold] ink := luma < mean * threshold (Bradley's 0.15 => 0.85).
 */

/** @typedef {BinarizerTuning & { requireGpu?: boolean }} BinarizerOptions
 * @property {boolean} [requireGpu] Throw instead of silently falling back to the CPU path.
 */

/** One 256-byte slot per pipeline stage (bind-group offset alignment). */
const STAGE = /** @type {const} */ ({
  luma: 0,
  blurH: 256,
  blurV: 512,
  binarize: 768,
  lumaSmall: 1024,
  globalH: 1280,
  globalV: 1536,
  statsLuma: 1792,
  histogram: 2048,
});
const UNIFORM_SLOTS = 9;
/** Side of the tiny luma downsample the global-mean floor is computed on. */
const SMALL = 64;
/**
 * Side of the luma probe the p2/p98 auto-levels statistic is computed on.
 * 64x64 percentiles are too noisy (they missed one synthetic dim set and,
 * ungated, one real frame); 256x256 matched the full-frame statistic on
 * both synthetic sets at a negligible pass cost (65k pixels, 32x32 groups).
 */
const STATS = 256;
/** 256 histogram bins, u32 counts. */
const BINS = 256;

export class GpuBinarizer {
  /** @type {GPUDevice} */
  #device;
  /** @type {number} */
  #maxSize;
  /** @type {number} */
  #windowDivisor;
  /** @type {number} */
  #threshold;
  /** @type {GPUComputePipeline} */ #lumaPipeline;
  /** @type {GPUComputePipeline} */ #blurPipeline;
  /** @type {GPUComputePipeline} */ #binarizePipeline;
  /** @type {GPUComputePipeline} */ #histPipeline;
  /** @type {GPUComputePipeline} */ #scanPipeline;
  /** @type {GPUSampler} */ #sampler;
  /** @type {GPUTexture | undefined} */ #srcTex;
  /** @type {number} */ #allocW = 0;
  /** @type {number} */ #texW = 0;
  /** @type {number} */ #texH = 0;
  /** @type {number} */ #allocH = 0;
  /** @type {{ uniform: GPUBuffer, luma: GPUBuffer, blurA: GPUBuffer, blurB: GPUBuffer, small: GPUBuffer, smallScratch: GPUBuffer, smallGlobal: GPUBuffer, statsLuma: GPUBuffer, hist: GPUBuffer, stretch: GPUBuffer, mask: GPUBuffer, staging: GPUBuffer } | null} */
  #bufs = null;

  /**
   * @param {GPUDevice} device
   * @param {Required<import("./binarizer.js").BinarizerTuning>} opts
   */
  constructor(device, opts) {
    this.#device = device;
    this.#maxSize = opts.maxSize;
    this.#windowDivisor = opts.windowDivisor;
    this.#threshold = opts.threshold;

    const d = device;
    d.addEventListener?.("uncapturederror", (/** @type {GPUUncapturedErrorEvent} */ ev) => {
      console.error("[WebGPU]", ev.error?.message ?? ev);
    });
    this.#sampler = d.createSampler({ magFilter: "linear", minFilter: "linear" });
    /** @type {(code: string) => GPUShaderModule} */
    const mod = (code) => d.createShaderModule({ code });
    this.#lumaPipeline = d.createComputePipeline({
      layout: "auto",
      compute: { module: mod(lumaShader), entryPoint: "main" },
    });
    this.#blurPipeline = d.createComputePipeline({
      layout: "auto",
      compute: { module: mod(blurShader), entryPoint: "main" },
    });
    this.#binarizePipeline = d.createComputePipeline({
      layout: "auto",
      compute: { module: mod(binarizeShader), entryPoint: "main" },
    });
    const statsModule = mod(statsShader);
    this.#histPipeline = d.createComputePipeline({
      layout: "auto",
      compute: { module: statsModule, entryPoint: "hist_main" },
    });
    this.#scanPipeline = d.createComputePipeline({
      layout: "auto",
      compute: { module: statsModule, entryPoint: "scan_main" },
    });
  }

  /**
   * @param {BinarizerOptions} [opts]
   * @returns {Promise<GpuBinarizer>}
   */
  static async create(opts = {}) {
    if (!("gpu" in navigator) || !navigator.gpu) {
      throw new Error(
        "WebGPU unavailable. The SDK requires a WebGPU-capable browser " +
          "(iOS 26+, Chrome 113+, Firefox 141+).",
      );
    }
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("No suitable GPU adapter.");
    const device = await adapter.requestDevice();
    return new GpuBinarizer(device, {
      maxSize: opts.maxSize ?? 1152,
      windowDivisor: opts.windowDivisor ?? 8,
      threshold: opts.threshold ?? 0.85,
    });
  }

  /**
   * Fit src dims to multiples of 32 under maxSize. Sources that already
   * fit are padded UP (drawn 1:1 into a white margin) — a down-squeeze
   * costs pitch and softens edges on exactly the small codes the locator
   * is pitch-starved for, and arbitrary user images are never 32-multiples.
   * Oversized sources still floor down (the camera's 1080 is not a
   * multiple of 32, and downscaling is unavoidable there).
   * @param {number} srcW
   * @param {number} srcH
   */
  #fitDst(srcW, srcH) {
    const scale = Math.min(1, this.#maxSize / Math.max(srcW, srcH));
    if (scale >= 1) {
      const snapUp = (/** @type {number} */ n) => Math.min(this.#maxSize, Math.max(32, Math.ceil(n / 32) * 32));
      return { w: snapUp(srcW), h: snapUp(srcH) };
    }
    /** @type {(n: number) => number} */
    const snapDown = (n) => Math.max(32, Math.floor((n * scale) / 32) * 32);
    return { w: snapDown(srcW), h: snapDown(srcH) };
  }

  /** @param {number} dstW @param {number} dstH */
  #alloc(dstW, dstH) {
    const d = this.#device;
    const px = dstW * dstH;
    const maskBytes = Math.ceil(px / 32) * 4;

    const old = this.#bufs;
    if (old) {
      old.uniform.destroy();
      old.luma.destroy();
      old.blurA.destroy();
      old.blurB.destroy();
      old.small.destroy();
      old.smallScratch.destroy();
      old.smallGlobal.destroy();
      old.statsLuma.destroy();
      old.hist.destroy();
      old.stretch.destroy();
      old.mask.destroy();
      old.staging.destroy();
    }
    const smallBytes = SMALL * SMALL * 4;
    const statsBytes = STATS * STATS * 4;

    this.#bufs = {
      uniform: d.createBuffer({
        size: 256 * UNIFORM_SLOTS,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
      }),
      luma: d.createBuffer({ size: px * 4, usage: GPUBufferUsage.STORAGE }),
      blurA: d.createBuffer({ size: px * 4, usage: GPUBufferUsage.STORAGE }),
      blurB: d.createBuffer({ size: px * 4, usage: GPUBufferUsage.STORAGE }),
      // The global-mean floor lives on a tiny downsample: two full-extent
      // box-mean passes over w*h pixels were O(frame) taps per pixel and
      // saturated phone GPUs (readable as preview jank).
      small: d.createBuffer({ size: smallBytes, usage: GPUBufferUsage.STORAGE }),
      smallScratch: d.createBuffer({ size: smallBytes, usage: GPUBufferUsage.STORAGE }),
      smallGlobal: d.createBuffer({ size: smallBytes, usage: GPUBufferUsage.STORAGE }),
      // Auto-levels: a 256x256 probe (never stretched itself), its histogram,
      // and the [lo, hi] the scan pass writes for the luma stage.
      statsLuma: d.createBuffer({ size: statsBytes, usage: GPUBufferUsage.STORAGE }),
      hist: d.createBuffer({
        size: BINS * 4,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
      }),
      stretch: d.createBuffer({ size: 2 * 4, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST }),
      mask: d.createBuffer({
        size: maskBytes,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST,
      }),
      staging: d.createBuffer({
        size: maskBytes,
        usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
      }),
    };
    this.#allocW = dstW;
    this.#allocH = dstH;
  }

  /**
   * Override the Bradley threshold for subsequent process() calls (the
   * uniform is rewritten every frame, so this is free). Used by the
   * accuracy-high pipeline to sweep a second threshold per frame.
   * @param {number} t
   */
  setThreshold(t) {
    this.#threshold = t;
  }

  /**
   * Binarize one frame. Accepts any `copyExternalImageToTexture` source
   * with dimensions: an HTMLVideoElement (videoWidth/videoHeight) or a
   * canvas / ImageBitmap (width/height) — the self-test harness feeds
   * synthetic specimen canvases through the same path.
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
    // The texture holds the source at its native size; the grid is dst.
    // Both dims can change independently (dst is the 32-snap of src), so
    // track the source dims too.
    if (w !== this.#allocW || h !== this.#allocH || srcW !== this.#texW || srcH !== this.#texH) {
      this.#srcTex?.destroy();
      this.#srcTex = this.#device.createTexture({
        size: [srcW, srcH],
        format: "rgba8unorm",
        usage:
          GPUTextureUsage.TEXTURE_BINDING |
          GPUTextureUsage.COPY_DST |
          GPUTextureUsage.RENDER_ATTACHMENT,
      });
      this.#texW = srcW;
      this.#texH = srcH;
      this.#alloc(w, h);
    }
    const bufs = this.#bufs;
    const srcTex = this.#srcTex;
    if (!bufs || !srcTex) throw new Error("Unreachable: buffers missing.");

    const d = this.#device;
    const radius = Math.max(1, Math.floor(Math.min(w, h) / (2 * this.#windowDivisor)));

    // Write all stage uniforms up front, at fixed slots. Bind groups pin the
    // offset per stage, so queue ordering can't clobber a pass's uniforms.
    const u = d.queue;
    u.writeBuffer(bufs.uniform, STAGE.luma, new Uint32Array([w, h, srcW, srcH, 1]));
    // The probe pass never stretches (apply=0), or the statistic would feed
    // back into itself frame after frame. srcW/srcH = STATS (not 0 — 0 is
    // the deliberate all-white pad used for the FIXED 0.5 floor pass) so the
    // shader samples the whole frame over the probe grid.
    u.writeBuffer(
      bufs.uniform,
      STAGE.statsLuma,
      new Uint32Array([STATS, STATS, STATS, STATS, 0]),
    );
    u.writeBuffer(bufs.uniform, STAGE.histogram, new Uint32Array([STATS, STATS]));
    u.writeBuffer(bufs.uniform, STAGE.blurH, new Uint32Array([w, h, radius, 0]));
    u.writeBuffer(bufs.uniform, STAGE.blurV, new Uint32Array([w, h, radius, 1]));
    u.writeBuffer(
      bufs.uniform,
      STAGE.binarize,
      new Uint32Array([w, h, Math.round(this.#threshold * 1000), 1000, 500, 1000, SMALL, SMALL]),
    );
    // The tiny luma downsample (the luma stage samples normalized UV, so a
    // 64x64 destination averages the whole frame), then full-extent box
    // means over those 4096 pixels. Only dstW/dstH are written on purpose:
    // the luma shader reads srcW/srcH as 0, pads the whole plane white, and
    // the binarize floor becomes the FIXED `luma < 0.5` rescue. That fixed
    // floor is load-bearing for faded thermal receipts (2026-11-18 field
    // corpus: a correct 0.5 x scene mean regressed receipt-qr-112247 and
    // shipping-itf-115030). The glossy-label DataMatrix it loses is read on
    // the CPU mask instead (see STILL_CPU_FAMILIES in api/worker.js) — do
    // not "fix" this uniform without re-running the field corpus.
    u.writeBuffer(bufs.uniform, STAGE.lumaSmall, new Uint32Array([SMALL, SMALL, 0, 0, 1]));
    u.writeBuffer(bufs.uniform, STAGE.globalH, new Uint32Array([SMALL, SMALL, SMALL, 0]));
    u.writeBuffer(bufs.uniform, STAGE.globalV, new Uint32Array([SMALL, SMALL, SMALL, 1]));

    d.queue.copyExternalImageToTexture(
      { source },
      { texture: srcTex },
      [srcW, srcH],
    );

    const enc = d.createCommandEncoder();
    const maskBytes = Math.ceil((w * h) / 32) * 4;
    // The mask is written with atomicOr and persists across frames — clear it
    // BEFORE the binarize pass or ink accumulates until the mask saturates.
    enc.clearBuffer(bufs.mask, 0, maskBytes);
    // Histogram bins accumulate with atomicAdd: clear every frame.
    enc.clearBuffer(bufs.hist, 0, BINS * 4);
    /** @param {number} offset @param {number} [size] */
    const uniform = (offset, size = 16) => ({ buffer: bufs.uniform, offset, size });

    /**
     * @param {GPUComputePipeline} pipeline
     * @param {GPUBindGroupEntry[]} entries
     * @param {number} [gw] dispatch grid width (default frame width)
     * @param {number} [gh] dispatch grid height (default frame height)
     */
    const pass = (pipeline, entries, gw = w, gh = h) => {
      const bg = enc.beginComputePass();
      bg.setPipeline(pipeline);
      bg.setBindGroup(0, d.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries }));
      bg.dispatchWorkgroups(Math.ceil(gw / 8), Math.ceil(gh / 8));
      bg.end();
    };

    // Auto-levels statistic first: probe -> histogram -> p2/p98 scan. The
    // scan writes the [lo, hi] the luma stage then applies.
    pass(
      this.#lumaPipeline,
      [
        { binding: 0, resource: srcTex.createView() },
        { binding: 1, resource: this.#sampler },
        { binding: 2, resource: uniform(STAGE.statsLuma, 32) },
        { binding: 3, resource: { buffer: bufs.statsLuma } },
        { binding: 4, resource: { buffer: bufs.stretch } },
      ],
      STATS,
      STATS,
    );
    pass(
      this.#histPipeline,
      [
        { binding: 0, resource: uniform(STAGE.histogram) },
        { binding: 1, resource: { buffer: bufs.statsLuma } },
        { binding: 2, resource: { buffer: bufs.hist } },
      ],
      STATS,
      STATS,
    );
    pass(
      this.#scanPipeline,
      [
        { binding: 2, resource: { buffer: bufs.hist } },
        { binding: 3, resource: { buffer: bufs.stretch } },
      ],
      1,
      1,
    );
    pass(this.#lumaPipeline, [
      { binding: 0, resource: srcTex.createView() },
      { binding: 1, resource: this.#sampler },
      { binding: 2, resource: uniform(STAGE.luma, 32) },
      { binding: 3, resource: { buffer: bufs.luma } },
      { binding: 4, resource: { buffer: bufs.stretch } },
    ]);
    pass(this.#blurPipeline, [
      { binding: 0, resource: uniform(STAGE.blurH) },
      { binding: 1, resource: { buffer: bufs.luma } },
      { binding: 2, resource: { buffer: bufs.blurA } },
    ]);
    pass(this.#blurPipeline, [
      { binding: 0, resource: uniform(STAGE.blurV) },
      { binding: 1, resource: { buffer: bufs.blurA } },
      { binding: 2, resource: { buffer: bufs.blurB } },
    ]);
    // Global-mean floor on the tiny downsample: luma pass into 64x64, then
    // full-extent row/column means of those.
    pass(
      this.#lumaPipeline,
      [
        { binding: 0, resource: srcTex.createView() },
        { binding: 1, resource: this.#sampler },
        { binding: 2, resource: uniform(STAGE.lumaSmall, 32) },
        { binding: 3, resource: { buffer: bufs.small } },
        { binding: 4, resource: { buffer: bufs.stretch } },
      ],
      SMALL,
      SMALL,
    );
    pass(
      this.#blurPipeline,
      [
        { binding: 0, resource: uniform(STAGE.globalH) },
        { binding: 1, resource: { buffer: bufs.small } },
        { binding: 2, resource: { buffer: bufs.smallScratch } },
      ],
      SMALL,
      SMALL,
    );
    pass(
      this.#blurPipeline,
      [
        { binding: 0, resource: uniform(STAGE.globalV) },
        { binding: 1, resource: { buffer: bufs.smallScratch } },
        { binding: 2, resource: { buffer: bufs.smallGlobal } },
      ],
      SMALL,
      SMALL,
    );
    pass(this.#binarizePipeline, [
      { binding: 0, resource: uniform(STAGE.binarize, 32) },
      { binding: 1, resource: { buffer: bufs.luma } },
      { binding: 2, resource: { buffer: bufs.blurB } },
      { binding: 3, resource: { buffer: bufs.mask } },
      { binding: 4, resource: { buffer: bufs.smallGlobal } },
    ]);

    enc.copyBufferToBuffer(bufs.mask, 0, bufs.staging, 0, maskBytes);
    d.queue.submit([enc.finish()]);

    await bufs.staging.mapAsync(GPUMapMode.READ);
    const data = new Uint8Array(bufs.staging.getMappedRange().slice(0));
    bufs.staging.unmap();

    return { width: w, height: h, data, bytesPerRow: w / 8 };
  }

  destroy() {
    this.#srcTex?.destroy();
    const old = this.#bufs;
    if (old) {
      old.luma.destroy();
      old.blurA.destroy();
      old.blurB.destroy();
      old.small.destroy();
      old.smallScratch.destroy();
      old.smallGlobal.destroy();
      old.mask.destroy();
      old.staging.destroy();
      old.uniform.destroy();
    }
    this.#device.destroy();
  }
}
