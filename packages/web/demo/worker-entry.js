// A real module Worker importing the thread-aware default entry. Its
// default export must be the bootstrap promise; the page binds it with
// barcode({ worker }) and then calls render/scan on the same worker.
import entry from "../api/index.js";

const bootstrap = /** @type {Promise<{ decode: Function, encode: Function }>} */ (
  /** @type {unknown} */ (entry)
);
const promise = typeof bootstrap?.then === "function";

postMessage({
  type: "loaded",
  kind: typeof entry,
  promise,
});

bootstrap.then(
  (/** @type {{ decode: Function, encode: Function }} */ bindings) => {
    postMessage({
      type: "entry",
      kind: typeof entry,
      promise,
      decode: typeof bindings?.decode === "function",
      encode: typeof bindings?.encode === "function",
    });
  },
  (/** @type {unknown} */ err) => postMessage({ error: String(err) }),
);

addEventListener("error", (event) => postMessage({ error: event.message }));
addEventListener("unhandledrejection", (event) =>
  postMessage({ error: String(event.reason) }));
