/**
 * Normalize the public options into the wasm wire sentinels (`-1`/`0` =
 * encoder default). Validation happens here so callers get a clear
 * TypeError before a frame reaches the worker.
 * @param {RenderOptions} [opts]
 * @returns {{ ec: number, mask: number, version: number, level: number,
 *   columns: number, rows: number, cols: number }}
 */
export function normalizeRenderOptions(opts?: RenderOptions): {
    ec: number;
    mask: number;
    version: number;
    level: number;
    columns: number;
    rows: number;
    cols: number;
};
/**
 * Layout options for re-encoding a code as SVG — shared by the public
 * `render(result, opts)` and the dev-page `createSVG(...)`.
 *
 * Every field is OPTIONAL and keeps the historical default, so existing
 * call sites are unchanged. The options describe the SYMBOL, not the SVG
 * element: the returned element is sized by its viewBox, so scale it with
 * CSS freely.
 *
 * - `module`   pixels per module (default 4), 1D and 2D.
 * - `height`   bar height for 1D (default 30 × module).
 * - `ec`       QR error correction: "L" | "M" | "Q" | "H" (default "M").
 * - `mask`     QR mask pattern 0..7 (default 2).
 * - `version`  QR version 1..40 (default: smallest that fits).
 * - `level`    PDF417 error correction 0..8 (default: encoder default).
 * - `columns`  PDF417 data columns 1..30 (default: auto).
 * - `rows`     Data Matrix ECC200 size in modules INCLUDING borders,
 *              e.g. 12 for a 12×36 DMRE (default: smallest square).
 * - `cols`     see `rows`; both must be provided together.
 */
export type RenderOptions = {
    module?: number;
    height?: number;
    ec?: "L" | "M" | "Q" | "H";
    mask?: number;
    version?: number;
    level?: number;
    columns?: number;
    rows?: number;
    cols?: number;
};
