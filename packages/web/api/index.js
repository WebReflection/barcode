/**
 * The default entry — for pages.
 *
 *   import barcode from '@webreflection/barcode';
 *   const { scan, render } = barcode();
 *
 * In a worker, import '@webreflection/barcode/worker' STATICALLY at the
 * top instead. The MessagePort handshake is the first message the page
 * sends to that worker; a module imported lazily (after an await) cannot
 * install the capture in time, so the channel is not guaranteed.
 */
if (typeof WorkerGlobalScope !== "undefined") {
  throw new Error(
    "@webreflection/barcode: in a worker import '@webreflection/barcode/worker' statically at the top — the MessagePort must be the first message.",
  );
}

export { default } from "./main.js";
