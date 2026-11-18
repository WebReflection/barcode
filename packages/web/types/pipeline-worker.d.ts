export type Binarizer = {
    process(video: HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas): Promise<import("./gpu/binarizer.js").BinarizedFrame>;
    destroy(): void;
    kind: "gpu" | "cpu";
    setThreshold(t: number): void;
};
export type FrameOpts = import("./gpu/binarizer.js").BinarizerOptions & {
    accuracy?: Accuracy;
    formats?: string[];
};
export type FrameResultMsg = {
    type: "frameResult";
    results: DecodedCode[];
    error?: string;
    qrPresence?: number;
    frameW?: number;
    frameH?: number;
    bytesPerRow?: number;
    mask?: ArrayBufferLike;
    decodeMs?: number;
    kind?: string;
};
export type DecodedCode = import("./decoder.js").DecodedCode;
export type Accuracy = import("./scanner.js").Accuracy;
