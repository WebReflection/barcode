/**
 * The worker URL every public page hands to `barcode()`.
 *
 * The published main.js resolves "./worker.js" next to itself at RUN time
 * (the dist/ files are the contract); a bundler build must hand it the
 * emitted worker chunk instead, or the page would look for a non-existent
 * assets/worker.js and silently never decode.
 *
 * The docs build replaces this module (see cdnEntries in
 * vite.docs.config.js) with the one-line same-origin CDN shim URL, because
 * `new Worker(url)` only accepts a same-origin script.
 */
import workerUrl from "../api/worker.js?worker&url";

export default workerUrl;
