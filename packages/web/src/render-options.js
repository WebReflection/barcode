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
 *
 * @typedef {{
 *   module?: number,
 *   height?: number,
 *   ec?: "L" | "M" | "Q" | "H",
 *   mask?: number,
 *   version?: number,
 *   level?: number,
 *   columns?: number,
 *   rows?: number,
 *   cols?: number,
 * }} RenderOptions
 */

/** ISO 18004 raw 2-bit EC field: M=00, L=01, H=10, Q=11. */
/** @type {Record<string, number>} */
const QR_EC = { M: 0, L: 1, H: 2, Q: 3 };

/**
 * @param {string} name
 * @param {number} value
 * @param {number} lo
 * @param {number} hi
 */
const int = (name, value, lo, hi) => {
  if (!Number.isInteger(value) || value < lo || value > hi) {
    throw new TypeError(`render: ${name} must be an integer ${lo}..${hi}`);
  }
  return value;
};

/**
 * Normalize the public options into the wasm wire sentinels (`-1`/`0` =
 * encoder default). Validation happens here so callers get a clear
 * TypeError before a frame reaches the worker.
 * @param {RenderOptions} [opts]
 * @returns {{ ec: number, mask: number, version: number, level: number,
 *   columns: number, rows: number, cols: number }}
 */
export function normalizeRenderOptions(opts = {}) {
  let ec = -1;
  if (opts.ec != null) {
    const key = String(opts.ec).trim().toUpperCase();
    const field = QR_EC[key];
    if (field == null) {
      throw new TypeError('render: ec must be "L", "M", "Q" or "H"');
    }
    ec = field;
  }
  const hasRows = opts.rows != null;
  const hasCols = opts.cols != null;
  if (hasRows !== hasCols) {
    throw new TypeError("render: rows and cols must be provided together");
  }
  return {
    ec,
    mask: opts.mask == null ? -1 : int("mask", opts.mask, 0, 7),
    version: opts.version == null ? 0 : int("version", opts.version, 1, 40),
    level: opts.level == null ? -1 : int("level", opts.level, 0, 8),
    columns: opts.columns == null ? 0 : int("columns", opts.columns, 1, 30),
    rows: hasRows ? int("rows", opts.rows ?? 0, 1, 144) : 0,
    cols: hasCols ? int("cols", opts.cols ?? 0, 1, 144) : 0,
  };
}
