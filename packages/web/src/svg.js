/**
 * Re-presentation API: turn a decoded value back into a clean SVG barcode.
 *
 *   import { createSVG } from "./svg.js";
 *   const node = await createSVG({ type: "CODE39", value: "GMMNDR78E17A271Q" });
 *   container.append(node);
 *
 * The `await` is real work: encoding runs in a dedicated worker (WASM), so
 * the main thread never blocks. The returned SVGSVGElement is sized by its
 * viewBox — scale it freely with CSS. Every 1D symbology the SDK decodes
 * is supported, plus QR Code (v1-40; EC/mask/version selectable — defaults
 * M / 2 / smallest), Micro QR, Data Matrix (explicit ECC200 sizes
 * possible), Aztec and PDF417 (EC level/columns selectable).
 */

import { svg } from "@webreflection/utils/dom-content";
import { escape } from "html-escaper";
import { normalizeRenderOptions } from "./render-options.js";

/** @typedef {import("./encode-worker.js").EncodedMsg} EncodedMsg */
/** @typedef {import("./encode-worker.js").EncodeReply} EncodeReply */

// Fast-path validation only — the encode worker is the authority (it
// returns "unknown symbology" for anything it cannot encode). Keep this in
// sync with the wasm SYMBOLOGIES list, or a scan RESULT of a family that is
// missing here cannot be re-presented as SVG (a DataMatrix result used to
// throw "unsupported symbology" in the demo).
const SUPPORTED = new Set([
  "ean13",
  "ean8",
  "upce",
  "upca",
  "code39",
  "code93",
  "code128",
  "itf",
  "codabar",
  "databar",
  "databarltd",
  "databarstk",
  "databarstackomni",
  "qr",
  "qrcode",
  "microqr",
  "datamatrix",
  "aztec",
  "pdf417",
]);

/**
 * Scan results for GS1 DataBar carry the `(01)` AI prefix and the GTIN-14
 * check digit; the encoder takes the 13-digit body. Without this the demo
 * result card shows the value but no SVG (`databar needs exactly 13
 * digits`). Hand-built 13-digit values pass through unchanged.
 * @param {string} symbology canonical lowercase name
 * @param {string} text
 */
export function encodableText(symbology, text) {
  if (symbology === "databar" || symbology === "databarltd" || symbology === "databarstk") {
    const m = /^(?:\(01\))?(\d{13})\d$/.exec(text);
    if (m) return m[1];
  }
  return text;
}

/** @type {Worker | null} */
let worker = null;
/** @type {Promise<Worker> | null} */
let ready = null;
/** The single in-flight encode request (strictly serial, like the Scanner). */
let /** @type {{ resolve: (m: EncodedMsg) => void, reject: (e: Error) => void, timer: number } | null} */ pending =
    null;
/** Serializes concurrent createSVG calls. */
let /** @type {Promise<unknown>} */ chain = Promise.resolve();

/** Spawn the encoder worker once and await its bootstrap. */
function ensureWorker() {
  ready ??= new Promise((resolve, reject) => {
    const w = new Worker(new URL("./encode-worker.js", import.meta.url), {
      type: "module",
    });
    worker = w;
    const to = setTimeout(() => reject(new Error("encode worker init timeout")), 10000);
    w.addEventListener("message", (/** @type {MessageEvent} */ ev) => {
      const m = /** @type {EncodeReply} */ (ev.data);
      if (m?.type === "ready") {
        clearTimeout(to);
        resolve(w);
      } else if (m?.type === "error") {
        clearTimeout(to);
        reject(new Error(m.message));
      } else if (m?.type === "encoded" || m?.type === "encodeError") {
        const p = pending;
        if (p) {
          clearTimeout(p.timer);
          pending = null;
          m.type === "encoded" ? p.resolve(m) : p.reject(new Error(m.message));
        }
      }
    });
    w.addEventListener("error", (/** @type {ErrorEvent} */ ev) => {
      clearTimeout(to);
      reject(new Error(ev.message ?? "encode worker error"));
    });
    // Messages queue until the worker script runs — safe to send now.
    w.postMessage({ type: "init" });
  });
  return ready;
}

/**
 * @typedef {import("./render-options.js").RenderOptions & {
 *   type: string,
 *   value: string,
 * }} CreateSVGOptions
 */

