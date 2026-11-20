// Types for the default entry (`@webreflection/barcode`) — pages only.
//
// In a worker, import '@webreflection/barcode/worker' statically at the
// top: the MessagePort must be the first message that worker receives.

export { default } from "./main.js";

export type {
  Barcode,
  BarcodeOptions,
  Code,
  RenderOptions,
  ScanOptions,
} from "./main.js";
