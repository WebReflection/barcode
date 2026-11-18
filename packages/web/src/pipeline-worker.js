/**
 * Pipeline worker: owns the GPU binarizer and the WASM scanner. Frames
 * arrive as transferred ImageBitmaps (zero-copy transfer — no shared
 * buffers, no special headers) and results go back as plain messages.
 *
 * Why a worker: decoding (especially the 2D tier) must never block the
 * camera preview or the UI; the main thread only captures frames. The
 * GPU is available inside dedicated workers (WebGPU in DedicatedWorker-
 * GlobalScope); when it is not, the binarizer factory falls back to the
 * CPU path — still inside the worker, still off the main thread.
 */

import { createBinarizer } from "./binarizer-factory.js";
import { decodeFrame, getSupportedSymbologies, getWasmScanner } from "./decoder.js";
import { rotateDrawable } from "./source-rotate.js";
import { cropDrawable, roiCandidates } from "./roi.js";

/** The binarizer factory's common surface, GPU or CPU. */
/** @typedef {{ process(video: HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas): Promise<import("./gpu/binarizer.js").BinarizedFrame>, destroy(): void, kind: "gpu" | "cpu", setThreshold(t: number): void }} Binarizer */

/** Frame options: the binarizer contract plus the decode knobs. */
/** @typedef {import("./gpu/binarizer.js").BinarizerOptions & { accuracy?: Accuracy, formats?: string[] }} FrameOpts */

/** @typedef {{ type: "frameResult", results: DecodedCode[], error?: string, qrPresence?: number, frameW?: number, frameH?: number, bytesPerRow?: number, mask?: ArrayBufferLike, decodeMs?: number, kind?: string }} FrameResultMsg */

/** @type {Promise<{bin: Binarizer, scanner: ReturnType<typeof getWasmScanner> extends Promise<infer S> ? S : never}> | null} */
let pipeline = null;
let /** @type {string} */ kind = "init";

// accuracy-high state: a second Bradley threshold is swept on alternating
// frames (glare needs MORE ink, shadow LESS; the primary 0.92 always runs).
const ALT_THRESHOLDS = [0.8, 1.04];
let /** @type {number} */ altIdx = 0;
let /** @type {number} */ primaryThreshold = 0.92;

/** Tiers worth a source-rotation retry (diagonal presentation). */
const DIAGONAL_FORMATS = new Set(["datamatrix", "aztec", "pdf417"]);
/** Live: at most one source-rotation retry per half second. */
let lastDiagonalAt = 0;
/** Live: at most one native-ROI pass per this many ms (crop + binarize). */
const ROI_MIN_INTERVAL_MS = 250;
let lastRoiAt = -Infinity;
/** Keep re-running the native pass for consecutive-frame confirmation. */
let roiHotUntil = 0;

/**
 * Merge two decode batches, dropping duplicates (same symbology + text may
 * be found by both threshold passes).
 * @param {DecodedCode[]} a
 * @param {DecodedCode[]} b
 * @returns {DecodedCode[]}
 */
function mergeResults(a, b) {
  if (!b.length) return a;
  const seen = new Set(a.map((r) => r.symbology + "\u0000" + r.text));
  return [...a, ...b.filter((r) => !seen.has(r.symbology + "\u0000" + r.text))];
}

/** @typedef {import("./decoder.js").DecodedCode} DecodedCode */
/** @typedef {import("./scanner.js").Accuracy} Accuracy */

/**
 * Map ROI-crop results into the FULL-FRAME mask coordinate space (the
 * space every other worker result uses): crop mask -> source pixels ->
 * full mask.
 * @param {DecodedCode[]} hits
 * @param {{x: number, y: number, map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[]}} crop
 * @param {{width: number, height: number}} croppedFrame
 * @param {{width: number, height: number}} fullFrame
 * @param {number} sourceW @param {number} sourceH
 */
