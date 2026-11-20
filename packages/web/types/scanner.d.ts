export class Scanner {
    /** @param {ScannerOptions} opts */
    constructor(opts: ScannerOptions);
    /**
     * What this Scanner instance is actually running, for feature detection
     * and diagnostics: the binarizer tier the pipeline got, the symbology
     * list the loaded wasm decodes, and the active formats filter (null =
     * everything). GPU-only features (perspective, once built) belong here.
     * @returns {{ binarizer: string, symbologies: string[], formats: string[] | null }}
     */
    get capabilities(): {
        binarizer: string;
        symbologies: string[];
        formats: string[] | null;
    };
    /** Which binarizer the pipeline worker actually got: "gpu", "cpu" or "init". */
    get binarizerKind(): string;
    start(): Promise<void>;
    /**
     * The torch's last-applied state: true/false after an explicit or
     * automated set, null while untouched. Lets UI reflect "auto" vs "on".
     * @returns {boolean | null}
     */
    get torchApplied(): boolean | null;
    /**
     * Manual torch override: sets the torch AND stands the automation down
     * for the session — the user wins, always.
     * @param {boolean} on
     * @returns {Promise<boolean>}
     */
    overrideTorch(on: boolean): Promise<boolean>;
    hasTorch(): boolean;
    /** Whether the camera track exposes its own zoom constraint. */
    hasZoom(): boolean;
    /**
     * The effective zoom: the camera constraint when supported, otherwise
     * the digital center crop. Never null — the default UI's pinch and
     * slider both drive this.
     */
    get zoomFactor(): number;
    /** @param {boolean} on @returns {Promise<boolean>} */
    setTorch(on: boolean): Promise<boolean>;
    /** The camera's current zoom factor, or null when the track has none. */
    get zoom(): number | null;
    /**
     * Set the zoom factor. Uses the camera track's own `zoom` constraint
     * when it has one; otherwise crops the center of the source (1–4x) so
     * the binarizer keeps the code's modules instead of downscaling them
     * with the whole frame. A manual zoom stands the adaptive policy down
     * for the session — the user wins, always.
     *
     * `settle` waits for autofocus to stop moving after the change (the
     * programmatic default). Continuous gestures pass false: a 2s settle
     * wait per pointermove would lag behind the fingers.
     *
     * @param {number} factor
     * @param {boolean} [settle]
     * @returns {Promise<number | null>} the applied track value, the
     *   digital factor, or null when the scanner is not running.
     */
    zoomTo(factor: number, settle?: boolean): Promise<number | null>;
    /**
     * Switch lens at runtime; keeps the GPU pipeline and pump alive.
     * Waits for the lens to settle focus before returning.
     * @param {LensPreference} lens
     */
    setLens(lens: LensPreference): Promise<void>;
    /**
     * Lock focus at a fixed distance (meters) — the reliable way to scan a
     * known working distance: continuous AF hunts on low-contrast targets.
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
     * but KEEP the pipeline worker (binarizer + wasm) alive. The next
     * `start()` reuses it, so a repeat scan in a long-lived UI pays only the
     * camera reopen, not a worker spawn + wasm init. Use `stop()` for the
     * full teardown. Instance options (binarizer/formats) are fixed for the
     * worker's lifetime, so reuse the same Scanner.
     */
    suspend(): Promise<void>;
    /** Full teardown: `suspend()` plus terminating the pipeline worker. */
    stop(): Promise<void>;
    #private;
}
export type CameraLens = import("./camera.js").CameraLens;
export type LensPreference = import("./camera.js").LensPreference;
export type TrackCaps = import("./camera.js").TrackCaps;
export type CameraAlternatives = import("./camera.js").CameraAlternatives;
export type FocusInfo = import("./camera.js").FocusInfo;
export type DetectedCode = {
    text: string;
    /**
     * Exact payload bytes (binary QR byte mode etc.).
     */
    raw: Uint8Array;
    /**
     * Canonical lowercase name ("qrcode", "ean13", ...).
     */
    symbology: string;
    /**
     * Corner quad in frame pixel coordinates.
     */
    position: Array<{
        x: number;
        y: number;
    }>;
};
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
export type Accuracy = "low" | "auto" | "high";
export type FrameResultMsg = import("./pipeline-worker.js").FrameResultMsg;
export type PipelineReply = {
    type: "ready";
    kind?: string;
    symbologies?: string[];
} | {
    type: "error";
    message: string;
} | FrameResultMsg;
export type ScannerOptions = {
    video: HTMLVideoElement;
    lens?: import("./camera.js").LensPreference | undefined;
    /**
     * Capability-gated track constraints applied after the camera opens:
     * `zoom` (optical/digital range from getCapabilities) and `torch`
     * (strobe — lights the scene from the phone, killing self-shadowing).
     */
    camera?: {
        zoom?: number;
        torch?: boolean;
    } | undefined;
    binarizer?: import("./gpu/binarizer.js").BinarizerOptions | undefined;
    /**
     * Decode-tier hint: only look for these
     * symbologies. Names are the normalized lowercase forms results carry
     * ("qrcode", "ean13", "code128", ...; "qr" is an alias). Omit to scan
     * for everything. Unknown names reject `start()` — typos must fail
     * loudly, not scan nothing. This is a *decode-tier* selection only: it
     * never changes capture behavior (lens/zoom/torch are camera options).
     */
    formats?: string[] | undefined;
    onFrame?: ((frame: import("./gpu/binarizer.js").BinarizedFrame) => void) | undefined;
    /**
     * Injected decoder — used by the published artifact (exactly 3 files:
     * main.js, worker.js, wasm.wasm; no fourth pipeline file). When absent,
     * the internal pipeline worker is spawned (dev/demo default).
     * Observer: fires per frame with the binarized mask (transferred back
     * from the worker, zero-copy) — for debug drawing and instrumentation.
     * Decoding itself happens inside the worker; results arrive via
     * `onResults`.
     */
    decode?: ((bitmap: ImageBitmap, formats?: string[], accuracy?: Accuracy) => Promise<{
        results: DetectedCode[];
        error?: string;
        elapsedMs?: number;
        qrPresence?: number;
        partial?: boolean;
        checked?: string[] | null;
    }>) | undefined;
    onResults?: ((results: DetectedCode[]) => void) | undefined;
    /**
     * Fired once when 3 consecutive frames fail to decode — the pipeline
     * itself is broken (dead worker/binding), not the scene. The scanner
     * stops itself right after; confirmation state dies with it.
     */
    onError?: ((err: Error) => void) | undefined;
    /**
     * Fired when frames flow but nothing decodes for `suggestAfterMs` — and
     * only when other cameras exist. Use it to suggest switching cameras.
     */
    onHint?: ((hint: ScanHint) => void) | undefined;
    /**
     * Default 3000; 0 disables hints.
     */
    suggestAfterMs?: number | undefined;
    /**
     * Target pump rate; the camera still delivers at its own rate.
     */
    maxFps?: number | undefined;
    /**
     * Default "auto". "high" always sweeps a
     * second Bradley threshold per frame (glare AND shadow variants);
     * "auto" sweeps only when a pass finds nothing; "low" never does.
     * Confirmation itself is the same two-frame state machine in all modes.
     */
    accuracy?: Accuracy | undefined;
    /**
     * Default 500 (ms). Hard per-frame decode
     * ceiling for live scanning — the confirmation law's clock: a frame
     * whose wall time exceeds it is discarded (and invalidates pending
     * evidence) *unless it carries candidates*, which still count — slow
     * hardware + slow symbologies must stay readable. Pass 5000 for
     * still-image scanning. Must match the worker's own decode budget,
     * which is calibrated to the same value.
     */
    budget?: number | undefined;
    /**
     * Default "full". "square" grabs the
     * center square of each frame: a portrait phone stream would otherwise
     * binarize a tall thin mask that crushes small codes (a micro QR ends
     * up with a handful of pixels). The default UI pairs this with a square
     * viewfinder — what the user sees is exactly what decodes.
     */
    crop?: "square" | "full" | undefined;
    /**
     * Grow zoom when a detected code occupies too little of the frame,
     * and fall back to native 1x a few seconds after decoding stops. Stands
     * down entirely when `camera.zoom` is set explicitly or once the user
     * zooms manually (never fight the user).
     */
    adaptiveZoom?: boolean | undefined;
    /**
     * Default true. Turn the torch on while a QR
     * is present but failing to decode (marginal contrast — the field-proven
     * fix), and off again when the scene is empty. If the frame carries no QR
     * signal (linear-only formats), fall back to a no-decode timer. A manual
     * `overrideTorch()` stands the automation down for the session.
     */
    autoTorch?: boolean | undefined;
    /**
     * Run binarization + decoding in a pipeline
     * worker (default true). Frames are transferred as ImageBitmaps and the
     * mask is transferred back — zero-copy, no shared buffers, no special
     * headers. `start()` rejects if the worker cannot start (workers are a
     * hard requirement: every browser that can run this SDK has them).
     */
    worker?: boolean | undefined;
    /**
     * The pipeline worker's
     * module URL, or an already-created Worker. Default:
     * "./pipeline-worker.js" next to this module (dev/demo). The demo's
     * production build passes a bundled URL, because the literal path is
     * deliberately kept in a variable: emitting it would make esbuild add a
     * fourth asset to the published main.js.
     */
    pipelineWorker?: string | URL | Worker | undefined;
    onStats?: ((stats: ScannerStats) => void) | undefined;
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
