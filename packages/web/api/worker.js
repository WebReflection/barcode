// The SDK worker: bound via @webreflection/bindings, exposing `decode`
// (one frame in — candidates out, stateless) and `encode` (symbology +
// text — raw module bitmap out). Strictly serial: the main thread sends
// one frame only after the previous answer, so no request ids exist.
//
// The bootstrap: the first port-carrying message is the page's
// MessagePort; the listener is gone right after and the port is bound
// exactly once. Messages without ports pass through untouched —
// embedders keep their protocol.
//
// Latency laws travel per-request: live frames default to the 500ms
// camera law, still images to 5000ms — both configurable per decode.
// Results are ranked: most centered first, larger symbol as tiebreak.

import binding, { options } from '@webreflection/bindings/message-port';
import { createBinarizer } from '../src/binarizer-factory.js';
import { CpuBinarizer } from '../src/cpu-binarizer.js';
import { getWasmScanner, decodeFrame } from '../src/decoder.js';
import { rotateDrawable } from '../src/source-rotate.js';
import { liftDim } from '../src/source-lift.js';
import { cropDrawable, linearRoiCandidates, mirrorDrawable, otsuMask, padDrawable, roiCandidates } from '../src/roi.js';
import { ALL_FORMATS, LINEAR, cheapFormats, dueForFullSweep, is2DResult, markFullSweep, needsScheduling } from '../src/live-schedule.js';
import {
  encode_fixture_opts,
  encode_fixture_raw_opts,
} from '../src/wasm/pkg/barcode_wasm.js';

/**
 * @typedef {import('./main.js').Accuracy} Accuracy
 * @typedef {import('./main.js').Bindings} Bindings
 * @typedef {import('../src/decoder.js').DecodedCode} DecodedCode
 */

/** A decode result plus the centering score ranked() attaches. */
/** @typedef {DecodedCode & { score?: number }} RankedCode */

/** Worker → main decode request. */
/** @typedef {{
 *   bitmap: ImageBitmap,
 *   formats?: string[],
 *   still?: boolean,
 *   accuracy?: Accuracy,
 *   latency?: number,
 * }} DecodeRequest
 */

/** @typedef {{ type: 'result', codes: DecodedCode[], decodeMs: number, partial?: boolean, checked?: string[] | null, qrPresence?: number, frameW?: number, frameH?: number } | { type: 'error', message: string }} DecodeResponse */

/**
 * One re-encode request. `ec`/`mask`/`version` are QR (raw 2-bit EC
 * field / mask 0..7 / version 1..40); `level`/`columns` are PDF417;
 * `rows`/`cols` are Data Matrix. Missing options arrive as `-1`/`0`,
 * i.e. the encoder default (see `src/render-options.js`).
 * @typedef {{
 *   symbology: string,
 *   text?: string,
 *   raw?: Uint8Array | null,
 *   module?: number,
 *   height?: number,
 *   ec?: number,
 *   mask?: number,
 *   version?: number,
 *   level?: number,
 *   columns?: number,
 *   rows?: number,
 *   cols?: number,
 * }} EncodeRequest
 */

/** @typedef {{ width: number, height: number, bytesPerRow: number, data: Uint8Array }} EncodeResponse */

/** Longest edge the still binarizer keeps (see `stillBinarizer`). */
const STILL_MAX = 2112;

const binarizerReady = createBinarizer({
  maxSize: 1056,
  windowDivisor: 20,
  threshold: 0.92,
});
// Gallery/photo stills get a larger cap: a 1D barcode is often a small
// part of a 4096px frame, and at 1056 its 1-module bars collapse to
// 2-3px and never decode (field photo: a La Farmacia loyalty card reads
// in every rotation at 2112, none at 1056). Live frames keep 1056 for
// the frame budget; stills have seconds and no FPS target. Created
// lazily so a live-only session never allocates the bigger texture.
let stillBinarizerPromise = null;
const stillBinarizer = () =>
  (stillBinarizerPromise ??= createBinarizer({
    maxSize: STILL_MAX,
    windowDivisor: 20,
    threshold: 0.92,
  }));
/**
 * Families a still decodes on a CPU mask in addition to the GPU one.
 *
 * The GPU mask's floor is a fixed `luma < 0.5` rescue (see binarize.wgsl),
 * which is load-bearing for faded thermal
 * receipts but force-inks a glossy label's midtone background. Two
 * measured failures share that root:
 * - the DataMatrix locator then spends the whole still budget on a glossy
 *   label's noise;
 * - a real version-6 QR on a woven mat (~3.4 px/module) fails the QR
 *   finder stage on the GPU mask at 1056 AND 2112, 1x AND 2x, while the
 *   CPU mask finds three finders and decodes it (desktop WebGPU and the
 *   phone agreed).
 *
 * The CPU mask thresholds against `0.5 x scene mean` and keeps the busy
 * background out, so one extra binarize (~76ms on a phone at
 * 2048x1152) reads what the GPU sweep cannot. It runs BEFORE the GPU
 * sweep: after the sweep is too late, the still law's exotic tiers can
 * consume the whole deadline on the busy mask (measured: 10s and no
 * room left). A CPU miss costs the binarize plus its cheap decode and
 * the GPU sweep still runs every family, so the fixed floor keeps its
 * receipts; a CPU read returns immediately, since one valid code already
 * satisfies the one-code contract.
 */
const STILL_CPU_FAMILIES = new Set(["datamatrix", "qrcode", "microqr"]);
/** The still CPU families that also escalate to native-resolution crops. */
const STILL_CPU_ROI_FAMILIES = new Set(["datamatrix"]);
/**
 * 2D families a CPU mask gets a SECOND OPINION for when the GPU path
 * missed. The fixed GPU floor over-inks a busy background: a real
 * version-6 QR on a woven mat (~3.4 px/module) failed the QR finder
 * stage on the GPU mask at 1056 AND 2112, 1x AND 2x, while the CPU mask
 * found three finders and decoded it (desktop WebGPU and the phone
 * agreed). Running the CPU mask only after the GPU miss keeps the
 * load-bearing fixed floor untouched and pays the extra binarize only on
 * frames that are already a miss.
 *
 * DataMatrix and still QR/Micro QR are excluded: they already run on the
 * CPU FIRST (see STILL_CPU_FAMILIES); live keeps DataMatrix's dedicated
 * ROI/diagonal paths.
 */
const CPU_SECOND_OPINION_FAMILIES = new Set(["qrcode", "microqr", "aztec", "pdf417"]);
/** The requested families a CPU still mask should answer (see above). */
const stillCpuFamilies = (/** @type {string[] | undefined} */ formats) =>
  (formats ?? ALL_FORMATS).filter((f) => STILL_CPU_FAMILIES.has(String(f).toLowerCase()));

