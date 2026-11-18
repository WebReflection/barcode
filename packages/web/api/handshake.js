/**
 * Port capture + readiness ping for the worker entry.
 *
 * The main thread only posts its MessagePort after this module answered
 * `barcode:ping`; the ping is queued by the browser until the worker is
 * listening, so `new Worker(...)` + `barcode({ worker })` in the same tick
 * is reliable regardless of how long the wasm/GPU graph takes.
 *
 * Exactly one port is ever accepted; port-less messages (an embedder's
 * own protocol on the same worker) are left untouched.

 */

/** @type {MessagePort | null} */
let captured = null;
/** @type {((port: MessagePort) => void)[]} */
const waiting = [];

/** @param {MessageEvent} event */
const capture = (event) => {
  const port = event.ports?.[0];
  if (!port) return;
  event.stopImmediatePropagation();
  removeEventListener("message", capture);
  captured = port;
  for (const resolve of waiting.splice(0)) resolve(port);
};
addEventListener("message", capture);

// Port-less readiness ping from `barcode()`: answering it is what makes
// the port handshake deterministic (the ping waits in the worker queue
// until this module is evaluated, even if it was sent before the worker
// started).
addEventListener("message", (event) => {
  if (event.data?.type === "barcode:ping") {
    // The page's dist bundle may know the wasm URL its bundler emitted;
    // the worker's own URL is unchanged (some dev servers reject query
    // strings on module workers).
    if (typeof event.data.wasm === "string") {
      /** @type {any} */ (globalThis).__barcodeWasm = event.data.wasm;
    }
    postMessage({ type: "barcode:ready", wasm: event.data.wasm ?? null });
  }
});

/** Resolves with the main thread's port (immediately if already seen). */
export const handshake = () =>
  captured
    ? Promise.resolve(captured)
    : new Promise((resolve) => waiting.push(resolve));
