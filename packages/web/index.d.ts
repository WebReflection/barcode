// Types for the thread-aware default entry (`@webreflection/barcode`).
//
// The runtime default depends on where the module is imported:
//   - main thread → the barcode() session factory (see ./main.d.ts)
//   - worker      → the bootstrap promise (see ./worker.d.ts)
//
// TypeScript cannot see the runtime context, so the honest type is the
// union. Import '@webreflection/barcode/main' or
// '@webreflection/barcode/worker' for a precise type.

declare const entry:
  | typeof import("./main.js").default
  | Promise<unknown>;

export default entry;

// Type-only: no runtime named exports exist on this entry.
export type {
  Barcode,
  BarcodeOptions,
  Code,
  RenderOptions,
  ScanOptions,
} from "./main.js";