/** The still CPU binarizer (lazily allocated, shared across scans). */
let stillCpuBinarizerInstance = null;
const stillCpuBinarizer = () =>
  (stillCpuBinarizerInstance ??= new CpuBinarizer({
    maxSize: STILL_MAX,
    windowDivisor: 20,
    threshold: 0.7,
  }));

/**
 * The live CPU second-opinion binarizer. Same frame geometry as the live
 * GPU binarizer (1056 longest edge) so quads map 1:1; a still-sized CPU
 * mask would both cost more per frame and need a coordinate remap.
 */
let liveCpuBinarizerInstance = null;
const liveCpuBinarizer = () =>
  (liveCpuBinarizerInstance ??= new CpuBinarizer({
    maxSize: 1056,
    windowDivisor: 20,
    threshold: 0.7,
  }));

/** The requested families a CPU second opinion should answer (GPU missed). */
const cpuSecondOpinionFormats = (
  /** @type {string[] | undefined} */ formats,
  /** @type {{ symbology: string }[]} */ codes,
  /** @type {Set<string> | null} */ skip = null,
) => {
  const seen = new Set(codes.map((c) => c.symbology));
  return (formats ?? ALL_FORMATS).filter((f) => {
    const name = String(f).toLowerCase();
    return CPU_SECOND_OPINION_FAMILIES.has(name) && !seen.has(name) && !skip?.has(name);
  });
};

/** Live: at most one CPU second-opinion pass per this many ms. */
const CPU_SECOND_OPINION_MIN_INTERVAL_MS = 400;
let lastCpuSecondOpinionAt = -Infinity;
/** Keep re-running after a hit so consecutive-frame confirmation can pass. */
let cpuSecondOpinionHotUntil = 0;

const scannerReady = getWasmScanner();
// the scanner init brings the whole wasm module up once — encode
// shares that instance (double init invalidates memory: 'unreachable')
const encoderReady = scannerReady;

/** Did the last FULL live sweep see a 2D code? Gates the linear tier. */
let lastFullHad2D = false;
/** Symbologies read on the previous live frame: re-checked next frame so
 *  the confirmation law does not have to wait for a scheduled sweep.
 *  @type {string[]} */
let lastCandidates = [];

/** The tiers worth a source-rotation retry (diagonal presentation). */
const DIAGONAL_FORMATS = new Set(["datamatrix", "aztec", "pdf417"]);
/** Live frames: at most one source-rotation retry per half second. */
let lastDiagonalAt = 0;

/**
 * The row-scan families. A still photo's 1D barcode is cheap to look for
 * (~tens of ms) while the exotic 2D sweep can consume the whole still
 * budget; photos of receipts/loyalty cards should not wait for it.
 */
const LINEAR_FORMATS = new Set([
  "upca", "ean13", "ean8", "upce", "code39", "code32", "code93", "code128",
  "itf", "codabar", "databar", "databarltd", "databarexpanded",
]);
const stillLinearFormats = (/** @type {string[] | undefined} */ formats) =>
  formats == null
    ? [...LINEAR_FORMATS]
    : formats.filter((f) => LINEAR_FORMATS.has(String(f).toLowerCase()));

/**
 * Attach the centering score to every code and sort best-first: the
 * quad centroid's distance from the frame center (normalized by the
 * half-diagonal, so 1 = dead center, 0 = at a corner), larger quad
 * area as tiebreak. Coordinates are mask-space — the same space the
 * quads are already in. Ranking only ever reorders: a peripheral code
 * still confirms when it is the only readable one, it just loses the
 * race to 2 consecutive frames when a centered one also reads.
 * @param {RankedCode[]} codes
 * @param {number} width
 * @param {number} height
 * @returns {RankedCode[]}
 */
const ranked = (codes, width, height) => {
  const cx = width / 2;
  const cy = height / 2;
  const half = Math.hypot(width, height) / 2 || 1;
  const area = (/** @type {RankedCode['position']} */ p) => {
    let a = 0;
    for (let i = 0; i < p.length; i++) {
      const cur = p[i] ?? { x: 0, y: 0 };
      const next = p[(i + 1) % p.length] ?? cur;
      a += cur.x * next.y - next.x * cur.y;
    }
    return Math.abs(a) / 2;
  };
  for (const c of codes) {
    let mx = 0;
    let my = 0;
    for (const p of c.position) {
      mx += p.x;
      my += p.y;
    }
    const d = Math.hypot(mx / c.position.length - cx, my / c.position.length - cy) / half;
    c.score = Math.max(0, Math.min(1, 1 - d));
  }
  return codes.sort((a, b) => ((b.score ?? 0) - (a.score ?? 0)) || (area(b.position) - area(a.position)));
};

/** Live: at most one native-ROI pass per this many ms (crop + binarize). */
const ROI_MIN_INTERVAL_MS = 250;
/** Max decode budget one still ROI candidate may consume. */
const ROI_BOX_CAP_MS = 1500;
let lastRoiAt = -Infinity;
/** Keep re-running the native pass for consecutive-frame confirmation. */
let roiHotUntil = 0;

/**
 * Native-resolution ROI pass (roadmap item 8): the full-frame mask says
 * WHERE the compact ink clusters are, the ORIGINAL bitmap still has the
 * pixels. Each candidate is cropped at source resolution, binarized
 * (a fitting crop is padded 1:1, never squeezed) and decoded.
 *
 * Results are mapped back into the FULL-FRAME mask coordinate space, the
 * same space every other worker result uses, so ranking/confirmation and
 * the hint close-up need no special case.
 *
 * @param {import('../src/decoder.js').DecodedCode[]} hits
 * @param {{x: number, y: number, map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[]}} crop
 * @param {{width: number, height: number}} croppedFrame
 * @param {{width: number, height: number}} fullFrame
 * @param {number} sourceW @param {number} sourceH
 */
const mapRoiHits = (hits, crop, croppedFrame, fullFrame, sourceW, sourceH) => {
  if (!hits.length) return hits;
  // crop mask -> source bitmap pixels -> full-frame mask pixels.
  const sx = sourceW <= fullFrame.width ? 1 : fullFrame.width / sourceW;
  const sy = sourceH <= fullFrame.height ? 1 : fullFrame.height / sourceH;
  for (const hit of hits) {
    if (!hit.position?.length) continue;
    const src = crop.map(croppedFrame.width, croppedFrame.height, hit.position);
    hit.position = src.map((p) => ({ x: p.x * sx, y: p.y * sy }));
  }
  return hits;
};

