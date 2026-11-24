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
// Results are ranked by reticle priority: the code under the frame
// center point first, then a code crossing the UI's center scan line,
// then surrounding codes (see src/rank.js) — larger symbol as tiebreak.

import binding, { options } from '@webreflection/bindings/message-port';
import { createBinarizer } from '../src/binarizer-factory.js';
import { CpuBinarizer } from '../src/cpu-binarizer.js';
import { getWasmScanner, decodeFrame } from '../src/decoder.js';
import { rotateDrawable } from '../src/source-rotate.js';
import { liftDim } from '../src/source-lift.js';
import { centerRoiCandidates, cropDrawable, linearRoiCandidates, mirrorDrawable, otsuMask, padDrawable, roiCandidates } from '../src/roi.js';
import { ALL_FORMATS, LINEAR, cheapFormats, dueForFullSweep, is2DResult, markFullSweep, needsScheduling } from '../src/live-schedule.js';
import { hasCentered, isCentered, rankCodes } from '../src/rank.js';
import {
  encode_fixture_opts,
  encode_fixture_raw_opts,
} from '../src/wasm/pkg/barcode_wasm.js';

/**
 * @typedef {import('./main.js').Accuracy} Accuracy
 * @typedef {import('./main.js').Bindings} Bindings
 * @typedef {import('../src/decoder.js').DecodedCode} DecodedCode
 */

/** A decode result plus the reticle-priority score rankCodes() attaches. */
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
/** @type {CpuBinarizer | null} */
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
/** @type {CpuBinarizer | null} */
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
/** Strongly checksummed/guarded 1D families that don't false-positive on 2D texture. */
const STRONG_1D = new Set([
  "code128", "code93", "ean13", "upca", "databar", "databarltd", "databarexpanded",
]);

/** Run `fn` with a temporary binarizer threshold, always restoring 0.92. */
/**
 * @template T
 * @param {{ setThreshold?: (t: number) => void }} bin
 * @param {number} threshold
 * @param {() => Promise<T>} fn
 * @returns {Promise<T>}
 */
const withThreshold = async (bin, threshold, fn) => {
  bin.setThreshold?.(threshold);
  try {
    return await fn();
  } finally {
    bin.setThreshold?.(0.92);
  }
};

/**
 * Would at least one of these hits satisfy the reticle? A centered hit is
 * the one-code contract: it may stop a still's fallback ladder, skip the
 * 2D hunt, or end a live frame's extra opinions. A peripheral hit may
 * not — it is ranked and kept, but every other path keeps running.
 * @param {RankedCode[]} codes
 * @param {{ width: number, height: number }} frame
 */
const centeredHit = (codes, frame) =>
  hasCentered(rankCodes(codes, frame.width, frame.height));

const stillLinearFormats = (/** @type {string[] | undefined} */ formats) =>
  formats == null
    ? [...LINEAR_FORMATS]
    : formats.filter((f) => LINEAR_FORMATS.has(String(f).toLowerCase()));


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
 * @param {Awaited<ReturnType<typeof createBinarizer>> | CpuBinarizer} bin
 * @param {ImageBitmap} bitmap
 * @param {{width: number, height: number, data: Uint8Array, bytesPerRow: number}} frame
 * @param {string[] | undefined} formats
 * @param {{still?: boolean, latency?: number}} opts
 * @param {number} limit @param {number} deadline
 * @param {boolean} [centerOnly] Skip the density proposals: probe the
 *   reticle crops only. Used when a peripheral read already exists and
 *   the question is just "is the centered code also readable?" — bounded
 *   to a couple of native crops instead of a full ROI sweep.
 * @returns {Promise<import('../src/decoder.js').DecodedCode[]>}
 */
