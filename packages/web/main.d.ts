// Public API types for barcode-web's main entry (api/main.js).
// The contract: one barcode() per page/SPA/session, returning
// { scan, render, worker }. Hand-authored against the frozen contract —
// the implementation is plain JS with JSDoc, so this file is the
// consumer-facing truth. Keep it in sync with api/README.md.

export interface Code {
  /** Lowercase canonical name on render input ("code128"); decode
   *  results carry the wasm's title-cased name ("Code128"). */
  symbology: string;
  text: string;
  /** Exact payload bytes. `text` is a display string — binary payloads
   *  (QR byte mode, Aztec, DataMatrix, PDF417) survive only here, where
   *  they are mapped to `text` Latin-1-style (byte → U+0000..U+00FF). */
  raw: Uint8Array;
  /** The four corner points, in mask coordinates. */
  position: Array<{ x: number; y: number }>;
  /** Centering rank: 1 = dead center, 0 = frame corner. */
  score?: number;
}

export interface BarcodeOptions {
  /**
   * Where the pipeline lives. A string/URL spawns a dedicated module
   * Worker instantly; an already-created Worker is used as-is. Either
   * way `Barcode.worker` is the live Worker instance. The session owns
   * the worker for its whole lifetime — abort never terminates it;
   * handing the same worker to a second session throws ('duplicated
   * worker').
   * @default new URL('./worker.js', import.meta.url)
   */
  worker?: string | URL | Worker;
  /** Session-wide symbology default: 'qr' | 'barcode' | exact name(s). */
  type?: string | string[];
  /** Session-wide accuracy default: 'low' | 'auto' (default) | 'high'. */
  accuracy?: 'low' | 'auto' | 'high';
  /** Camera lens hint for live scans. */
  lens?: 'ultrawide' | 'wide' | 'default';
}

export interface ScanOptions {
  /** Exactly one of into / video / image may be set (a TypeError is
   *  thrown otherwise). Default: { into: document.body } — the default
   *  UI with its own ✕ that aborts the scan. */
  into?: string | Node;
  /** Bring your own <video> (chain your own signal to stop early). */
  video?: HTMLVideoElement;
  /** Still scan: canvas, ImageBitmap, HTMLImageElement, SVG, Blob/File. */
  image?: CanvasImageSource | Blob | OffscreenCanvas;
  /** Graceful stop → the promise resolves undefined (never throws). */
  signal?: AbortSignal;
  /** Escape hatch over the mode defaults (500ms live, 5000ms still). */
  latency?: number;
  /** Per-scan override of the session type/accuracy/lens. */
  type?: string | string[];
  accuracy?: 'low' | 'auto' | 'high';
  lens?: 'ultrawide' | 'wide' | 'default';
  /** Live framing: 'square' (default — what you frame is what decodes)
   *  or 'full' (the explicit opt-out). */
  crop?: 'square' | 'full';
  /** Initial camera zoom factor. */
  zoom?: number;
  /** Initial torch state (the default UI also carries a toggle). */
  torch?: boolean;
}

/** Layout for render(): every field is optional and keeps its default. */
export interface RenderOptions {
  /** Module (dot/bar) size in px. @default 4 */
  module?: number;
  /** 1D bar height in px. @default 30 * module */
  height?: number;
  /** QR error correction level. @default "M" */
  ec?: "L" | "M" | "Q" | "H";
  /** QR mask pattern 0..7. @default 2 */
  mask?: number;
  /** QR version 1..40; pads the symbol to that size. @default smallest that fits */
  version?: number;
  /** PDF417 error correction level 0..8. @default encoder default */
  level?: number;
  /** PDF417 data columns 1..30. @default auto */
  columns?: number;
  /** Data Matrix ECC200 size in modules INCLUDING borders, e.g. 12 for a 12×36 DMRE. @default smallest square */
  rows?: number;
  /** Data Matrix ECC200 columns; provide together with `rows`. @default smallest square */
  cols?: number;
}

export interface Barcode {
  /**
   * One complete scan. Resolves ONCE with the best code (most centered
   * first, larger symbol as tiebreak) or `undefined` — abort, still
   * latency expired, or a graceful end. Real errors (camera denied,
   * broken pipeline) reject. One scan at a time per session.
   */
  scan(options?: ScanOptions): Promise<Code | undefined>;
  /**
   * Render a scan result (or a bare { symbology, text }) as an
   * SVGSVGElement, styleable via --barcode-ink / --barcode-paper.
   * `undefined`/empty → null. Stateless: safe to fire N times in
   * parallel.
   */
  render(
    result:
      | { symbology: string; text?: string; raw?: Uint8Array }
      | null
      | undefined,
    opts?: RenderOptions,
  ): Promise<SVGSVGElement | null>;
  /** The live Worker instance — the one spawned from the string/URL
   *  option, or the caller's own, verbatim. */
  worker: Worker;
}

declare function barcode(options?: BarcodeOptions): Barcode;
export default barcode;