/**
 * Try the best native-resolution crops under a wall-clock deadline.
 * @param {ReturnType<typeof getWasmScanner> extends Promise<infer S> ? S : never} scanner
 * @param {Awaited<ReturnType<typeof createBinarizer>>} bin
 * @param {ImageBitmap} bitmap
 * @param {{width: number, height: number, data: Uint8Array, bytesPerRow: number}} frame
 * @param {string[] | undefined} formats
 * @param {{still?: boolean, latency?: number}} opts
 * @param {number} limit @param {number} deadline
 * @returns {Promise<import('../src/decoder.js').DecodedCode[]>}
 */
const decodeRoi = async (scanner, bin, bitmap, frame, formats, opts, limit, deadline) => {
  const sourceW = bitmap.width;
  const sourceH = bitmap.height;
  if (!sourceW || !sourceH) return [];
  const wantsLinear =
    !formats?.length || formats.some((f) => LINEAR.includes(String(f).toLowerCase()));
  /** The requested 1D families only: a bar strip must never pay for a 2D
   *  tier (DataMatrix/PDF417 on the strip can spend the whole live
   *  deadline before the row scanners run). */
  const linearOnly = formats?.length
    ? formats.filter((f) => LINEAR.includes(String(f).toLowerCase()))
    : [...LINEAR];
  // Linear bands first: a wide bar strip is often not a "compact cluster"
  // (it merges with the page/box around it, or loses to the text below).
  const linear = wantsLinear
    ? linearRoiCandidates(frame, sourceW, sourceH, limit).map((box) => ({ ...box, linear: true }))
    : [];
  /** @type {{ x: number, y: number, w: number, h: number, score: number, linear?: boolean }[]} */
  const boxes = [];
  for (const box of [...linear, ...roiCandidates(frame, sourceW, sourceH, limit)]) {
    const overlap = boxes.some((p) => {
      const ix = Math.max(0, Math.min(box.x + box.w, p.x + p.w) - Math.max(box.x, p.x));
      const iy = Math.max(0, Math.min(box.y + box.h, p.y + p.h) - Math.max(box.y, p.y));
      // Two linear proposals for the SAME strip at different padding are
      // both worth a crop: the tight one reads a marginal dark-on-white
      // photo, the generous one keeps the quiet-zone margin an inverted
      // (white-on-black) symbol needs. Only near-identical sizes dedupe.
      const isLinear = /** @type {{ linear?: boolean }} */ (box).linear;
      if (isLinear && p.linear && Math.max(box.w / p.w, p.w / box.w) > 1.15) return false;
      return ix * iy > 0.6 * Math.min(box.w * box.h, p.w * p.h);
    });
    if (!overlap) boxes.push(box);
    // The caller's crop budget is the bound: extra candidates multiply
    // the wasm sweep, and a junk frame produces no valid crop anyway.
    // Linear candidates now come in two flavours (tight dense-row bands
    // plus the looser original proposals), so allow the extra pair; the
    // deadline below still caps the actual work.
    if (boxes.length >= limit + 2) break;
  }
  if (!boxes.length) return [];
  bin.setThreshold?.(0.92);
  const remaining = () => deadline - performance.now();
  const callOpts = () =>
    opts.still === true
      ? {
          still: true,
          latency: Math.max(250, Math.min(opts.latency ?? 5000, remaining(), ROI_BOX_CAP_MS)),
        }
      : opts;
  /** @type {{ box: { x: number, y: number, w: number, h: number, linear?: boolean }, crop: Awaited<ReturnType<typeof cropDrawable>> }[]} */
  const crops = [];
  /**
   * @param {DecodedCode[]} hits
   * @param {import('../src/gpu/binarizer.js').BinarizedFrame} mask
   * @param {{ x: number, y: number, width: number, height: number, map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[] }} mapper
   */
  const done = (hits, mask, mapper) => mapRoiHits(hits, mapper, mask, frame, sourceW, sourceH);
  /** The short-checksum EAN/UPC family: on a marginal Code 128 strip the
   *  EAN-8 tier can fit a guard + 8 digits on two rows and pass its 1/10
   *  check digit. Such a hit is a fallback, never a reason to stop before
   *  the Code 128 (1/103) opinion has had its chance. */
  const WEAK = new Set(['ean13', 'ean8', 'upce', 'upca']);
  /** @type {any} */
  let weakHit = null;
  /**
   * Return the strong hits of a stage (and only those); remember a
   * weak-only stage as the eventual fallback.
   * @param {DecodedCode[]} hits
   * @param {import('../src/gpu/binarizer.js').BinarizedFrame} mask
   * @param {any} mapper
   */
  const offer = (hits, mask, mapper) => {
    if (!hits.length) return null;
    const strong = hits.filter((h) => !WEAK.has(h.symbology));
    if (strong.length) return done(strong, mask, mapper);
    weakHit ??= { hits, mask, mapper };
    return null;
  };

  // Pass 1: cheap opinions on every candidate. The 0.92/inverted pair is
  // the established path; for a linear strip a 0.88 SOURCE mirror and the
  // unmirrored 0.88 follow. Cheap-first matters: the inverted island's
  // readable crop is the generous box AFTER the tight one, and Otsu must
  // not spend the live deadline before that box is even tried.
  for (const box of boxes) {
    if (remaining() <= 0) break;
    const crop = await cropDrawable(bitmap, box);
    crops.push({ box, crop });
    // A bar strip is decoded by the row scanners only.
    const boxFormats = box.linear ? linearOnly : formats;
    /** @type {import('../src/gpu/binarizer.js').BinarizedFrame} */
    const f = await bin.process(crop.canvas);
    let found = offer(decodeFrame(scanner, f, boxFormats, callOpts()), f, crop);
    if (found) return found;
    const inverted = { ...f, data: f.data.map((/** @type {number} */ b) => ~b) };
    found = offer(decodeFrame(scanner, inverted, boxFormats, callOpts()), f, crop);
    if (found) return found;
    if (!box.linear) continue;

    // The quiet zone is the other half of the tight-band lever. A real
    // label's printed quiet zone can sit below the 10 modules Code 128
    // mandates (the DHL/GoFo sample measures ~6 on one side), and the
    // dense-row band does not widen it. The row scanners then reject an
    // otherwise clean strip that zxing reads, because zxing is lenient
    // about quiet zones. Drawing the crop onto paper white restores the
    // mandated run of white at no extra camera-frame cost, and this
    // stage also carries the marginal 0.88 threshold.
    const padPx = Math.max(16, Math.min(96, Math.round(box.h)));
    const padded = await padDrawable(crop.canvas, padPx);
    bin.setThreshold(0.88);
    const fp = await bin.process(padded.canvas);
    const padMap = {
      ...crop,
      /** @param {number} fw @param {number} fh @param {{x: number, y: number}[]} points */
      map: (fw, fh, points) => crop.map(crop.width, crop.height, padded.map(fw, fh, points)),
    };
    found = offer(decodeFrame(scanner, fp, boxFormats, callOpts()), fp, padMap);
    if (found) {
      bin.setThreshold(0.92);
      return found;
    }

    // The source mirror is the upside-down phone lever;
    // both opinions live at 0.88, the measured threshold for a marginal
    // strip. The mirror is unconditional: a deadline check here cut it on
    // the full (slower) frames while the cheap frames read it.
    const mirrored = await mirrorDrawable(crop.canvas);
    const fm = await bin.process(mirrored.canvas);
    const mirrorMap = {
      ...crop,
      /** @param {number} fw @param {number} fh @param {{x: number, y: number}[]} points */
      map: (fw, fh, points) =>
        mirrored.map(fw, fh, points).map((p) => ({ x: crop.x + p.x, y: crop.y + p.y })),
    };
    found = offer(decodeFrame(scanner, fm, boxFormats, callOpts()), fm, mirrorMap);
    if (found) {
      bin.setThreshold(0.92);
      return found;
    }
    const f88 = await bin.process(crop.canvas);
    found = offer(decodeFrame(scanner, f88, boxFormats, callOpts()), f88, crop);
    if (found) {
      bin.setThreshold(0.92);
      return found;
    }
    bin.setThreshold(0.92);
  }

  // Pass 2: the local threshold floods thin spaces on a blurry bar strip;
  // a global Otsu split over a 2x upscale keeps them white. That is the
  // distant/out-of-focus 1D lever (a real Code 32 package measured 0
  // reads locally, reads at 2x Otsu). Only linear crops, only after every
  // cheap opinion missed. The unpadded crop keeps the full 2x upscale:
  // padding pushes the padded frame over `otsuMask`'s 1600px cap and the
  // smaller factor loses the thin spaces this pass exists for.
  for (const { box, crop } of crops) {
    if (!box.linear || remaining() <= 0) continue;
    /** @param {number} fw @param {number} fh @param {{x: number, y: number}[]} points */
    const scaledMap = (fw, fh, points) =>
      points.map((p) => ({
        x: crop.x + (p.x * crop.width) / fw,
        y: crop.y + (p.y * crop.height) / fh,
      }));
    const scaled = { ...crop, map: scaledMap };
    for (const scale of [2, 1]) {
      if (remaining() <= 0) break;
      const otsu = otsuMask(crop.canvas, scale);
      for (const data of [otsu, { ...otsu, data: otsu.data.map((/** @type {number} */ b) => ~b) }]) {
        const found = offer(decodeFrame(scanner, data, linearOnly, callOpts()), otsu, scaled);
        if (found) return found;
      }
    }
  }
  return weakHit ? done(weakHit.hits, weakHit.mask, weakHit.mapper) : [];
};