const decodeRoi = async (scanner, bin, bitmap, frame, formats, opts, limit, deadline, centerOnly = false) => {
  const sourceW = bitmap.width;
  const sourceH = bitmap.height;
  if (!sourceW || !sourceH) return [];
  const wantsLinear =
    !formats?.length || formats.some((f) => LINEAR.includes(String(f).toLowerCase()));
  const wants2D =
    !formats?.length || formats.some((f) => !LINEAR.includes(String(f).toLowerCase()));
  /** The requested 1D families only: a bar strip must never pay for a 2D
   *  tier (DataMatrix/PDF417 on the strip can spend the whole live
   *  deadline before the row scanners run). */
  const linearOnly = formats?.length
    ? formats.filter((f) => LINEAR.includes(String(f).toLowerCase()))
    : [...LINEAR];
  // Linear bands first: a wide bar strip is often not a "compact cluster"
  // (it merges with the page/box around it, or loses to the text below).
  // The reticle crops lead EVERY list: they are the boxes the UI promises,
  // so a packed label's surrounding codes cannot take their crop budget.
  const linear = wantsLinear
    ? linearRoiCandidates(frame, sourceW, sourceH, limit).map((box) => ({ ...box, linear: true }))
    : [];
  const center = centerRoiCandidates(sourceW, sourceH, { linear: wantsLinear, square: wants2D });
  /** @type {{ x: number, y: number, w: number, h: number, score: number, linear?: boolean }[]} */
  // When `centerOnly` is false, keep at most the primary center strip/square
  // ahead of the density proposals so off-center small codes still get slots
  // inside `limit + 2`.
  const proposals = centerOnly
    ? center
    : [
        ...center.slice(0, 2),
        ...linear,
        ...roiCandidates(frame, sourceW, sourceH, limit),
        ...center.slice(2),
      ];
  /** @type {{ x: number, y: number, w: number, h: number, score: number, linear?: boolean }[]} */
  const boxes = [];
  const cap = centerOnly ? center.length : limit + 3;
  for (const box of proposals) {
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
    if (boxes.length >= cap) break;
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
  /** @type {{ box: { x: number, y: number, w: number, h: number, linear?: boolean }, crop: Awaited<ReturnType<typeof cropDrawable>>, padded?: Awaited<ReturnType<typeof padDrawable>> }[]} */
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
  /**
   * A crop's hits: return them only when one is under the reticle (the
   * one-code contract is satisfied). A peripheral read is remembered as
   * the fallback and the remaining crops still run — the whole point of
   * the center-first box order. `hits` are already in full-frame mask
   * coordinates.
   * @type {DecodedCode[] | null}
   */
  let peripheral = null;
  /**
   * @param {DecodedCode[]} hits
   * @returns {DecodedCode[] | null}
   */
  const settle = (hits) => {
    if (!hits.length) return null;
    rankCodes(hits, frame.width, frame.height);
    const top = hits[0];
    if (!top) return null;
    if (isCentered(top)) return hits;
    const fallback = peripheral?.[0];
    if (!fallback || (top.score ?? 0) > (fallback.score ?? 0)) peripheral = hits;
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
    /** @type {{ box: { x: number, y: number, w: number, h: number, linear?: boolean }, crop: Awaited<ReturnType<typeof cropDrawable>>, padded?: Awaited<ReturnType<typeof padDrawable>> }} */
    const entry = { box, crop };
    crops.push(entry);
    // A bar strip is decoded by the row scanners only.
    const boxFormats = box.linear ? linearOnly : formats;
    /** @type {import('../src/gpu/binarizer.js').BinarizedFrame} */
    const f = await bin.process(crop.canvas);
    let found = offer(decodeFrame(scanner, f, boxFormats, callOpts()), f, crop);
    if (found) {
      const settled = settle(found);
      if (settled) return settled;
    }
    // WASM's still path already tries both polarities (`suspicious = still`);
    // only live frames need the explicit JS inversion for light-page islands.
    if (opts.still !== true) {
      const inverted = { ...f, data: f.data.map((/** @type {number} */ b) => ~b) };
      found = offer(decodeFrame(scanner, inverted, boxFormats, callOpts()), f, crop);
      if (found) {
        const settled = settle(found);
        if (settled) return settled;
      }
    }
    if (!box.linear) continue;

    // 0.88 unpadded first: reuses `crop.canvas`'s cached luma + blur for free.
    found = await withThreshold(bin, 0.88, async () => {
      const f88 = await bin.process(crop.canvas);
      let hit = offer(decodeFrame(scanner, f88, boxFormats, callOpts()), f88, crop);
      if (hit) return hit;

      // White-pad restores the 10-module Code 128 quiet zone on tight labels.
      const padPx = Math.max(16, Math.min(96, Math.round(box.h)));
      const padded = await padDrawable(crop.canvas, padPx);
      entry.padded = padded;
      const fp = await bin.process(padded.canvas);
      const padMap = {
        ...crop,
        /** @param {number} fw @param {number} fh @param {{x: number, y: number}[]} points */
        map: (fw, fh, points) => crop.map(crop.width, crop.height, padded.map(fw, fh, points)),
      };
      hit = offer(decodeFrame(scanner, fp, boxFormats, callOpts()), fp, padMap);
      if (hit) return hit;

      // Source mirror is the upside-down phone lever (unconditional so slow
      // full frames do not drop it while cheap frames read it).
      const mirrored = await mirrorDrawable(crop.canvas);
      const fm = await bin.process(mirrored.canvas);
      const mirrorMap = {
        ...crop,
        /** @param {number} fw @param {number} fh @param {{x: number, y: number}[]} points */
        map: (fw, fh, points) =>
          mirrored.map(fw, fh, points).map((p) => ({ x: crop.x + p.x, y: crop.y + p.y })),
      };
      return offer(decodeFrame(scanner, fm, boxFormats, callOpts()), fm, mirrorMap);
    });
    if (found) {
      const settled = settle(found);
      if (settled) return settled;
    }
  }

  // Pass 2: the local threshold floods thin spaces on a blurry bar strip;
  // a global Otsu split over a 2x upscale keeps them white. Only linear crops,
  // only after every cheap opinion missed. Try the unpadded crop first, then
  // the paper-white padded copy (restores tight quiet zones AND shifts Otsu
  // away from dark box edges).
  for (const { box, crop, padded } of crops) {
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
      const masks = opts.still === true
        ? [otsu]
        : [otsu, { ...otsu, data: otsu.data.map((/** @type {number} */ b) => ~b) }];
      for (const data of masks) {
        const found = offer(decodeFrame(scanner, data, linearOnly, callOpts()), otsu, scaled);
        if (found) {
          const settled = settle(found);
          if (settled) return settled;
        }
      }
    }
    if (padded && remaining() > 0) {
      const otsu = otsuMask(padded.canvas, 2);
      const padScaled = {
        ...crop,
        /** @param {number} fw @param {number} fh @param {{x: number, y: number}[]} points */
        map: (fw, fh, points) =>
          crop.map(
            crop.width,
            crop.height,
            padded.map(
              padded.width,
              padded.height,
              points.map((p) => ({
                x: (p.x * padded.width) / fw,
                y: (p.y * padded.height) / fh,
              })),
            ),
          ),
      };
      const found = offer(decodeFrame(scanner, otsu, linearOnly, callOpts()), otsu, padScaled);
      if (found) {
        const settled = settle(found);
        if (settled) return settled;
      }
    }
  }
  if (peripheral) return peripheral;
  return weakHit ? done(weakHit.hits, weakHit.mask, weakHit.mapper) : [];
};

