// Module worker used by demo/artifact-test.html: a consumer worker that
// imports the SDK worker entry STATICALLY at the top (the contract), so the
// page's first message — the MessagePort — is captured.
import entry from "../dist/worker.js";

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
