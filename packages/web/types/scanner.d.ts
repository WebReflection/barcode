/** @typedef {import("./camera.js").CameraLens} CameraLens */
/** @typedef {import("./camera.js").LensPreference} LensPreference */
/** @typedef {import("./camera.js").TrackCaps} TrackCaps */
/** @typedef {import("./camera.js").CameraAlternatives} CameraAlternatives */
/** @typedef {import("./camera.js").FocusInfo} FocusInfo */
/** @typedef {import("./live-scanner.js").DetectedCode} DetectedCode */
/** @typedef {import("./live-scanner.js").Accuracy} Accuracy */
/** @typedef {import("./live-scanner.js").LiveFrameResult} LiveFrameResult */
/** @typedef {import("./live-scanner.js").LiveScannerOptions} LiveScannerOptions */
/** @typedef {Object} ScanHint
 * @property {"no-decode"} reason
 * @property {number} elapsedMs Since scanning started (or the last decode).
 * @property {CameraAlternatives} cameras Other cameras the Web can use (may be empty).
 * @property {FocusInfo} focus Current focus state — `changing: true` means AF is still hunting.
 */
/** @typedef {import("./pipeline-worker.js").FrameResultMsg} FrameResultMsg */
/** The pipeline worker's reply union. */
/** @typedef {{ type: "ready", kind?: string, symbologies?: string[] } | { type: "error", message: string } | FrameResultMsg} PipelineReply */
/** @typedef {LiveScannerOptions & {
 *   binarizer?: import("./gpu/binarizer.js").BinarizerOptions,
 *   onFrame?: (frame: import("./gpu/binarizer.js").BinarizedFrame) => void,
 *   onHint?: (hint: ScanHint) => void,
 *   suggestAfterMs?: number,
 *   worker?: boolean,
 *   pipelineWorker?: URL | string | Worker,
 *   onStats?: (stats: ScannerStats) => void,
 * }} ScannerOptions */
/** @typedef {Object} ScannerStats
 * @property {number} fps
 * @property {number} frameMs
 * @property {number} frameW
 * @property {number} frameH
 * @property {number | null} zoom
 * @property {number} dropped Frames skipped while the pipeline was busy or maxFps throttled.
 */
export class Scanner extends LiveScanner {
    /** @param {ScannerOptions} opts */
    constructor(opts: ScannerOptions);
    /**
     * What this Scanner instance is actually running, for feature detection
     * and diagnostics.
     * @returns {{ binarizer: string, symbologies: string[], formats: string[] | null }}
     */
    get capabilities(): {
        binarizer: string;
        symbologies: string[];
        formats: string[] | null;
    };
    /** Which binarizer the pipeline worker actually got: "gpu", "cpu" or "init". */
    get binarizerKind(): string;
    /**
     * The torch's last-applied state: true/false after an explicit or
     * automated set, null while untouched. Lets UI reflect "auto" vs "on".
     * @returns {boolean | null}
     */
    get torchApplied(): boolean | null;
    /** @param {boolean} on @returns {Promise<boolean>} */
    setTorch(on: boolean): Promise<boolean>;
    /** The camera's current zoom factor, or null when the track has none. */
    get zoom(): number | null;
    /**
     * Set the zoom factor, optionally waiting for autofocus to settle on
     * native track zoom.
     *
     * @override
     * @param {number} factor
     * @param {boolean} [settle]
     * @returns {Promise<number | null>}
     */
    override zoomTo(factor: number, settle?: boolean): Promise<number | null>;
    /**
     * Switch lens at runtime; keeps the GPU pipeline and pump alive.
     * Waits for the lens to settle focus before returning.
     * @param {LensPreference} lens
     */
    setLens(lens: LensPreference): Promise<void>;
    /**
     * Lock focus at a fixed distance (meters).
     * @param {number} meters
     * @returns {Promise<boolean>}
     */
    setFocusDistance(meters: number): Promise<boolean>;
    /** Resume continuous autofocus. @returns {Promise<boolean>} */
    setFocusContinuous(): Promise<boolean>;
    /** Focus snapshot (mode, distance, whether AF is still hunting). @returns {Promise<FocusInfo>} */
    focusInfo(): Promise<FocusInfo>;
    /** @returns {Promise<CameraLens[]>} */
    lenses(): Promise<CameraLens[]>;
    /**
     * Stop scanning — camera closed, pump halted, pending frame rejected —
     * but KEEP the pipeline worker (binarizer + wasm) alive.
     */
    suspend(): Promise<void>;
    #private;
}
export type CameraLens = import("./camera.js").CameraLens;
export type LensPreference = import("./camera.js").LensPreference;
export type TrackCaps = import("./camera.js").TrackCaps;
export type CameraAlternatives = import("./camera.js").CameraAlternatives;
export type FocusInfo = import("./camera.js").FocusInfo;
export type DetectedCode = import("./live-scanner.js").DetectedCode;
export type Accuracy = import("./live-scanner.js").Accuracy;
export type LiveFrameResult = import("./live-scanner.js").LiveFrameResult;
export type LiveScannerOptions = import("./live-scanner.js").LiveScannerOptions;
export type ScanHint = {
    reason: "no-decode";
    /**
     * Since scanning started (or the last decode).
     */
    elapsedMs: number;
    /**
     * Other cameras the Web can use (may be empty).
     */
    cameras: CameraAlternatives;
    /**
     * Current focus state — `changing: true` means AF is still hunting.
     */
    focus: FocusInfo;
};
export type FrameResultMsg = import("./pipeline-worker.js").FrameResultMsg;
export type PipelineReply = {
    type: "ready";
    kind?: string;
    symbologies?: string[];
} | {
    type: "error";
    message: string;
} | FrameResultMsg;
export type ScannerOptions = LiveScannerOptions & {
    binarizer?: import("./gpu/binarizer.js").BinarizerOptions;
    onFrame?: (frame: import("./gpu/binarizer.js").BinarizedFrame) => void;
    onHint?: (hint: ScanHint) => void;
    suggestAfterMs?: number;
    worker?: boolean;
    pipelineWorker?: URL | string | Worker;
    onStats?: (stats: ScannerStats) => void;
};
export type ScannerStats = {
    fps: number;
    frameMs: number;
    frameW: number;
    frameH: number;
    zoom: number | null;
    /**
     * Frames skipped while the pipeline was busy or maxFps throttled.
     */
    dropped: number;
};
import { LiveScanner } from "./live-scanner.js";