/**
 * Append `hits` not already present in `dst` (by `symbology\0text`, or
 * spatially overlapping an existing hit of the same symbology). The
 * spatial guard prevents a harsher fallback pass (0.88 / Otsu / 0.7) from
 * re-decoding the SAME physical barcode into a noisier variant and
 * outranking the clean primary read (measured on a thermal receipt).
 * Returns whether at least one new hit was added.
 * @param {DecodedCode[]} dst
 * @param {DecodedCode[]} hits
 */
const mergeUnique = (dst, hits) => {
  if (!hits.length) return false;
  /** @param {DecodedCode['position']} p */
  const boxOf = (p) => {
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const { x, y } of p ?? []) {
      if (x < x0) x0 = x;
      if (x > x1) x1 = x;
      if (y < y0) y0 = y;
      if (y > y1) y1 = y;
    }
    return { x0, y0, x1, y1 };
  };
  const seen = new Set(dst.map((c) => c.symbology + '\u0000' + c.text));
  let added = false;
  for (const hit of hits) {
    const key = hit.symbology + '\u0000' + hit.text;
    if (seen.has(key)) continue;
    const hb = boxOf(hit.position);
    const sameSpot = dst.some((c) => {
      if (c.symbology !== hit.symbology) return false;
      const cb = boxOf(c.position);
      // Two 1D slivers from the same barcode can land on different rows of
      // the same bar strip (a 800px-wide receipt barcode at 2112px is ~140px
      // tall), so pad vertically proportionally to the strip's width (~12%).
      const xOverlap = Math.min(hb.x1, cb.x1) - Math.max(hb.x0, cb.x0);
      const minW = Math.min(hb.x1 - hb.x0, cb.x1 - cb.x0);
      const padY = Math.max(24, minW * 0.12);
      return minW > 0 && xOverlap > minW * 0.5 && hb.y0 <= cb.y1 + padY && hb.y1 >= cb.y0 - padY;
    });
    if (sameSpot) continue;
    seen.add(key);
    dst.push(hit);
    added = true;
  }
  return added;
};

