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