/**
 * Encode `value` as `type` and return an SVGSVGElement representing it.
 *
 * `module` and `height` size the symbol; the rest are symbol layout
 * options kept in sync with the public `render(result, opts)`:
 * `ec` ("L"|"M"|"Q"|"H"), `mask` (0..7) and `version` (1..40) for QR,
 * `level` (0..8) and `columns` (1..30) for PDF417, `rows`/`cols` for a
 * Data Matrix of an explicit ECC200 size. All optional;
 * see render-options.js for the defaults.
 * @param {CreateSVGOptions} opts
 * @returns {Promise<SVGSVGElement>}
 */
export async function createSVG({ type, value, module = 4, height, ...options }) {
  const symbology = String(type ?? "").toLowerCase().replace(/[^a-z0-9]/g, "");
  if (!SUPPORTED.has(symbology)) {
    throw new Error(`unsupported symbology: ${type}`);
  }
  const text = String(value ?? "");
  if (!text) throw new Error("missing value");
  const payload = encodableText(symbology, text);
  const layout = normalizeRenderOptions(options);
  await ensureWorker();
  const fx = await (chain = chain.then(() => {
    const { promise, resolve, reject } =
      /** @type {PromiseWithResolvers<EncodedMsg>} */ (Promise.withResolvers());
    const p = { resolve, reject, timer: 0 };
    pending = p;
    p.timer = setTimeout(() => {
      pending = null;
      reject(new Error("encode timeout"));
    }, 5000);
    /** @type {Worker} */ (worker).postMessage({
      type: "encode",
      symbology,
      text: payload,
      module,
      height: height ?? 30 * module,
      ...layout,
    });
    return promise;
  }));
  return bitmapToSVG(/** @type {EncodedMsg} */ (fx), text);
}

/**
 * Build the SVG element from a 1bpp fixture bitmap: rows are collapsed
 * into horizontal ink runs, and consecutive rows with identical runs merge
 * into single rects — one band for 1D symbologies, per-row bands for 2D.
 * @param {{ width: number, height: number, bytesPerRow: number, data: Uint8Array }} fx
 * @param {string} value
 * @returns {SVGSVGElement}
 */
export function bitmapToSVG({ width, height, bytesPerRow, data }, value) {
  const ink = (/** @type {number} */ x, /** @type {number} */ y) =>
    /** @type {number} */ (data[y * bytesPerRow + (x >> 3)]) >> (x & 7) & 1;

  const emit = (/** @type {number[][]} */ runs, /** @type {number} */ y0, /** @type {number} */ y1) => {
    for (const [x, w] of runs) {
      rects += `<rect x="${x}" y="${y0}" width="${w}" height="${y1 - y0}" fill="var(--barcode-ink, #000)"/>`;
    }
  };

  /** @type {number[][] | null} */ let prev = null;
  let /** @type {number} */ bandY0 = 0;
  let /** @type {string} */ rects = "";

  for (let y = 0; y < height; y++) {
    /** @type {number[][]} */
    const runs = [];
    let start = -1;
    for (let x = 0; x < width; x++) {
      if (ink(x, y)) {
        if (start < 0) start = x;
      } else if (start >= 0) {
        runs.push([start, x - start]);
        start = -1;
      }
    }
    if (start >= 0) runs.push([start, width - start]);

    const same =
      !!prev &&
      prev.length === runs.length &&
            prev.every(([px, pw], i) => {
        const r = /** @type {number[]} */ (runs[i]);
        return r[0] === px && r[1] === pw;
      });
    if (!same) {
      if (prev) emit(prev, bandY0, y);
      bandY0 = y;
    }
    prev = runs;
  }
  if (prev) emit(prev, bandY0, height);

  // XML 1.0 forbids raw C0/C1 controls in an attribute. A GS1 payload
  // carries GS (0x1D, FNC1) and would otherwise serialize into an SVG
  // that browsers refuse to decode as an image. Keep the label valid.
  const label = escape(
    value.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F]/g, " "),
  );
  const markup =
    `<svg xmlns="http://www.w3.org/2000/svg" class="barcode-svg" viewBox="0 0 ${width} ${height}" ` +
    `width="${width}" height="${height}" role="img" aria-label="${label}">` +
    // Styleable via CSS custom properties; the fallbacks keep the SVG
    // self-contained (white paper, black ink) so it reads on any surface —
    // including <img> rasterization, where outer CSS never reaches.
    `<rect width="${width}" height="${height}" fill="var(--barcode-paper, #fff)"/>${rects}</svg>`;
  return /** @type {SVGSVGElement} */ (svg(markup).firstChild);
}