const mapRoiHits = (hits, crop, croppedFrame, fullFrame, sourceW, sourceH) => {
  if (!hits.length) return hits;
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
 * Native-resolution ROI pass (roadmap item 8): crop the strongest compact
 * ink clusters out of the ORIGINAL bitmap (a fitting crop is padded 1:1,
 * never squeezed) and decode those. This is how a small code keeps its
 * module pitch after the full-frame binarizer downscale.
 *
 * @param {Awaited<ReturnType<typeof getWasmScanner>>} scanner
 * @param {Binarizer} bin
 * @param {ImageBitmap} bitmap
 * @param {{width: number, height: number, data: Uint8Array, bytesPerRow: number}} frame
 * @param {string[] | undefined} formats
 * @param {number} limit @param {number} deadline
 * @returns {Promise<DecodedCode[]>}
 */
async function decodeRoi(scanner, bin, bitmap, frame, formats, limit, deadline) {
  const sourceW = bitmap.width;
  const sourceH = bitmap.height;
  if (!sourceW || !sourceH) return [];
  const boxes = roiCandidates(frame, sourceW, sourceH, limit);
  if (!boxes.length) return [];
  bin.setThreshold?.(0.92);
  for (const box of boxes) {
    if (deadline - performance.now() <= 0) break;
    const crop = await cropDrawable(bitmap, box);
    const f = await bin.process(crop.canvas);
    const hits = decodeFrame(scanner, f, formats);
    if (hits.length) return mapRoiHits(hits, crop, f, frame, sourceW, sourceH);
    // A white-on-black symbol on a light page is a compact ink cluster whose
    // crop still carries the code's black paper plus a light margin, so the
    // full-frame ink heuristic never picks the inverted polarity. The crop
    // is small: flip it and let the decoders' own format/RS validation
    // arbitrate (a wrong polarity costs one crop, never a false result).
    const flipped = f.data.map((/** @type {number} */ b) => ~b);
    const invHits = decodeFrame(scanner, { ...f, data: flipped }, formats);
    if (invHits.length) return mapRoiHits(invHits, crop, f, frame, sourceW, sourceH);
  }
  return [];
}

/** @param {FrameOpts} [opts] */
async function ensure(opts) {
  if (!pipeline) {
    pipeline = (async () => {
      const bin = await createBinarizer(opts ?? {});
      const scanner = await getWasmScanner();
      kind = bin.kind;
      if (opts?.accuracy === "high") {
        primaryThreshold = opts.threshold ?? 0.92;
        // bin.setThreshold must exist on both binarizer paths.
        bin.setThreshold?.(primaryThreshold);
      }
      return { bin, scanner };
    })();
    pipeline.catch(() => {
      pipeline = null; // allow a retry on the next frame
    });
  }
  return pipeline;
}

/**
 * Worker-scope postMessage (the tsconfig DOM lib types `self` as
 * Window, whose postMessage signature has no transfer list).
 * @param {unknown} msg
 * @param {Transferable[]} [transfer]
 */
const post = (msg, transfer = []) => {
  (/** @type {{ postMessage: (m: unknown, t?: Transferable[]) => void }} */ (self)).postMessage(msg, transfer);
};

self.onmessage = async (/** @type {MessageEvent} */ e) => {
  const msg = /** @type {{ type: "init", opts?: FrameOpts } | { type: "frame", bitmap: ImageBitmap, opts?: FrameOpts }} */ (e.data);
  if (msg?.type === "init") {
    try {
      await ensure(msg.opts);
      // Single source of truth: the symbology list comes from the wasm.
      const symbologies = await getSupportedSymbologies();
      post({ type: "ready", kind, symbologies });
    } catch (err) {
      post({ type: "error", message: String(err) });
    }
    return;
  }
  if (msg?.type === "frame") {
    // Frames are strictly serial (the main thread sends one only after the
    // previous answer), so there is no request id to echo back.
    if (msg.type !== "frame") return;
    const { bitmap, opts } = msg;
    /** @type {FrameResultMsg} */
    let payload;
    let /** @type {ArrayBuffer | undefined} */ mask;
    try {
      const { bin, scanner } = await ensure(opts);
      const t0 = performance.now();
      const frame = await bin.process(bitmap);
      let results = decodeFrame(scanner, frame, opts?.formats);
      // accuracy-high: sweep a second Bradley threshold over the SAME frame
      // (both binarizers allocate a fresh mask per process, so the primary
      // mask in `frame` survives untouched for the observers below).
      if (opts?.accuracy === "high" && typeof bin.setThreshold === "function") {
        bin.setThreshold(ALT_THRESHOLDS[altIdx++ % ALT_THRESHOLDS.length] ?? 0.92);
        const altFrame = await bin.process(bitmap);
        bin.setThreshold(primaryThreshold);
        results = mergeResults(results, decodeFrame(scanner, altFrame, opts?.formats));
      }
      // Diagonal retry (roadmap item 4): a mask-level 45° rotation cannot
      // restore Aztec's bullseye from a mask the camera already
      // resampled; rotating the SOURCE with antialiasing and binarizing
      // again can. Only explicit diagonal tiers and at most twice a
      // second — the rotation + extra binarize costs ~20-50ms, and the
      // "Everything" mode must keep its fps.
      const requested = /** @type {string[] | undefined} */ (opts?.formats);
      if (!results.length && requested?.length) {
        const diag = requested.filter((f) => DIAGONAL_FORMATS.has(String(f).toLowerCase()));
        const now = performance.now();
        if (diag.length && now - lastDiagonalAt >= 500) {
          lastDiagonalAt = now;
          bin.setThreshold?.(0.92);
          for (const deg of [45, -45]) {
            const rotated = await rotateDrawable(bitmap, deg);
            const df = await bin.process(rotated.canvas);
            const hits = decodeFrame(scanner, df, diag);
            if (hits.length) {
              for (const hit of hits) {
                if (hit.position?.length) {
                  hit.position = rotated.map(df.width, df.height, hit.position);
                }
              }
              results = hits;
              break;
            }
          }
        }
      }
      // Native-resolution ROI pass (roadmap item 8): the full frame was
      // downscaled, so a small code lost its module pitch. Crop the
      // strongest compact ink clusters from the ORIGINAL bitmap and
      // decode those at native resolution. Live frames get two crops at
      // most twice a second — the confirmation law needs the frames to
      // keep flowing.
      if (!results.length) {
        const now = performance.now();
        // A native-ROI hit is deliberately not repeated on every frame
        // (crop + binarize costs more than the full-frame 1D tier), but
        // the confirmation law needs the SAME value on two CONSECUTIVE
        // frames. While a hit is hot, bypass the cooldown.
        if (now <= roiHotUntil || now - lastRoiAt >= ROI_MIN_INTERVAL_MS) {
          lastRoiAt = now;
          const roi = await decodeRoi(
            scanner,
            bin,
            bitmap,
            frame,
            /** @type {string[] | undefined} */ (opts?.formats),
            2,
            now + 120,
          );
          if (roi.length) {
            results = roi;
            roiHotUntil = now + 600;
          }
        }
      }
      // Presence signal for the auto-framing policy: is a QR in view but
      // failing to decode? Only meaningful when the QR tier ran.
      const fmts = /** @type {string[] | undefined} */ (opts?.formats);
      const qrRan = !fmts?.length || fmts.some((f) => f === "qr" || f === "qrcode");
      const qrPresence = qrRan ? scanner.qr_presence() : undefined;
      // Both binarizers allocate a fresh mask buffer per frame, so its
      // ArrayBuffer can be transferred back to the main thread zero-copy
      // (observers — debug drawing, instrumentation — get the real mask).
      const mask = frame.data.buffer;
      payload = {
        type: "frameResult",
        results,
        qrPresence,
        frameW: frame.width,
        frameH: frame.height,
        bytesPerRow: frame.bytesPerRow,
        mask,
        decodeMs: Math.round(performance.now() - t0),
        kind,
      };
    } catch (err) {
      payload = {
        type: "frameResult",
        results: [],
        error:
          String(err) +
          " || STACK: " +
          String(err instanceof Error ? err.stack ?? "" : "").slice(0, 500),
      };
    } finally {
      // The bitmap is owned by the worker once transferred.
      bitmap.close?.();
    }
    post(payload, mask ? [/** @type {ArrayBuffer} */ (mask)] : []);
  }
};
