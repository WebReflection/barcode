// Module worker used by demo/artifact-test.html: prove the thread-aware
// default entry (`@webreflection/barcode`) chooses worker.js here.
//
// The first port-carrying message is the handshake from barcode(); the
// default export is the bootstrap promise (non-blocking until then).
import entry from "../dist/index.js";

const bootstrap = /** @type {Promise<{ decode: Function, encode: Function }>} */ (entry);
const globals = /** @type {any} */ (globalThis);

postMessage({
  loaded: true,
  kind: typeof entry,
  then: typeof bootstrap?.then,
  wgs: typeof WorkerGlobalScope,
});
addEventListener("message", (event) =>
  postMessage({ gotPort: !!event.ports?.length, data: event.data }));

bootstrap.then((bindings) => {
  globals.__bound =
    typeof bindings.decode === "function" &&
    typeof bindings.encode === "function";
  postMessage({ kind: typeof entry, bound: globals.__bound });
});

addEventListener("error", (event) => postMessage({ error: event.message }));
addEventListener("unhandledrejection", (event) =>
  postMessage({ error: String(event.reason) }));
