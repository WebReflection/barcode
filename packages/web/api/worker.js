// The SDK worker: bound via @webreflection/bindings, exposing `decode`
// (one frame in — candidates out, stateless) and `encode` (symbology +
// text — raw module bitmap out). Strictly serial: the main thread sends
// one frame only after the previous answer, so no request ids exist.
//
// The bootstrap: the first port-carrying message is the handshake and
// the listener is gone right after (bind exactly once); messages
// without ports pass through untouched — embedders keep their protocol.
//
// Latency laws travel per-request: live frames default to the 500ms
// camera law, still images to 5000ms — both configurable per decode.
// Results are ranked: most centered first, larger symbol as tiebreak.

import binding, { options } from '@webreflection/bindings/message-port';
// Captures the main thread's port (and answers its readiness ping),
// independent of how long the wasm/GPU module graph takes to evaluate.
import { handshake } from './handshake.js';
import { createBinarizer } from '../src/binarizer-factory.js';
import { getWasmScanner, decodeFrame } from '../src/decoder.js';
import { rotateDrawable } from '../src/source-rotate.js';
import { cropDrawable, linearRoiCandidates, otsuMask, roiCandidates } from '../src/roi.js';
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
      return ix * iy > 0.6 * Math.min(box.w * box.h, p.w * p.h);
    });
    if (!overlap) boxes.push(box);
    // The caller's crop budget is the bound: extra candidates multiply
    // the wasm sweep, and a junk frame produces no valid crop anyway.
    if (boxes.length >= limit) break;
  }
  if (!boxes.length) return [];
  bin.setThreshold?.(0.92);
  for (const box of boxes) {
    const left = deadline - performance.now();
    if (left <= 0) break;
    const crop = await cropDrawable(bitmap, box);
    const roiOpts = opts.still === true
      ? { still: true, latency: Math.max(250, Math.min(opts.latency ?? 5000, left)) }
      : opts;
    /** @param {import('../src/gpu/binarizer.js').BinarizedFrame} mask */
    const decodeCrop = (mask) => decodeFrame(scanner, mask, formats, roiOpts);
    /**
     * @param {DecodedCode[]} hits
     * @param {import('../src/gpu/binarizer.js').BinarizedFrame} mask
     * @param {{ x: number, y: number, width: number, height: number, map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[] }} [mapper]
     * @returns {DecodedCode[]}
     */
    const mapHits = (hits, mask, mapper = crop) =>
      mapRoiHits(hits, mapper, mask, frame, sourceW, sourceH);

    /** @type {{ width: number, height: number, data: Uint8Array, bytesPerRow: number }} */
    let f = await bin.process(crop.canvas);
    let hits = decodeCrop(f);
    if (hits.length) return mapHits(hits, f);
    // White-on-black on a light page: the crop keeps a light margin, so the
    // full-frame ink heuristic cannot pick the inverted polarity. Flip the
    // (small) crop and let the decoders' own format/RS validation arbitrate.
    let left2 = deadline - performance.now();
    if (left2 <= 0) break;
    let invOpts = roiOpts.still === true
      ? { ...roiOpts, latency: Math.max(250, Math.min(roiOpts.latency ?? 5000, left2)) }
      : roiOpts;
    let flipped = f.data.map((/** @type {number} */ b) => ~b);
    let invHits = decodeFrame(scanner, { ...f, data: flipped }, formats, invOpts);
    if (invHits.length) return mapHits(invHits, f);

    // The local threshold floods thin spaces when the crop is a blurry bar
    // strip. A global Otsu split over a 2x upscale keeps them white; this
    // is the distant/out-of-focus 1D lever (a real Code 32 package
    // measured 0 reads locally, reads at 2x Otsu).
    if (!box.linear) continue;
    left2 = deadline - performance.now();
    if (left2 <= 0) break;
    invOpts = roiOpts.still === true
      ? { ...roiOpts, latency: Math.max(250, Math.min(roiOpts.latency ?? 5000, left2)) }
      : roiOpts;
    /** @param {number} fw @param {number} fh @param {{x: number, y: number}[]} points */
    const scaledMap = (fw, fh, points) =>
      points.map((p) => ({
        x: crop.x + (p.x * crop.width) / fw,
        y: crop.y + (p.y * crop.height) / fh,
      }));
    const scaled = { ...crop, map: scaledMap };
    for (const scale of [2, 1]) {
      if (deadline - performance.now() <= 0) break;
      const otsu = otsuMask(crop.canvas, scale);
      hits = decodeFrame(scanner, otsu, formats, invOpts);
      if (hits.length) return mapHits(hits, otsu, scaled);
      const otsuFlipped = otsu.data.map((/** @type {number} */ b) => ~b);
      hits = decodeFrame(scanner, { ...otsu, data: otsuFlipped }, formats, invOpts);
      if (hits.length) return mapHits(hits, otsu, scaled);
    }
  }
  return [];
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
            let f = frame;
            if (threshold !== 0.92) {
              bin.setThreshold(threshold);
              f = await bin.process(bitmap);
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
      // Small 1D codes (the roadmap's receipt lever): the full-frame
      // 2112px ladder can still leave a bar code at 2-3px/module. The
      // compact clusters the mask already shows are cropped from the
      // ORIGINAL photo and read at native pitch before the exotic sweep.
      if (still === true && !stillLinearHit) {
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
      if (!stillLinearHit) {
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
          const cheapList = cheapFormats(activeFormats, !lastFullHad2D);
          // `decodeFrame` treats an empty formats list as "everything":
          // only run the cheap pass when it actually selected families.
          if (cheapList.length) {
            codes = decodeFrame(scanner, frame, cheapList, opts);
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
            const rest = (activeFormats ?? ALL_FORMATS).filter(
              (f) => !ranCheap.has(String(f).toLowerCase()),
            );
            // Families are disjoint, so this only ever appends: a linear
            // hit must not be lost when the exotic tiers miss.
            if (rest.length) codes = codes.concat(decodeFrame(scanner, frame, rest, opts));
            // cheap + rest cover every requested family.
            checked = null;
          }
        } else {
          codes = decodeFrame(scanner, frame, activeFormats, opts);
        }
        const firstMs = performance.now() - t0;
        // 'auto' = lazy second threshold when the first pass finds nothing.
        // On live camera frames that is the common case, and a second
        // binarize+decode doubles the frame cost — the exact frames where
        // the user is still aiming. Skip the sweep when the first pass
        // already spent (or nearly spent) the live budget: a stale second
        // opinion cannot help that frame. Still scans keep the sweep while
        // their 5s law leaves room.
        const sweep =
          accuracy === 'high' ||
          (accuracy === 'auto' &&
            codes.length === 0 &&
            firstMs < (still ? 2500 : 220));
        // second Bradley threshold over the SAME frame (glare AND
        // shadow variants), deduped against the first pass
        if (sweep && typeof bin.setThreshold === 'function') {
          bin.setThreshold(0.82);
          const alt = await bin.process(bitmap);
          bin.setThreshold(0.92);
          const seen = new Set(codes.map((c) => c.symbology + '\u0000' + c.text));
          codes = codes.concat(
            decodeFrame(scanner, alt, activeFormats, opts).filter(
              (c) => !seen.has(c.symbology + '\u0000' + c.text),
            ),
          );
          // The alternate-threshold pass decodes every requested family.
          checked = null;
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
        if (diag.length > 0 && due) {
          lastDiagonalAt = now;
          bin.setThreshold(0.92);
          for (const deg of [45, -45]) {
            const rotated = await rotateDrawable(bitmap, deg);
            const df = await bin.process(rotated.canvas);
            const hits = decodeFrame(scanner, df, diag, opts);
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
          const deadline =
            still === true ? t0 + (latency ?? 5000) : now + 120;
          const roi = await decodeRoi(
            scanner,
            bin,
            bitmap,
            frame,
            activeFormats,
            opts,
            still === true ? 4 : 2,
            deadline,
          );
          if (roi.length) {
            codes = roi;
            roiHotUntil = now + 600;
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
 * The bootstrap binds exactly once: the first port-carrying message is
 * the handshake (captured by ./handshake.js as early as possible), then
 * the listener is gone. Embedders' port-less messages pass through.
 */
export default handshake().then((port) => binding(port, localHandlers()));