/**
 * Build the ranked decode response envelope.
 * @param {DecodedCode[]} codes
 * @param {{ width: number, height: number }} frame
 * @param {number} t0
 * @param {boolean} partial
 * @param {string[] | null} checked
 * @param {ReturnType<typeof getWasmScanner> extends Promise<infer S> ? S : never} scanner
 * @param {string[] | undefined} formats
 * @returns {DecodeResponse}
 */
const makeResult = (codes, frame, t0, partial, checked, scanner, formats) => ({
  type: 'result',
  codes: rankCodes(codes, frame.width, frame.height),
  decodeMs: Math.round(performance.now() - t0),
  frameW: frame.width,
  frameH: frame.height,
  partial,
  checked,
  qrPresence: !formats?.length || formats.some((f) => f === 'qr' || f === 'qrcode')
    ? scanner.qr_presence()
    : undefined,
});

/**
 * CPU second opinion at threshold 0.7 for busy-background 2D misses where
 * the GPU's fixed floor over-inked the scene.
 * @param {ReturnType<typeof getWasmScanner> extends Promise<infer S> ? S : never} scanner
 * @param {CpuBinarizer} cpuBin
 * @param {ImageBitmap | OffscreenCanvas} input
 * @param {{ width: number, height: number }} frame
 * @param {string[]} families
 * @param {{ still?: boolean, latency?: number }} callOpts
 * @returns {Promise<DecodedCode[]>}
 */
const runCpuSecondOpinion = async (scanner, cpuBin, input, frame, families, callOpts) => {
  cpuBin.setThreshold(0.7);
  const cpuFrame = await cpuBin.process(input);
  const hits = decodeFrame(scanner, cpuFrame, families, callOpts);
  if (hits.length && (cpuFrame.width !== frame.width || cpuFrame.height !== frame.height)) {
    const sx = frame.width / cpuFrame.width;
    const sy = frame.height / cpuFrame.height;
    for (const hit of hits) {
      hit.position = hit.position.map((p) => ({ x: p.x * sx, y: p.y * sy }));
    }
  }
  return hits;
};

/**
 * Diagonal (±45°) source-rotation retry: rotating the antialiased source
 * preserves Aztec bullseyes / DM edges that mask-level 45° rotation erodes.
 * @param {ReturnType<typeof getWasmScanner> extends Promise<infer S> ? S : never} scanner
 * @param {Awaited<ReturnType<typeof createBinarizer>>} bin
 * @param {ImageBitmap | OffscreenCanvas} input
 * @param {string[]} diag
 * @param {() => { still?: boolean, latency?: number }} getOpts
 * @param {() => boolean} [outOfTime]
 * @returns {Promise<DecodedCode[]>}
 */
const runDiagonalRetry = async (scanner, bin, input, diag, getOpts, outOfTime) => {
  bin.setThreshold(0.92);
  for (const deg of [45, -45]) {
    if (outOfTime?.()) break;
    const rotated = await rotateDrawable(input, deg);
    const df = await bin.process(rotated.canvas);
    const hits = decodeFrame(scanner, df, diag, getOpts());
    if (hits.length) {
      for (const hit of hits) {
        if (hit.position?.length) {
          hit.position = rotated.map(df.width, df.height, hit.position);
        }
      }
      return hits;
    }
  }
  return [];
};

/**
 * Still-image decode path: area-filtered 2112px input, shared deadline,
 * fast 1D threshold ladder, centered-1D early exit, CPU first/second
 * opinions, native-resolution ROI, dim lift + slight-roll, and diagonal.
 * Never touches live scheduler/cooldown state.
 * @param {ImageBitmap} bitmap
 * @param {string[] | undefined} formats
 * @param {Accuracy | undefined} accuracy
 * @param {number | undefined} latency
 * @returns {Promise<DecodeResponse>}
 */
