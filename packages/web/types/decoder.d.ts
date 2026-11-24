/**
 * Lazy singleton: the .wasm loads on first call (call it on page load, not
 * on the first frame).
 * @returns {Promise<WasmScanner>}
 */
export function getWasmScanner(): Promise<WasmScanner>;
/**
 * The canonical symbology name list, from the wasm (single source of
 * truth with the `formats` API).
 * @returns {Promise<string[]>}
 */
export function getSupportedSymbologies(): Promise<string[]>;
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
export function decodeFrame(scanner: WasmScanner, frame: import("./gpu/binarizer.js").BinarizedFrame, formats?: string[], opts?: {
    still?: boolean;
    latency?: number;
}): DecodedCode[];
export type DecodedCode = {
    text: string;
    symbology: string;
    /**
     * Exact payload bytes (binary QR byte mode etc.).
     */
    raw: Uint8Array;
    /**
     * Corner quad in frame coordinates.
     */
    position: Array<{
        x: number;
        y: number;
    }>;
    /**
     * Reticle priority attached by api/worker.js
     * (1 = under the frame center, 0 = peripheral); absent on raw frames.
     */
    score?: number | undefined;
};
import { BarcodeScanner as WasmScanner } from "./wasm/pkg/barcode_wasm.js";
