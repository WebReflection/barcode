/**
 * CPU fallback binarizer — same BinarizedFrame contract as the GPU path
 * (1bpp, bit (x,y) = data[y*bytesPerRow + (x>>3)] >> (x&7) & 1, bytesPerRow
 * a multiple of 4, set bit = ink). Slower (~30-60ms/frame) but runs on any
 * browser: canvas 2D + integral-image box mean + Bradley threshold.
 */
/** @typedef {import("./gpu/binarizer.js").BinarizedFrame} BinarizedFrame */
export class CpuBinarizer {
    /** @param {Required<import("./gpu/binarizer.js").BinarizerTuning>} opts */
    constructor(opts: Required<import("./gpu/binarizer.js").BinarizerTuning>);
    /**
     * Override the Bradley threshold for subsequent process() calls (accuracy-
     * high pipeline sweeps a second threshold per frame).
     * @param {number} t
     */
    setThreshold(t: number): void;
    /** Release cached luma/integral buffers so nothing lingers between frames. */
    clearCache(): void;
    /**
     * Same source contract as the GPU binarizer: video elements or
     * canvases/ImageBitmaps. Repeated calls on the same immutable source
     * (threshold sweeps) reuse the cached luma + integral image.
     * @param {HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas} source
     * @returns {Promise<BinarizedFrame>}
     */
    process(source: HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas): Promise<BinarizedFrame>;
    destroy(): void;
    #private;
}
export type BinarizedFrame = import("./gpu/binarizer.js").BinarizedFrame;