/**
 * The handler pair. One wasm instance, one binarizer and one scanner
 * serve the single bound session; the main thread's own convention is
 * one frame in flight and requests serialize through a queue besides.
 */
const handlers = {
  /**
   * One frame in — best-ranked candidates out. `msg.bitmap` is a
   * transferred ImageBitmap (closed here); `msg.still` selects the
   * still law (default 5000ms) over the live one (500ms), both
   * overridable via `msg.latency`. `accuracy`:
   * 'low' = one pass, 'auto' = lazy second threshold when the
   * first pass finds nothing, 'high' = always sweep.
   * Resolves { type: 'result', codes, decodeMs } or, on failure,
   * { type: 'error', message } — never throws across the port.
   * @param {DecodeRequest} msg
   * @returns {Promise<DecodeResponse>}
   * @this {Bindings}
   */
  async decode(msg) {
    /** Whether this frame runs the full exotic sweep (for the scheduler). */
    let full = true;
    try {
      const { bitmap, formats, still, accuracy, latency } = msg ?? {};
      if (!bitmap) throw new Error('decode: missing bitmap');
      // Live-frame scheduling: the exotic 2D families run at most every
      // 250ms of wall time; the cheap ones (QR/Micro/Aztec/DataBarStk)
      // run on every frame. Still images always get the full sweep.
      let activeFormats = formats;
      /** Families actually decoded this frame: null = everything requested. @type {string[] | null} */
      let checked = null;
      const scheduled = still !== true && needsScheduling(formats);
      if (scheduled) {
        full = dueForFullSweep(performance.now());
        if (!full) {
          // Cheap frames keep the linear tier alive unless the last full
          // sweep actually saw a 2D code (the false-positive gate).
          activeFormats = cheapFormats(formats, !lastFullHad2D);
          // Re-confirmation boost: a code read on the previous frame gets
          // its tier re-checked on THIS frame. Without it the second
          // agreeing read waited for the next scheduled full sweep (a
          // screen PDF417 measured 17.4s in Everything mode). The boost
          // keeps the 2D-first order — it only appends a family that was
          // just observed. Result names are already canonical, so this
          // is the same vocabulary as `activeFormats` and `formats`.
          if (activeFormats.length) {
            const seen = new Set(activeFormats);
            for (const symbology of lastCandidates) {
              if (symbology && !seen.has(symbology)) {
                seen.add(symbology);
                activeFormats.push(symbology);
              }
            }
          }
          checked = activeFormats;
          if (!activeFormats.length) {
            return { type: 'result', codes: [], decodeMs: 0, partial: true, checked };
          }
        }
      }
      const bin = await (still === true ? stillBinarizer() : binarizerReady);
      const scanner = await scannerReady;
      const t0 = performance.now();
      // Gallery photos are 12MP and the GPU luma stage reaches 2112 with a
      // single bilinear tap per destination pixel: that aliases a small
      // symbol's module pitch away (the same DataMatrix read in 1.7s when
      // the source had already been area-filtered by the browser, never
      // at native 12MP). Area-filter the still HERE with the browser's
      // resampler — but keep the ORIGINAL bitmap for the native-resolution
      // ROI pass below, which needs those pixels.
      /** @type {ImageBitmap | OffscreenCanvas} */
      let input = bitmap;
      if (still === true) {
        const longest = Math.max(bitmap.width, bitmap.height);
        if (longest > STILL_MAX) {
          const scale = STILL_MAX / longest;
          const w = Math.max(1, Math.round(bitmap.width * scale));
          const h = Math.max(1, Math.round(bitmap.height * scale));
          const canvas = new OffscreenCanvas(w, h);
          const ctx = /** @type {OffscreenCanvasRenderingContext2D | null} */ (canvas.getContext('2d'));
          if (ctx) {
            ctx.imageSmoothingEnabled = true;
            ctx.imageSmoothingQuality = 'high';
            ctx.drawImage(bitmap, 0, 0, w, h);
            input = canvas;
          }
        }
      }
      const frame = await bin.process(input);
      const opts = { still, latency };
      // Stills have ONE hard deadline: `latency` bounds the whole decode,
      // and every stage below — threshold ladder, full sweep, source
      // rotation, native ROI — shares it. Live frames keep their per-frame
      // scheduling (the confirmation law drops over-budget frames instead).
      const deadline =
        still === true ? t0 + (latency ?? 5000) : Number.POSITIVE_INFINITY;
      const remaining = () => deadline - performance.now();
      const outOfTime = () => performance.now() >= deadline;
      /** Per-call opts: the still tiers take their own budget (DM/PDF417). */
      const budgetOpts = () =>
        still === true ? { ...opts, latency: Math.max(250, remaining()) } : opts;
      // Stills: try the linear tier alone first, across a small threshold
      // ladder. A 2112px loyalty-card photo measured 6s for the full
      // all-format sweep and 30ms for the 1D tier — and the barcode only
      // appeared at threshold 0.7 (the card's white label sits on a
      // light-teal background, so 0.92/0.82 miss it). Live frames keep
      // the tier order; their linear pass is already every frame.
      /** @type {DecodedCode[]} */
      let codes = [];
      let stillLinearHit = false;
      if (still === true && typeof bin.setThreshold === "function") {
        const linear = stillLinearFormats(activeFormats);
        if (linear.length) {
          const thresholds = accuracy === "low" ? [0.92] : [0.92, 0.82, 0.7];
          for (const threshold of thresholds) {
            if (outOfTime()) break;
            let f = frame;
            if (threshold !== 0.92) {
              bin.setThreshold(threshold);
              f = await bin.process(input);
            }
            const lin = decodeFrame(scanner, f, linear, opts);
            if (lin.length) {
              codes = lin;
              stillLinearHit = true;
              break;
            }
          }
          bin.setThreshold(0.92);
        }
      }
      // GPU stills: a first opinion for STILL_CPU_FAMILIES (see above).
      // A hit also drops those families from the GPU sweep below, which is
      // where the seconds go on a glossy label. A miss is capped so a hard
      // negative cannot eat the deadline before the GPU tiers run.
      /** @type {DecodedCode[]} */
      let cpuCodes = [];
      if (still === true && bin.kind === "gpu") {
        const families = stillCpuFamilies(activeFormats);
        if (families.length && !outOfTime()) {
          // `input` (not `bitmap`) is the same area-filtered source the GPU
          // frame came from: a >2112px photo must not take a second,
          // different downscale path here.
          const cpuBin = stillCpuBinarizer();
          const cpuLatency = () => Math.min(budgetOpts().latency ?? 2000, 2000);
          /** @param {DecodedCode[]} hits @returns {boolean} whether any was new */
          const add = (hits) => {
            const seen = new Set(cpuCodes.map((c) => c.symbology + '\u0000' + c.text));
            let got = false;
            for (const hit of hits) {
              const key = hit.symbology + '\u0000' + hit.text;
              if (!seen.has(key)) {
                seen.add(key);
                cpuCodes.push(hit);
                got = true;
              }
            }
            return got;
          };
          const isRoi = (/** @type {string} */ f) => STILL_CPU_ROI_FAMILIES.has(String(f).toLowerCase());
          const roiFamilies = families.filter(isRoi);
          const cheapFamilies = families.filter((f) => !isRoi(f));
          /** The 0.7 full-frame mask, shared by the cheap ladder and the
           *  DataMatrix first opinion. @type {import('../src/gpu/binarizer.js').BinarizedFrame | null} */
          let frame07 = null;
          // QR/Micro QR: 0.7 then 0.82. The captured phone frame reads at
          // 0.75/0.82 but NOT at 0.7 when the 1080 source pads up to 1088;
          // the zoomed frame reads at 0.7. A hit ends the ladder (one
          // code per scan; another threshold only re-reads the same value).
          // This extra opinion uses the LIVE law: the still law's forced
          // inverted retry turned one CPU miss into ~2s, while a mostly-ink
          // mask still gets its inverted pass, and an inverted code
          // escalates to the GPU path below.
          for (const threshold of [0.7, 0.82]) {
            if (!cheapFamilies.length || outOfTime()) break;
            const seen = new Set(cpuCodes.map((c) => c.symbology));
            if (cheapFamilies.every((f) => seen.has(String(f).toLowerCase()))) break;
            let cpuMask = threshold === 0.7 ? frame07 : null;
            if (!cpuMask) {
              cpuBin.setThreshold(threshold);
              const mask = await cpuBin.process(input);
              cpuMask = mask;
              if (threshold === 0.7) frame07 = mask;
            }
            if (cpuMask && add(decodeFrame(scanner, cpuMask, cheapFamilies, { ...opts, still: false }))) break;
          }
          // DataMatrix first opinion, only when the cheap tier read nothing:
          // its 0.7 full-frame low-contrast mask plus the still law's
          // forced inverted retry costs up to 2s on a busy label, and a
          // QR/Micro read already satisfies the one-code contract.
          if (!cpuCodes.length && roiFamilies.length && !outOfTime()) {
            if (!frame07) {
              cpuBin.setThreshold(0.7);
              frame07 = await cpuBin.process(input);
            }
            const mask07 = frame07;
            if (mask07) add(decodeFrame(scanner, mask07, roiFamilies, { ...opts, latency: cpuLatency() }));
          }
          // The full 2112 frame can still crush a 2px/module symbol; the
          // same native-pitch crops the desktop CPU path uses read it
          // (3072px crop: 842ms desktop). Rank on a 0.92 opinion — the
          // 0.7 mask is for the full-frame low-contrast symbol, not for
          // picking compact clusters. Only the DataMatrix family escalates:
          // QR/Micro QR read on the full CPU mask or not at all.
          const cpuHit = new Set(cpuCodes.map((c) => c.symbology));
          const roiLeft = roiFamilies.filter((f) => !cpuHit.has(String(f).toLowerCase()));
          if (roiLeft.length && !outOfTime()) {
            cpuBin.setThreshold?.(0.92);
            const cpuRoiFrame = await cpuBin.process(input);
            cpuCodes = cpuCodes.concat(
              await decodeRoi(
                scanner,
                cpuBin,
                bitmap,
                cpuRoiFrame,
                roiLeft,
                budgetOpts(),
                4,
                Math.min(performance.now() + 3000, deadline),
              ),
            );
          }
          if (cpuCodes.length) codes = codes.concat(cpuCodes);
        }
      }
      /** Families actually decoded on the CPU still mask. */
      const cpuHit = new Set(cpuCodes.map((c) => c.symbology));
      // A still can carry BOTH a linear code and a 2D one (a product box:
      // EAN-13 and a GS1 DataMatrix on the same label). The linear
      // fast-path above must not hide the 2D tier: when the caller asked
      // for 2D families too, try them on the same still and let ranking
      // pick the more centered code. The compact-cluster ROI reads the
      // ORIGINAL bitmap at native pitch (cheap, tens of ms); when it
      // misses, one full sweep at the 0.7 threshold finds low-contrast
      // labels that the 0.92 default washes out (measured: a glossy GS1
      // DataMatrix decodes only at 0.7).
      if (still === true && stillLinearHit) {
        const twoD = (activeFormats ?? ALL_FORMATS).filter(
          (f) =>
            !LINEAR_FORMATS.has(String(f).toLowerCase()) &&
            // answered on the CPU mask already
            !cpuHit.has(String(f).toLowerCase()),
        );
        if (twoD.length && !outOfTime()) {
          const roi = await decodeRoi(
            scanner,
            bin,
            bitmap,
            frame,
            twoD,
            budgetOpts(),
            2,
            Math.min(performance.now() + 1500, deadline),
          );
          if (roi.length) codes = codes.concat(roi);
          else if (!outOfTime() && typeof bin.setThreshold === "function") {
            bin.setThreshold(0.7);
            const alt = await bin.process(input);
            bin.setThreshold(0.92);
            const seen = new Set(codes.map((c) => c.symbology + '\u0000' + c.text));
            codes = codes.concat(
              decodeFrame(scanner, alt, twoD, budgetOpts()).filter(
                (c) => !seen.has(c.symbology + '\u0000' + c.text),
              ),
            );
            checked = null;
          }
        }
      }
      // Small 1D codes (the roadmap's receipt lever): the full-frame
      // 2112px ladder can still leave a bar code at 2-3px/module. The
      // compact clusters the mask already shows are cropped from the
      // ORIGINAL photo and read at native pitch before the exotic sweep.
      if (still === true && !stillLinearHit && codes.length === 0) {
        const linear = stillLinearFormats(activeFormats);
        if (linear.length) {
          const roi = await decodeRoi(
            scanner,
            bin,
            bitmap,
            frame,
            linear,
            opts,
            2,
            Math.min(performance.now() + 900, t0 + (latency ?? 5000)),
          );
          if (roi.length) {
            codes = roi;
            stillLinearHit = true;
          }
        }
      }
      // A CPU read already satisfies the one-code contract: the remaining
      // exotic tiers on the busy mask cost seconds (measured: the GPU
      // sweep spent the rest of the 10s still deadline after the CPU had
      // read the QR), so only an empty CPU result escalates to the GPU.
      const sweepFormats = activeFormats;
      if (!stillLinearHit && !cpuHit.size && sweepFormats?.length !== 0) {
        // Cheap-first escalation: on a scheduled FULL frame, decode the
        // cheap tier (QR/Micro/Aztec/DataBarStk + linear when the 2D gate
        // allows) FIRST — QR/1D hits then confirm for a fraction of the
        // full sweep's cost on slow phones (a 920px phone frame measured
        // 104ms cheap vs 543ms all-17). Only a miss pays for the exotic
        // remainder (DataMatrix, PDF417), in ONE combined call: family
        // costs are additive within a call but two calls re-pay shared
        // preprocessing, so cheap-then-exotic costs more than one union
        // call (measured 630ms vs 543ms) — never split further.
        if (full && scheduled) {
          const cheapList = cheapFormats(sweepFormats, !lastFullHad2D);
          // `decodeFrame` treats an empty formats list as "everything":
          // only run the cheap pass when it actually selected families.
          if (cheapList.length) {
            codes = decodeFrame(scanner, frame, cheapList, budgetOpts());
            checked = cheapList;
          }
          // 2D-before-1D: a 1D hit can be noise from an exotic 2D symbol's
          // texture. When the cheap pass included the linear tier and
          // returned only linear results, the exotic tiers must still run
          // before this frame can vote — the scheduling gate protects
          // cheap frames, so full frames must keep the same order.
          const linearOnly =
            codes.length > 0 && codes.every((c) => !is2DResult(c.symbology));
          if (!codes.length || linearOnly) {
            const ranCheap = new Set(cheapList.map((f) => String(f).toLowerCase()));
            const rest = (sweepFormats ?? ALL_FORMATS).filter(
              (f) => !ranCheap.has(String(f).toLowerCase()),
            );
            // Families are disjoint, so this only ever appends: a linear
            // hit must not be lost when the exotic tiers miss.
            if (rest.length) codes = codes.concat(decodeFrame(scanner, frame, rest, budgetOpts()));
            // cheap + rest cover every requested family.
            checked = null;
          }
        } else {
          // Append, never assign: a CPU still first opinion may already
          // have read a code (see STILL_CPU_FAMILIES).
          codes = codes.concat(decodeFrame(scanner, frame, sweepFormats, budgetOpts()));
        }
        const firstMs = performance.now() - t0;
        // 'auto' = lazy second threshold when the first pass finds nothing.
        // On live camera frames that is the common case, and a second
        // binarize+decode doubles the frame cost — the exact frames where
        // the user is still aiming. Skip the sweep when the first pass
        // already spent (or nearly spent) the live budget: a stale second
        // opinion cannot help that frame. Still scans keep the sweep while
        // their 5s law leaves room.
        // A still ALWAYS keeps the sweep: its deadline is the bound, and
        // a low-contrast label can need the 0.7 opinion even when the
        // first pass alone took seconds (headless desktop measured
        // >2500ms). Live frames still skip it when the first pass nearly
        // spent the frame budget.
        const sweep =
          accuracy === 'high' ||
          (accuracy === 'auto' && codes.length === 0 && (still || firstMs < 220));
        // second Bradley threshold over the SAME frame (glare AND
        // shadow variants), deduped against the first pass. Stills get a
        // third opinion at 0.7: a glossy/curved product label can wash out
        // at 0.92/0.82 and read cleanly at 0.7 (field: a GS1 DataMatrix
        // that only the linear ladder's low threshold recovered).
        if (sweep && !outOfTime() && typeof bin.setThreshold === "function") {
          const thresholds = still === true ? [0.82, 0.7] : [0.82];
          const seen = new Set(codes.map((c) => c.symbology + '\u0000' + c.text));
          for (const threshold of thresholds) {
            if (outOfTime()) break;
            bin.setThreshold(threshold);
            const alt = await bin.process(input);
            bin.setThreshold(0.92);
            const hits = decodeFrame(scanner, alt, sweepFormats, budgetOpts()).filter(
              (c) => !seen.has(c.symbology + '\u0000' + c.text),
            );
            for (const hit of hits) seen.add(hit.symbology + '\u0000' + hit.text);
            codes = codes.concat(hits);
            // The alternate-threshold passes decode every requested family.
            checked = null;
          }
        }
      }
      // CPU second opinion for small/dense 2D codes (roadmap item 10):
      // the GPU mask's fixed floor can over-ink a busy background and starve
      // the finder detector, while the CPU mask (thresholded against the
      // scene mean) reads it. Only after a GPU 2D miss, so the load-bearing
      // GPU path is untouched and the extra binarize is paid on misses; a
      // CPU-only browser already decoded on its CPU path, so it skips this.
      // Live is rate-limited (and stays hot for 600ms after a hit, so the
      // consecutive-frame confirmation law can see it) because a QR is a
      // cheap-tier family and this would otherwise run on every frame.
      if (bin.kind === "gpu" && !codes.some((c) => is2DResult(c.symbology))) {
        const families = cpuSecondOpinionFormats(activeFormats, codes, still === true ? STILL_CPU_FAMILIES : null);
        const now = performance.now();
        const due =
          still === true ||
          now <= cpuSecondOpinionHotUntil ||
          now - lastCpuSecondOpinionAt >= CPU_SECOND_OPINION_MIN_INTERVAL_MS;
        // The CPU binarize itself is not deadline-aware; leave it room.
        if (families.length && due && (still !== true || remaining() > 250)) {
          lastCpuSecondOpinionAt = now;
          const cpuBin = still === true ? stillCpuBinarizer() : liveCpuBinarizer();
          cpuBin.setThreshold(0.7);
          const cpuFrame = await cpuBin.process(input);
          const seen = new Set(codes.map((c) => c.symbology + '\u0000' + c.text));
          const hits = decodeFrame(scanner, cpuFrame, families, budgetOpts()).filter(
            (c) => !seen.has(c.symbology + '\u0000' + c.text),
          );
          if (hits.length) {
            // Same maxSize as the GPU binarizer in use (STILL_MAX / 1056),
            // so this is 1:1; scale anyway if a source ever rounds the two
            // fit paths differently.
            if (cpuFrame.width !== frame.width || cpuFrame.height !== frame.height) {
              const sx = frame.width / cpuFrame.width;
              const sy = frame.height / cpuFrame.height;
              for (const hit of hits) {
                hit.position = hit.position.map((p) => ({ x: p.x * sx, y: p.y * sy }));
              }
            }
            codes = codes.concat(hits);
            cpuSecondOpinionHotUntil = now + 600;
          }
        }
      }
      // Native-resolution ROI pass (roadmap item 8): the full sweep
      // missed, but a compact ink cluster may be a code that the frame
      // downscale crushed. Still photos get up to four crops inside the
      // caller's deadline; live frames get two crops twice a second.
      if (codes.length === 0) {
        const now = performance.now();
        // Live ROI hits must repeat on consecutive frames: the
        // confirmation law rejects a value that skips one. The
        // cooldown only gates the SEARCH for a new candidate.
        if (still === true || now <= roiHotUntil || now - lastRoiAt >= ROI_MIN_INTERVAL_MS) {
          lastRoiAt = now;
          const roi = await decodeRoi(
            scanner,
            bin,
            bitmap,
            frame,
            activeFormats,
            budgetOpts(),
            still === true ? 4 : 2,
            still === true ? deadline : now + 120,
          );
          if (roi.length) {
            codes = roi;
            roiHotUntil = now + 600;
          }
        }
      }
      // Dim still retry, then the still slight-roll retry.
      //
      // DIM: the binarizers stretch a low-exposure, low-contrast frame to
      // 0..1 before the Bradley threshold. That is right for a faded
      // receipt, but a capture with few distinct levels (synthetic
      // light=0.4: label paper 0.37, print 0.04) loses module edges, and
      // zxing -- which thresholds the grayscale its own way -- still reads
      // it. Re-drawing the SOURCE brighter skips the gate and reads the
      // same frame: measured on the 1D differential, the dim cells recover
      // only once the lift is combined with the small-roll ladder.
      // `liftDim` applies the gate itself and returns null for a normal
      // frame, so an ordinary still miss pays one 64px probe (~1ms).
      //
      // ROLL: the row scanners assume near-vertical bars. A careless-wrist
      // roll (Code 39 / DataBar at 6°) defeats them while zxing reads the
      // very same frame. Rotate the (lifted when dim) SOURCE with
      // antialiasing and re-binarize. The ±3 pair is the measured DataBar
      // lever (a 6° roll reads once the residual is 2-3°; Code 39 needs
      // ±4); cheapest first.
      //
      // Stills only: a lift/rotation + binarize is 20-60ms, which a live
      // frame cannot pay. Only requested families run; the cheap stacked
      // tier is included because the dim differential measured it.
      if (still === true && codes.length === 0 && remaining() > 300) {
        const liftFormats = (activeFormats ?? [...LINEAR_FORMATS, 'databarstk']).filter((f) => {
          const n = String(f).toLowerCase();
          return LINEAR_FORMATS.has(n) || n === 'databarstk';
        });
        const linear = stillLinearFormats(activeFormats);
        if (liftFormats.length) {
          const thresholds = [0.92, 0.82, 0.7];
          // `input` is the same area-filtered source the straight ladder
          // binarized (<= STILL_MAX); lifting the ORIGINAL 12MP bitmap
          // would pay a 12MP pixel pass and then take the binarizer's
          // aliasing bilinear downscale. Positions stay in mask space.
          const lifted = await liftDim(input);
          if (lifted) {
            for (const threshold of thresholds) {
              if (remaining() < 250) break;
              bin.setThreshold(threshold);
              const df = await bin.process(lifted.canvas);
              bin.setThreshold(0.92);
              const hits = decodeFrame(scanner, df, liftFormats, budgetOpts());
              if (hits.length) {
                codes = hits;
                break;
              }
            }
            bin.setThreshold(0.92);
          }
          const source = lifted?.canvas ?? input;
          for (const deg of [-3, 3, -4, 4, -8, 8]) {
            if (codes.length || remaining() < 250 || !linear.length) break;
            const rotated = await rotateDrawable(source, deg);
            for (const threshold of thresholds) {
              if (remaining() < 250) break;
              bin.setThreshold(threshold);
              const df = await bin.process(rotated.canvas);
              bin.setThreshold(0.92);
              const hits = decodeFrame(scanner, df, linear, budgetOpts());
              if (hits.length) {
                for (const hit of hits) {
                  if (hit.position?.length) {
                    hit.position = rotated.map(df.width, df.height, hit.position);
                  }
                }
                codes = hits;
                break;
              }
            }
            bin.setThreshold(0.92);
            if (codes.length) break;
          }
        }
      }
      // Diagonal retry (roadmap item 4): a mask-level 45° rotation
      // cannot restore Aztec's bullseye from a mask the camera already
      // resampled; rotating the SOURCE with antialiasing and binarizing
      // again does. The wasm pass stays for the cheap cases.
      if (codes.length === 0 && (still === true || formats != null)) {
        const diag = activeFormats == null
          ? [...DIAGONAL_FORMATS]
          : activeFormats.filter((n) => DIAGONAL_FORMATS.has(String(n).toLowerCase()));
        const now = performance.now();
        // Live: only explicit diagonal tiers, at most twice a second (the
        // rotation + extra binarize is ~20-40ms). Stills have seconds.
        const due = still === true || (diag.length > 0 && now - lastDiagonalAt >= 500);
        if (diag.length > 0 && due && !outOfTime()) {
          lastDiagonalAt = now;
          bin.setThreshold(0.92);
          for (const deg of [45, -45]) {
            if (outOfTime()) break;
            const rotated = await rotateDrawable(input, deg);
            const df = await bin.process(rotated.canvas);
            const hits = decodeFrame(scanner, df, diag, budgetOpts());
            if (hits.length) {
              for (const hit of hits) {
                if (hit.position?.length) {
                  hit.position = rotated.map(df.width, df.height, hit.position);
                }
              }
              codes = hits;
              break;
            }
          }
        }
      }
      if (full) lastFullHad2D = codes.some((c) => is2DResult(c.symbology));
      lastCandidates = codes.map((c) => c.symbology);
      return {
        type: 'result',
        codes: ranked(codes, frame.width, frame.height),
        decodeMs: Math.round(performance.now() - t0),
        frameW: frame.width,
        frameH: frame.height,
        // Backward-compatible flag: this was a cheap (scheduled) frame.
        // The confirmation law must not treat it as evidence of absence —
        // see `checked` for the per-family refinement.
        partial: !full,
        // Families this frame actually decoded: null = every requested
        // family ran, an array = the subset that ran. The confirmation
        // law counts values from these families and freezes the rest — a
        // QR seen on a cheap frame is real evidence and may confirm on
        // the next cheap frame, without waiting for a full sweep. A
        // cheap-first full frame that short-circuits on a hit therefore
        // reports the cheap set, not `null`.
        checked,
        // Presence signal for the auto-torch policy: 0 = no finders,
        // 1 = QR present but not decoding, 2 = decoded. Only meaningful
        // when the QR tier ran on this frame.
        qrPresence: (() => {
          const qrRan = !activeFormats?.length || activeFormats.some((f) => f === 'qr' || f === 'qrcode');
          return qrRan ? scanner.qr_presence() : undefined;
        })(),
      };
    } catch (err) {
      console.error('decode failed:', err);
      return { type: 'error', message: String(err instanceof Error ? err.message : err) };
    } finally {
      if (full) markFullSweep(performance.now());
      msg?.bitmap?.close?.();
    }
  },

  /**
   * Symbology + text (or `raw` bytes) in — raw module bitmap out
   * (width, height, bytesPerRow, data transferred). The main
   * thread turns it into SVG/canvas/Image. A non-empty `raw`
   * encodes the exact bytes (binary QR byte mode etc.).
   * @param {EncodeRequest} msg
   * @returns {Promise<EncodeResponse>}
   * @this {Bindings}
   */
  async encode(msg) {
    await encoderReady;
    const module = msg.module ?? 4;
    const height = msg.height ?? 30 * module;
    // Sentinel defaults preserve the historical encoders when a caller
    // omits an option (the wasm side documents the ranges).
    const ec = msg.ec ?? -1;
    const mask = msg.mask ?? -1;
    const version = msg.version ?? 0;
    const level = msg.level ?? -1;
    const columns = msg.columns ?? 0;
    const rows = msg.rows ?? 0;
    const cols = msg.cols ?? 0;
    const fx =
      msg.raw instanceof Uint8Array && msg.raw.length
        ? encode_fixture_raw_opts(
            msg.symbology, msg.raw, module, height,
            ec, mask, version, level, columns, rows, cols,
          )
        : encode_fixture_opts(
            msg.symbology, msg.text ?? '', module, height,
            ec, mask, version, level, columns, rows, cols,
          );
    const data = fx.data instanceof Uint8Array ? fx.data : Uint8Array.from(fx.data);
    (/** @type {Record<symbol, unknown>} */ (this))[options] = { transfer: [data.buffer] };
    return { width: fx.width, height: fx.height, bytesPerRow: fx.bytesPerRow, data };
  },
};

