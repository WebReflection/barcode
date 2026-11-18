export class GpuBinarizer {
    /**
     * @param {BinarizerOptions} [opts]
     * @returns {Promise<GpuBinarizer>}
     */
    static create(opts?: BinarizerOptions): Promise<GpuBinarizer>;
    /**
     * @param {GPUDevice} device
     * @param {Required<import("./binarizer.js").BinarizerTuning>} opts
     */
    constructor(device: GPUDevice, opts: Required<import("./binarizer.js").BinarizerTuning>);
    /**
     * Override the Bradley threshold for subsequent process() calls (the
     * uniform is rewritten every frame, so this is free). Used by the
     * accuracy-high pipeline to sweep a second threshold per frame.
     * @param {number} t
     */
    setThreshold(t: number): void;
    /**
     * Binarize one frame. Accepts any `copyExternalImageToTexture` source
     * with dimensions: an HTMLVideoElement (videoWidth/videoHeight) or a
     * canvas / ImageBitmap (width/height) — the self-test harness feeds
     * synthetic specimen canvases through the same path.
     * @param {HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas} source
     * @returns {Promise<BinarizedFrame>}
     */
    process(source: HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas): Promise<BinarizedFrame>;
    destroy(): void;
    #private;
}
export type BinarizedFrame = {
    width: number;
    height: number;
    /**
     * Packed 1bpp ink mask, `bytesPerRow`-aligned rows.
     */
    data: Uint8Array;
    bytesPerRow: number;
};
export type BinarizerTuning = {
    /**
     * Longest edge of the binarized frame. 1D symbologies want >= 1000.
     */
    maxSize?: number | undefined;
    /**
     * Bradley window diameter = min(w, h) / windowDivisor.
     */
    windowDivisor?: number | undefined;
    /**
     * ink := luma < mean * threshold (Bradley's 0.15 => 0.85).
     */
    threshold?: number | undefined;
};
export type BinarizerOptions = BinarizerTuning & {
    requireGpu?: boolean;
};
