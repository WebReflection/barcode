/**
 * The reticle itself as crops, best first: the code the user is aiming
 * at, at native resolution, isolated from every surrounding code.
 *
 * Connected components merge a packed label's stacked barcodes + text
 * into one blob, and the density ranking then favours whichever code in
 * the frame has the most bars — not the one under the line. These
 * deterministic boxes encode the UI contract directly:
 *
 *   - wide horizontal strip centered on the frame's mid line (the
 *     viewfinder's scan line) for a 1D symbol under it;
 *   - tight strip on the same line for a short centered symbol beside a
 *     longer neighbor on the same band;
 *   - center square for a centered 2D symbol.
 *
 * @param {number} sourceW @param {number} sourceH
 * @param {{ linear?: boolean, square?: boolean }} [opts]
 * @returns {RoiBox[]}
 */
export function centerRoiCandidates(sourceW: number, sourceH: number, opts?: {
    linear?: boolean;
    square?: boolean;
}): RoiBox[];
/**
 * Propose source-space ROI boxes for the native-resolution pass, best
 * first. `frame` is the binarized full-frame mask; `sourceW/sourceH` are
 * the original bitmap dimensions that mask came from.
 *
 * @param {BinarizedFrame} frame
 * @param {number} sourceW
 * @param {number} sourceH
 * @param {number} [limit]
 * @returns {RoiBox[]}
 */
export function roiCandidates(frame: BinarizedFrame, sourceW: number, sourceH: number, limit?: number): RoiBox[];
/**
 * Row-scan (1D) ROI candidates. The compact-cluster detector above sees a
 * barcode as a thin band of high-frequency ink, and a wide symbol often
 * loses to a text block or merges with the box/hand around it. A 1D reader
 * only needs one clean scanline, so look for rows with many black/white
 * transitions and crop the densest horizontal run of them. Cheap: one
 * pass over the mask, and it runs only after the full-frame pass missed.
 *
 * @param {BinarizedFrame} frame
 * @param {number} sourceW @param {number} sourceH
 * @param {number} [limit]
 * @returns {RoiBox[]}
 */
export function linearRoiCandidates(frame: BinarizedFrame, sourceW: number, sourceH: number, limit?: number): RoiBox[];
/**
 * Global-histogram (Otsu) binarization of a small crop, upscaled to give
 * the 1D decoder back the module pitch it lost in the full-frame
 * downscale. The local Bradley pass is best on a whole scene, but on a
 * tightly cropped bar strip the blur makes every space a little darker
 * than its neighbourhood — a global two-class split keeps those spaces
 * white where `threshold * local mean` floods them. Runs in the worker;
 * the crop is small.
 *
 * @param {HTMLCanvasElement | OffscreenCanvas} source
 * @param {number} [scale] Upscale factor (capped so no side exceeds 1600px).
 * @returns {BinarizedFrame & { threshold: number }}
 */
export function otsuMask(source: HTMLCanvasElement | OffscreenCanvas, scale?: number): BinarizedFrame & {
    threshold: number;
};
/**
 * Crop a source drawable at native resolution. The returned `map` takes
 * points decoded from the binarized crop and puts them back into source
 * pixel coordinates (the binarizer pads a fitting crop 1:1 and only
 * downscales one larger than maxSize).
 *
 * @param {CanvasImageSource | HTMLVideoElement} source
 * @param {RoiBox} box
 * @returns {Promise<{
 *   canvas: OffscreenCanvas | HTMLCanvasElement,
 *   width: number,
 *   height: number,
 *   x: number,
 *   y: number,
 *   map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[],
 * }>}
 */
export function cropDrawable(source: CanvasImageSource | HTMLVideoElement, box: RoiBox): Promise<{
    canvas: OffscreenCanvas | HTMLCanvasElement;
    width: number;
    height: number;
    x: number;
    y: number;
    map: (fw: number, fh: number, points: {
        x: number;
        y: number;
    }[]) => {
        x: number;
        y: number;
    }[];
}>;
/**
 * Horizontal mirror of a crop, at source resolution. A 180°-presented
 * 1D symbol is the common phone case (the label is upside down), and the
 * mask-level mirror in wasm is starved when the binarization itself is
 * marginal: the same bar strip re-binarized from a mirrored SOURCE reads.
 * Mirrors the pixels (no mask round-trip), so the decoder gets a clean
 * upright run sequence.
 *
 * @param {CanvasImageSource} source
 * @returns {Promise<{
 *   canvas: OffscreenCanvas | HTMLCanvasElement,
 *   width: number,
 *   height: number,
 *   map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[],
 * }>}
 */
export function mirrorDrawable(source: CanvasImageSource): Promise<{
    canvas: OffscreenCanvas | HTMLCanvasElement;
    width: number;
    height: number;
    map: (fw: number, fh: number, points: {
        x: number;
        y: number;
    }[]) => {
        x: number;
        y: number;
    }[];
}>;
/**
 * White-pad a crop at source resolution. A tight bar strip can cut the
 * quiet zone Code 128 mandates (10 modules) down to a couple of modules:
 * a real DHL/GoFo label measures ~6 modules on one side and the row
 * scanners reject the otherwise clean strip, while zxing — lenient about
 * quiet zones — reads the very same photo. Drawing the strip onto paper
 * white restores the zone (and costs no extra camera frame). The
 * returned `map` takes points from the padded frame back into the
 * *source crop's* pixel coordinates, so the caller composes it with
 * `cropDrawable`'s map as before.
 *
 * @param {CanvasImageSource} source
 * @param {number} [pad] White margin on every side, in source pixels.
 * @returns {Promise<{
 *   canvas: OffscreenCanvas | HTMLCanvasElement,
 *   width: number,
 *   height: number,
 *   map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[],
 * }>}
 */
export function padDrawable(source: CanvasImageSource, pad?: number): Promise<{
    canvas: OffscreenCanvas | HTMLCanvasElement;
    width: number;
    height: number;
    map: (fw: number, fh: number, points: {
        x: number;
        y: number;
    }[]) => {
        x: number;
        y: number;
    }[];
}>;
export type BinarizedFrame = import("./gpu/binarizer.js").BinarizedFrame;
/**
 * A candidate box in the frame's SOURCE pixel coordinates (not mask
 * coordinates). `linear` marks a bar-strip proposal (1D decoders only).
 */
export type RoiBox = {
    x: number;
    y: number;
    w: number;
    h: number;
    score: number;
    linear?: boolean;
};
