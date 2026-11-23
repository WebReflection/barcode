/** @typedef {import("./gpu/binarizer.js").BinarizerOptions} BinarizerOptions */
/**
 * @param {BinarizerOptions} [opts]
 * @returns {Promise<{ process(video: HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas): Promise<import("./gpu/binarizer.js").BinarizedFrame>, destroy(): void, kind: "gpu" | "cpu", setThreshold(t: number): void, clearCache(): void }>}
 */
export function createBinarizer(opts?: BinarizerOptions): Promise<{
    process(video: HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas): Promise<import("./gpu/binarizer.js").BinarizedFrame>;
    destroy(): void;
    kind: "gpu" | "cpu";
    setThreshold(t: number): void;
    clearCache(): void;
}>;
export type BinarizerOptions = import("./gpu/binarizer.js").BinarizerOptions;