let /** @type {Promise<void>} */ queue = Promise.resolve();
/**
 * Wrap a handler so calls serialize through one queue (the pipeline is
 * single-instance; frames must never interleave mid-decode).
 * @template {DecodeRequest | EncodeRequest} M
 * @param {(this: Bindings, msg: M) => Promise<unknown>} handler
 * @returns {(this: Bindings, msg: M) => Promise<unknown>}
 */
const serialized = (handler) =>
  /**
   * @param {M} msg
   * @this {Bindings}
   */
  function (msg) {
    const run = queue.then(() => handler.call(this, msg));
    queue = run.then(() => {}, () => {});
    return run;
  };
// fresh object per binding: binding() installs a non-configurable
// `[options]` accessor on `local`, so one object can only bind once
const localHandlers = () => ({
  decode: serialized(handlers.decode),
  encode: serialized(handlers.encode),
});

/**
 * The bootstrap binds exactly once, on the page's first port-carrying
 * message: the MessagePort. This file must be the worker entry (or be
 * imported statically at the top of it) so the listener below exists
 * before that message can be dispatched. The default export is the
 * promise of the bound surface; an initialization failure rejects it.
 * Port-less messages from an embedder sharing the worker pass through
 * untouched.
 */
export default new Promise((resolve, reject) => {
  /** @param {MessageEvent} event */
  const capture = (event) => {
    const port = event.ports?.[0];
    if (!port) return;
    event.stopImmediatePropagation();
    removeEventListener("message", capture);
    try {
      resolve(binding(port, localHandlers()));
    } catch (err) {
      reject(err);
    }
  };
  addEventListener("message", capture);
});
