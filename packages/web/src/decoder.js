/**
 * Decoder bridge: loads the Rust/WASM scanner and exposes a typed API.
 * The mask layout passed to decode() is the GPU pipeline's BinarizedFrame
 * contract (1bpp, bytesPerRow-aligned), identical to barcode-core's
 * BitImage — no conversion needed.
 */

import init, { BarcodeScanner as WasmScanner, supported_symbologies } from "./wasm/pkg/barcode_wasm.js";
import wasmUrl from "./wasm/pkg/barcode_wasm_bg.wasm?url";

/**
 * Resolve the wasm URL at INIT time (never at import time):
 *
 * 1. `?wasm=<encoded-url>` on THIS script (explicit escape hatch).
 * 2. `__barcodeWasm`, set by api/handshake.js from the page's
 *    `barcode:ping` (published dist main.js knows the URL its bundler
 *    emitted; the worker URL itself stays query-free).
 * 3. the sibling wasm.wasm (plain static hosting / direct worker.js).
 */
const resolveWasm = () => {
  try {
    const self = new URL(import.meta.url);
    const explicit = self.searchParams.get("wasm");
    if (explicit) return explicit;
    // dist/main.js puts the bundler-emitted wasm URL in the worker's
    // fragment: available at module-evaluation time, unlike a message.
    const hash = self.hash.replace(/^#/, "");
    if (hash.startsWith("wasm=")) {
      const fromHash = decodeURIComponent(hash.slice(5));
      if (fromHash) return fromHash;
    }
  } catch { /* import.meta.url is always a URL; be defensive */ }
  const fromPing = /** @type {any} */ (globalThis).__barcodeWasm;
  return typeof fromPing === "string" ? fromPing : wasmUrl;
};

/** @typedef {Object} DecodedCode
 * @property {string} text
 * @property {string} symbology
 * @property {Uint8Array} raw Exact payload bytes (binary QR byte mode etc.).
 * @property {Array<{x: number, y: number}>} position Corner quad in frame coordinates.
 */

/** @type {Promise<WasmScanner> | null} */
let instancePromise = null;

/**
 * Lazy singleton: the .wasm loads on first call (call it on page load, not
 * on the first frame).
 * @returns {Promise<WasmScanner>}
 */
export function getWasmScanner() {
  if (!instancePromise) {
    instancePromise = (async () => {
      await init(resolveWasm());
      return new WasmScanner();
    })();
  }
  return instancePromise;
}

/**
 * The canonical symbology name list, from the wasm (single source of
 * truth with the `formats` API).
 * @returns {Promise<string[]>}
 */
export async function getSupportedSymbologies() {
  await init(resolveWasm());
  return supported_symbologies();
}

/**
 * Synchronous decode of one binarized frame.
 * @param {WasmScanner} scanner
 * @param {import("./gpu/binarizer.js").BinarizedFrame} frame
 * @param {string[]} [formats] Decode-tier hint: only these symbologies are
 *   looked for. Names are the normalized lowercase forms results carry.
 *   Unknown names throw (wasm side).
 * @param {{ still?: boolean, latency?: number }} [opts] `still` switches
 *   to the still DM law (gallery/photo-picker single shots — no
 *   confirmation to amortize across); default is the 500ms camera law.
 *   `latency` overrides the still budget in ms (default 5000).
 * @returns {DecodedCode[]}
 */
export function decodeFrame(scanner, frame, formats, opts = {}) {
  const still = opts.still === true;
  const latency = opts.latency ?? 5000;
  const raw = /** @type {Array<{text: string, raw: number[] | Uint8Array, symbology: string, quad: number[]}>} */ (
    formats?.length
      ? still
        ? scanner.decode_filtered_still(frame.width, frame.height, frame.data, formats, latency)
        : scanner.decode_filtered(frame.width, frame.height, frame.data, formats)
      : still
        ? scanner.decode_still(frame.width, frame.height, frame.data, latency)
        : scanner.decode(frame.width, frame.height, frame.data)
  );
  return raw.map((r) => ({
    text: r.text,
    // Exact payload bytes (binary QR byte mode etc.); serde hands them
    // over as a plain array — make the typed view here, once.
    raw: r.raw instanceof Uint8Array ? r.raw : Uint8Array.from(r.raw ?? []),
    symbology: r.symbology,
    position: [0, 1, 2, 3].map((i) => ({
      x: r.quad[i * 2] ?? 0,
      y: r.quad[i * 2 + 1] ?? 0,
    })),
  }));
}
