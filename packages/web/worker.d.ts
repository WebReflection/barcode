// Types for the worker entry (`@webreflection/barcode/worker`, dist/worker.js).
//
// Spawn it —
//   new Worker('<url-to>/worker.js', { type: 'module' })
// — or point barcode({ worker }) at it, or import it statically at the top
// of your own module Worker. The default export is the bootstrap promise:
// the module installs the port-capture listener synchronously at
// evaluation, and the promise resolves with the bound decode/encode
// surface when the page's first port-carrying message arrives (it
// rejects if binding that surface fails).
//
// The first port-carrying 'message' event binds the protocol, exactly
// once. Port-less messages pass through untouched, so an embedder can
// keep its own protocol on the same worker.

declare const bootstrap: Promise<{
  decode: (msg: unknown) => Promise<unknown>;
  encode: (msg: unknown) => Promise<unknown>;
}>;
export default bootstrap;
