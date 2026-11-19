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
export type BinarizedFrame = import("./gpu/binarizer.js").BinarizedFrame;
/**
 * A candidate box in the frame's SOURCE pixel coordinates (not mask
 * coordinates).
 */
export type RoiBox = {
    x: number;
    y: number;
    w: number;
    h: number;
    score: number;
};
