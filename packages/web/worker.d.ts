// Types for the worker entry (`@webreflection/barcode/worker`, dist/worker.js).
//
// Spawn it —
//   new Worker('<url-to>/worker.js', { type: 'module' })
// — or point barcode({ worker }) at it, or import the auto entry
// (`@webreflection/barcode`) inside a worker. The default export is the
// bootstrap promise; the module installs its listener synchronously and
// resolves after the main thread's ping/ready handshake.
//
// The first port-carrying 'message' event binds the protocol, exactly
// once. Port-less messages pass through untouched, so an embedder can
// keep its own protocol on the same worker.

declare const bootstrap: Promise<{
  decode: (msg: unknown) => Promise<unknown>;
  encode: (msg: unknown) => Promise<unknown>;
}>;
export default bootstrap;
