/**
 * Thread-aware default entry — the `@webreflection/barcode` specifier.
 *
 * One import works in both contexts:
 *
 *   // main thread: the default is barcode(options) → { scan, render, worker }
 *   import barcode from '@webreflection/barcode';
 *   const { scan, render } = barcode();
 *
 *   // dedicated/shared worker: the default is the bootstrap promise
 *   // (non-blocking by design — the listener is installed synchronously;
 *   // the promise resolves when the main thread hands over its port)
 *   import bootstrap from '@webreflection/barcode';
 *
 * The detection runs BEFORE anything DOM-ish is imported: on a worker we
 * only load `worker.js`, so the SDK never evaluates main-thread code there.
 * `WorkerGlobalScope` is exposed in dedicated/shared/service workers and
 * undefined on the main thread, so this never touches `window`.
 *
 * Use the explicit entries when a static import is preferable:
 *   '@webreflection/barcode/main'   main thread only
 *   '@webreflection/barcode/worker' worker only
 */
const inWorker = typeof WorkerGlobalScope !== "undefined";

const { default: entry } = inWorker
  ? await import("./worker.js")
  : await import("./main.js");

export default entry;
