/**
 * Decoder bridge: loads the Rust/WASM scanner and exposes a typed API.
 * The mask layout passed to decode() is the GPU pipeline's BinarizedFrame
 * contract (1bpp, bytesPerRow-aligned), identical to barcode-core's
 * BitImage — no conversion needed.
 */

import init, { BarcodeScanner as WasmScanner, supported_symbologies } from "./wasm/pkg/barcode_wasm.js";
import wasmUrl from "./wasm/pkg/barcode_wasm_bg.wasm?url";

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
      await init({ module_or_path: wasmUrl });
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
  await init({ module_or_path: wasmUrl });
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