const decodeStill = async (bitmap, formats, accuracy, latency) => {
  const bin = await stillBinarizer();
  try {
    const scanner = await scannerReady;
    const t0 = performance.now();
    // Gallery photos are 12MP and the GPU luma stage reaches 2112 with a
    // single bilinear tap per destination pixel: that aliases a small
    // symbol's module pitch away. Area-filter HERE with the browser's
    // resampler, keeping the ORIGINAL bitmap for native-resolution ROI.
    /** @type {ImageBitmap | OffscreenCanvas} */
    let input = bitmap;
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
    const frame = await bin.process(input);
    const opts = { still: true, latency };
    const deadline = t0 + (latency ?? 5000);
    const remaining = () => deadline - performance.now();
    const outOfTime = () => performance.now() >= deadline;
    const budgetOpts = () => ({ ...opts, latency: Math.max(250, remaining()) });

    // 1. Fast 1D threshold ladder (~ms vs seconds for full exotic 2D).
    /** @type {DecodedCode[]} */
    let codes = [];
    let stillLinearHit = false;
    if (typeof bin.setThreshold === "function") {
      const linear = stillLinearFormats(formats);
      if (linear.length) {
        const thresholds = accuracy === "low" ? [0.92] : [0.92, 0.82, 0.7];
        for (const threshold of thresholds) {
          if (outOfTime()) break;
          const f = threshold === 0.92 ? frame : await withThreshold(bin, threshold, () => bin.process(input));
          const lin = decodeFrame(scanner, f, linear, opts);
          if (lin.length) {
            // A peripheral read at 0.92 must not hide a centered read at
            // 0.82/0.7 (two stacked barcodes on one label): merge every
            // threshold and stop only when the reticle is satisfied.
            mergeUnique(codes, lin);
            stillLinearHit = true;
            if (centeredHit(codes, frame)) break;
          }
        }
      }
    }

    // A CENTERED, reliable 1D barcode already satisfies the one-code
    // contract — skip the 5-8s DataMatrix/2D hunt. "Centered" is the
    // reticle tier from src/rank.js (center point inside the quad, or a
    // strong center-scan-line hit): a barcode in the corner never stops
    // the 2D hunt, which is what let a corner QR outrank a centered
    // EAN-13 on a packed label. Weak/loose 1D hits (ean8/upce/codabar/
    // code39) never short-circuit 2D so a noisy ean8 inside an Aztec/QR
    // grid (bahn-509724794512) cannot hide it.
    const strongLinear = codes.some((c) => STRONG_1D.has(c.symbology) || c.symbology === "itf");
    const centeredLinear =
      stillLinearHit && strongLinear && centeredHit(codes, frame);

    // 2. GPU stills: CPU first opinion for STILL_CPU_FAMILIES unless a
    // centered reliable 1D hit already satisfied the one-code contract.
    /** @type {DecodedCode[]} */
    let cpuCodes = [];
    if (bin.kind === "gpu" && !centeredLinear) {
      const families = stillCpuFamilies(formats);
      if (families.length && !outOfTime()) {
        const cpuBin = stillCpuBinarizer();
        const cpuLatency = () => Math.min(budgetOpts().latency ?? 2000, stillLinearHit ? 1000 : 2000);
        const isRoi = (/** @type {string} */ f) => STILL_CPU_ROI_FAMILIES.has(String(f).toLowerCase());
        const roiFamilies = families.filter(isRoi);
        const cheapFamilies = families.filter((f) => !isRoi(f));
        /** @type {import('../src/gpu/binarizer.js').BinarizedFrame | null} */
        let frame07 = null;
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
          if (cpuMask && mergeUnique(cpuCodes, decodeFrame(scanner, cpuMask, cheapFamilies, { ...opts, still: false }))) break;
        }
        if (!cpuCodes.length && roiFamilies.length && !outOfTime()) {
          if (!frame07) {
            cpuBin.setThreshold(0.7);
            frame07 = await cpuBin.process(input);
          }
          const mask07 = frame07;
          if (mask07) mergeUnique(cpuCodes, decodeFrame(scanner, mask07, roiFamilies, { ...opts, latency: cpuLatency() }));
        }
        const cpuHitNow = new Set(cpuCodes.map((c) => c.symbology));
        const roiLeft = stillLinearHit
          ? []
          : roiFamilies.filter((f) => !cpuHitNow.has(String(f).toLowerCase()));
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
    // A short DataMatrix-only CPU first opinion is *provisional*: even with
    // the clock-track gate, a tiny 10x10/12x12 DM fragment has the lowest
    // Reed-Solomon margin of any 2D family and can fire on dense Aztec/QR
    // textures (real GS1/invoice DMs are much longer and skip this check
    // completely, paying 0ms). Let the stricter 2D families (including QR
    // and MicroQR, which may decode on the GPU 0.92 mask even after missing
    // on CPU 0.7/0.82) read the GPU mask with `still: false` (no dual-polarity
    // tilt sweep) before accepting the tiny DM hit.
    const PROVISIONAL_DM_MAX_BYTES = 10;
    if (
      bin.kind === "gpu" &&
      cpuCodes.length &&
      cpuCodes.every(
        (c) => c.symbology === "datamatrix" && (c.raw?.length ?? c.text.length) <= PROVISIONAL_DM_MAX_BYTES,
      )
    ) {
      const strict = cpuSecondOpinionFormats(formats, cpuCodes, new Set(["datamatrix"]));
      if (strict.length && !outOfTime()) {
        const hits = decodeFrame(scanner, frame, strict, { ...opts, still: false });
        if (hits.length) {
          codes = codes.filter((c) => c.symbology !== "datamatrix").concat(hits);
          cpuCodes = hits;
        }
      }
    }
    const cpuHit = new Set(cpuCodes.map((c) => c.symbology));

    // 3. Off-center or weak 1D hit: check remaining 2D via ROI + 0.7 so a
    // centered 2D code (e.g. gs1-two-codes DataMatrix) still wins.
    if (stillLinearHit && !centeredLinear) {
      const twoD = (formats ?? ALL_FORMATS).filter(
        (f) =>
          !LINEAR_FORMATS.has(String(f).toLowerCase()) &&
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
          Math.min(performance.now() + (bin.kind === "gpu" ? 800 : 1500), deadline),
        );
        if (roi.length) codes = strongLinear ? codes.concat(roi) : roi;
        else if (!outOfTime() && typeof bin.setThreshold === "function") {
          const alt = await withThreshold(bin, 0.7, () => bin.process(input));
          const twoDHits = decodeFrame(scanner, alt, twoD, budgetOpts());
          if (twoDHits.length) {
            if (strongLinear) mergeUnique(codes, twoDHits);
            else codes = twoDHits;
          }
        }
      }
    }

    // 4. Small 1D native-resolution ROI before the exotic 2D sweep. A
    // peripheral read does not satisfy the reticle, so it still runs —
    // and with the full proposal list, because the tight dense-row band
    // is exactly what isolates a bar strip printed next to a dark label
    // border (its printed quiet zone is ~6 modules, where the standard
    // mandates 10): the crop then gets re-padded on paper white.
    if (!centeredHit(codes, frame)) {
      const linear = stillLinearFormats(formats);
      if (linear.length) {
        const peripheralOnly = codes.length > 0;
        const roi = await decodeRoi(
          scanner,
          bin,
          bitmap,
          frame,
          linear,
          opts,
          2,
          Math.min(performance.now() + (peripheralOnly ? 1200 : 900), deadline),
          false,
        );
        if (roi.length) {
          mergeUnique(codes, roi);
          stillLinearHit = true;
        }
      }
    }

    // 5. Full sweep + threshold ladder when neither 1D nor CPU first opinion hit.
    if (!stillLinearHit && !cpuHit.size && formats?.length !== 0) {
      codes = codes.concat(decodeFrame(scanner, frame, formats, budgetOpts()));
      const sweep = accuracy === 'high' || (accuracy === 'auto' && codes.length === 0);
      if (sweep && !outOfTime() && typeof bin.setThreshold === "function") {
        for (const threshold of [0.82, 0.7]) {
          if (outOfTime()) break;
          const alt = await withThreshold(bin, threshold, () => bin.process(input));
          mergeUnique(codes, decodeFrame(scanner, alt, formats, budgetOpts()));
        }
      }
    }

    // 6. CPU second opinion for non-STILL_CPU_FAMILIES (Aztec / PDF417).
    if (bin.kind === "gpu" && codes.length === 0 && remaining() > 250) {
      const families = cpuSecondOpinionFormats(formats, codes, STILL_CPU_FAMILIES);
      if (families.length) {
        codes = codes.concat(
          await runCpuSecondOpinion(scanner, stillCpuBinarizer(), input, frame, families, budgetOpts()),
        );
      }
    }

    // 7. Full native-resolution ROI pass. With a peripheral read already
    // in hand the question is narrower — is the centered code readable
    // too? — so the pass collapses to the reticle crops and keeps the
    // peripheral fallback either way.
    if (!centeredHit(codes, frame) && !outOfTime()) {
      const peripheralOnly = codes.length > 0;
      const roi = await decodeRoi(
        scanner,
        bin,
        bitmap,
        frame,
        formats,
        budgetOpts(),
        peripheralOnly ? 2 : 4,
        deadline,
        peripheralOnly,
      );
      if (roi.length) mergeUnique(codes, roi);
    }

    // 8. Dim still brightness lift + small-roll source rotation retry.
    if (codes.length === 0 && remaining() > 300) {
      const liftFormats = (formats ?? [...LINEAR_FORMATS, 'databarstk']).filter((f) => {
        const n = String(f).toLowerCase();
        return LINEAR_FORMATS.has(n) || n === 'databarstk';
      });
      const linear = stillLinearFormats(formats);
      if (liftFormats.length) {
        const thresholds = [0.92, 0.82, 0.7];
        const lifted = await liftDim(input);
        if (lifted) {
          for (const threshold of thresholds) {
            if (remaining() < 250) break;
            const df = await withThreshold(bin, threshold, () => bin.process(lifted.canvas));
            const hits = decodeFrame(scanner, df, liftFormats, budgetOpts());
            if (hits.length) {
              codes = hits;
              break;
            }
          }
        }
        const source = lifted?.canvas ?? input;
        for (const deg of [-3, 3, -4, 4, -8, 8]) {
          if (codes.length || remaining() < 250 || !linear.length) break;
          const rotated = await rotateDrawable(source, deg);
          for (const threshold of thresholds) {
            if (remaining() < 250) break;
            const df = await withThreshold(bin, threshold, () => bin.process(rotated.canvas));
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
        }
      }
    }

    // 9. Diagonal ±45° source-rotation retry.
    if (codes.length === 0 && !outOfTime()) {
      const diag = formats == null
        ? [...DIAGONAL_FORMATS]
        : formats.filter((n) => DIAGONAL_FORMATS.has(String(n).toLowerCase()));
      if (diag.length) {
        codes = await runDiagonalRetry(scanner, bin, input, diag, budgetOpts, outOfTime);
      }
    }

    return makeResult(codes, frame, t0, false, null, scanner, formats);
  } finally {
    bin.clearCache?.();
    stillCpuBinarizerInstance?.clearCache?.();
  }
};

/**
 * Live-camera frame decode path: 1056px binarizer, cheap/full scheduling,
 * centered strong-1D early exit, throttled CPU second opinion / native ROI /
 * diagonal retries, and confirmation-law `checked` tracking.
 * @param {ImageBitmap} bitmap
 * @param {string[] | undefined} formats
 * @param {Accuracy | undefined} accuracy
 * @param {number | undefined} latency
 * @returns {Promise<DecodeResponse>}
 */
const decodeLive = async (bitmap, formats, accuracy, latency) => {
  let full = true;
  let activeFormats = formats;
  /** Families actually decoded this frame: null = everything requested. @type {string[] | null} */
  let checked = null;
  const scheduled = needsScheduling(formats);
  if (scheduled) {
    full = dueForFullSweep(performance.now());
    if (!full) {
      activeFormats = cheapFormats(formats, !lastFullHad2D);
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
  const bin = await binarizerReady;
  try {
    const scanner = await scannerReady;
    const t0 = performance.now();
    const frame = await bin.process(bitmap);
    const opts = { still: false, latency };
    /** @type {DecodedCode[]} */
    let codes = [];

    if (activeFormats?.length !== 0) {
      if (full && scheduled) {
        const cheapList = cheapFormats(activeFormats, !lastFullHad2D);
        if (cheapList.length) {
          codes = decodeFrame(scanner, frame, cheapList, opts);
          checked = cheapList;
        }
        // 2D-before-1D: loose/unchecksummed 1D hits can be noise from a 2D
        // symbol's texture, so still run exotic tiers then. A centered,
        // strongly-checksummed 1D hit confirms directly without paying ~400ms
        // of DataMatrix + PDF417.
        const linearOnly =
          codes.length > 0 && codes.every((c) => !is2DResult(c.symbology));
        const strongCentered1D =
          linearOnly &&
          codes.some((c) => STRONG_1D.has(c.symbology)) &&
          centeredHit(codes, frame);
        if (!codes.length || (linearOnly && !strongCentered1D)) {
          const ranCheap = new Set(cheapList.map((f) => String(f).toLowerCase()));
          const rest = (activeFormats ?? ALL_FORMATS).filter(
            (f) => !ranCheap.has(String(f).toLowerCase()),
          );
          if (rest.length) codes = codes.concat(decodeFrame(scanner, frame, rest, opts));
          checked = null;
        }
      } else {
        codes = decodeFrame(scanner, frame, activeFormats, opts);
      }
      const firstMs = performance.now() - t0;
      // A peripheral read does not end the frame's opinions: the second
      // threshold may be the one that reads the centered code.
      const sweep =
        accuracy === 'high' ||
        (accuracy === 'auto' && !centeredHit(codes, frame) && firstMs < 220);
      if (sweep && typeof bin.setThreshold === "function") {
        const alt = await withThreshold(bin, 0.82, () => bin.process(bitmap));
        mergeUnique(codes, decodeFrame(scanner, alt, activeFormats, opts));
        checked = null;
      }
    }

    // Live CPU second opinion: after a true miss, or a peripheral-only
    // read (the centered code may need the CPU mask's relative floor).
    if (bin.kind === "gpu" && !centeredHit(codes, frame)) {
      const families = cpuSecondOpinionFormats(activeFormats, codes);
      const now = performance.now();
      const due =
        now <= cpuSecondOpinionHotUntil ||
        now - lastCpuSecondOpinionAt >= CPU_SECOND_OPINION_MIN_INTERVAL_MS;
      if (families.length && due) {
        lastCpuSecondOpinionAt = now;
        const hits = await runCpuSecondOpinion(scanner, liveCpuBinarizer(), bitmap, frame, families, opts);
        if (hits.length) {
          mergeUnique(codes, hits);
          cpuSecondOpinionHotUntil = now + 600;
        }
      }
    }

    // Live native-resolution ROI pass (2 crops, 120ms budget, hot bypass for confirmation).
    // With a peripheral read in hand, probe the reticle crops only.
    if (!centeredHit(codes, frame)) {
      const now = performance.now();
      if (now <= roiHotUntil || now - lastRoiAt >= ROI_MIN_INTERVAL_MS) {
        lastRoiAt = now;
        const peripheralOnly = codes.length > 0;
        const roi = await decodeRoi(
          scanner,
          bin,
          bitmap,
          frame,
          activeFormats,
          opts,
          peripheralOnly ? 1 : 2,
          now + 120,
          peripheralOnly,
        );
        if (roi.length) {
          mergeUnique(codes, roi);
          roiHotUntil = now + 600;
        }
      }
    }

    // Live diagonal ±45° retry: only explicit diagonal tiers, at most 2 Hz.
    if (codes.length === 0 && formats != null) {
      const diag = activeFormats == null
        ? [...DIAGONAL_FORMATS]
        : activeFormats.filter((n) => DIAGONAL_FORMATS.has(String(n).toLowerCase()));
      const now = performance.now();
      if (diag.length > 0 && now - lastDiagonalAt >= 500) {
        lastDiagonalAt = now;
        codes = await runDiagonalRetry(scanner, bin, bitmap, diag, () => opts);
      }
    }

    // Only a CENTERED 2D read may mute the linear tier on the next cheap
    // frames: a corner QR used to disable 1D for the rest of the sweep
    // interval, which is how it outranked the barcode under the line.
    if (full) {
      rankCodes(codes, frame.width, frame.height);
      lastFullHad2D = codes.some((c) => is2DResult(c.symbology) && isCentered(c));
    }
    lastCandidates = codes.map((c) => c.symbology);
    return makeResult(codes, frame, t0, !full, checked, scanner, activeFormats);
  } finally {
    if (full) markFullSweep(performance.now());
    bin.clearCache?.();
    liveCpuBinarizerInstance?.clearCache?.();
  }
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
    try {
      const { bitmap, formats, still, accuracy, latency } = msg ?? {};
      if (!bitmap) throw new Error('decode: missing bitmap');
      return still === true
        ? await decodeStill(bitmap, formats, accuracy, latency)
        : await decodeLive(bitmap, formats, accuracy, latency);
    } catch (err) {
      console.error('decode failed:', err);
      return { type: 'error', message: String(err instanceof Error ? err.message : err) };
    } finally {
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
