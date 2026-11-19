/**
 * Scan results for GS1 DataBar carry the `(01)` AI prefix and the GTIN-14
 * check digit; the encoder takes the 13-digit body. Without this the demo
 * result card shows the value but no SVG (`databar needs exactly 13
 * digits`). Hand-built 13-digit values pass through unchanged.
 * @param {string} symbology canonical lowercase name
 * @param {string} text
 */
export function encodableText(symbology: string, text: string): string | undefined;
/**
 * @typedef {import("./render-options.js").RenderOptions & {
 *   type: string,
 *   value: string,
 * }} CreateSVGOptions
 */
/**
 * Encode `value` as `type` and return an SVGSVGElement representing it.
 *
 * `module` and `height` size the symbol; the rest are symbol layout
 * options kept in sync with the public `render(result, opts)`:
 * `ec` ("L"|"M"|"Q"|"H"), `mask` (0..7) and `version` (1..40) for QR,
 * `level` (0..8) and `columns` (1..30) for PDF417, `rows`/`cols` for a
 * Data Matrix of an explicit ECC200 size. All optional;
 * see render-options.js for the defaults.
 * @param {CreateSVGOptions} opts
 * @returns {Promise<SVGSVGElement>}
 */
export function createSVG({ type, value, module, height, ...options }: CreateSVGOptions): Promise<SVGSVGElement>;
/**
 * Build the SVG element from a 1bpp fixture bitmap: rows are collapsed
 * into horizontal ink runs, and consecutive rows with identical runs merge
 * into single rects — one band for 1D symbologies, per-row bands for 2D.
 * @param {{ width: number, height: number, bytesPerRow: number, data: Uint8Array }} fx
 * @param {string} value
 * @returns {SVGSVGElement}
 */
export function bitmapToSVG({ width, height, bytesPerRow, data }: {
    width: number;
    height: number;
    bytesPerRow: number;
    data: Uint8Array;
}, value: string): SVGSVGElement;
export type CreateSVGOptions = import("./render-options.js").RenderOptions & {
    type: string;
    value: string;
};
export type EncodedMsg = import("./encode-worker.js").EncodedMsg;
export type EncodeReply = import("./encode-worker.js").EncodeReply;
