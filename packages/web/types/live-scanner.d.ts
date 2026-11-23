export class LiveScanner {
    /** @param {LiveScannerOptions} opts */
    constructor(opts: LiveScannerOptions);
    /** @protected @type {LiveScannerOptions & { maxFps: number, adaptiveZoom: boolean }} */
    protected _opts: LiveScannerOptions & {
        maxFps: number;
        adaptiveZoom: boolean;
    };
    /** @protected @type {MediaStream | null} */
    protected _stream: MediaStream | null;
    /** @protected @type {boolean} */
    protected _running: boolean;
    /** @protected @type {number} */
    protected _lastDecodeAt: number;
    /** The hint the most recent grab used (for position mapping). */
    /** @protected @type {{ x: number, y: number, w: number, h: number, format: string | null, frames: number } | null} */
    protected _lastGrab: {
        x: number;
        y: number;
        w: number;
        h: number;
        format: string | null;
        frames: number;
    } | null;
    /** Whether the last zoomTo() applied a native track zoom constraint. */
    /** @protected @type {boolean} */
    protected _nativeZoom: boolean;
    /** Subclass hook run after the camera stream starts and plays. @protected */
    protected _onStart(): Promise<void>;
    /** Subclass hook run when a frame is dropped while busy or over maxFps. @protected */
    protected _onDrop(): void;
    /** Subclass hook run with the raw decode reply before `onResults`. @protected @param {LiveFrameResult} _msg */
    protected _onFrame(_msg: LiveFrameResult): Promise<void>;
    /** Subclass hook run at the end of each decoded frame. @protected @param {LiveFrameResult} _msg @param {number} _t0 */
    protected _onTick(_msg: LiveFrameResult, _t0: number): void;
    start(): Promise<void>;
    /** @protected @param {MediaStreamTrack} track @param {boolean} on */
    protected _applyTorch(track: MediaStreamTrack, on: boolean): Promise<boolean>;
    /**
     * Manual torch override: sets the torch AND stands the automation down
     * for the session — the user wins, always.
     * @param {boolean} on
     * @returns {Promise<boolean>}
     */
    overrideTorch(on: boolean): Promise<boolean>;
    /**
     * Grab a video frame honoring the active hint crop, `crop: "square"`,
     * and digital zoom.
     * @protected
     * @param {HTMLVideoElement} video
     * @returns {Promise<ImageBitmap>}
     */
    protected _grab(video: HTMLVideoElement): Promise<ImageBitmap>;
    /**
     * One video frame through the injected decoder (strictly serial: one
     * request at a time, bitmap owned by the decoder and closed after use).
     * @protected
     * @param {HTMLVideoElement} video
     * @returns {Promise<LiveFrameResult>}
     */
    protected _decode(video: HTMLVideoElement): Promise<LiveFrameResult>;
    /** @protected @returns {MediaStreamTrack} */
    protected _track(): MediaStreamTrack;
    hasTorch(): boolean;
    /** Whether the camera track exposes its own zoom constraint. */
    hasZoom(): boolean;
    /**
     * The effective zoom: the camera constraint when supported, otherwise
     * the digital center crop. Never null — the default UI's pinch and
     * slider both drive this.
     */
    get zoomFactor(): number;
    /**
     * Set the zoom factor. Uses the camera track's own `zoom` constraint
     * when it has one; otherwise crops the center of the source (1–4x).
     * A manual zoom stands the adaptive policy down for the session.
     *
     * @param {number} factor
     * @returns {Promise<number | null>} the applied track value, the
     *   digital factor, or null when the scanner is not running.
     */
    zoomTo(factor: number): Promise<number | null>;
    /** Stop scanning: close camera, halt pump, clear confirmation state. */
    stop(): Promise<void>;
    #private;
}
export type LensPreference = import("./camera.js").LensPreference;
export type TrackCaps = import("./camera.js").TrackCaps;
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
export type Accuracy = "low" | "auto" | "high";
export type LiveFrameResult = {
    results: DetectedCode[];
    error?: string;
    elapsedMs?: number;
    qrPresence?: number;
    partial?: boolean;
    checked?: string[] | null;
    frameW?: number;
    frameH?: number;
    bytesPerRow?: number;
    mask?: ArrayBufferLike;
};
export type LiveScannerOptions = {
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
    /**
     * Decode-tier hint: only look for these
     * symbologies. Names are the normalized lowercase forms results carry
     * ("qrcode", "ean13", "code128", ...; "qr" is an alias). Omit to scan
     * for everything.
     */
    formats?: string[] | undefined;
    /**
     * Injected decoder — used by the published artifact (frames decode
     * through the bound worker; no extra pipeline file).
     */
    decode?: ((bitmap: ImageBitmap, formats?: string[], accuracy?: Accuracy) => Promise<LiveFrameResult>) | undefined;
    onResults?: ((results: DetectedCode[]) => void) | undefined;
    /**
     * Fired once when 3 consecutive frames fail to decode — the pipeline
     * itself is broken (dead worker/binding), not the scene. The scanner
     * stops itself right after; confirmation state dies with it.
     */
    onError?: ((err: Error) => void) | undefined;
    /**
     * Target pump rate; the camera still delivers at its own rate.
     */
    maxFps?: number | undefined;
    /**
     * Default "auto".
     */
    accuracy?: Accuracy | undefined;
    /**
     * Default 500 (ms). Hard per-frame decode
     * ceiling for live scanning — the confirmation law's clock.
     */
    budget?: number | undefined;
    /**
     * Default "full". "square" grabs the
     * center square of each frame.
     */
    crop?: "square" | "full" | undefined;
    /**
     * Grow zoom when a detected code occupies too little of the frame,
     * and fall back to native 1x a few seconds after decoding stops.
     */
    adaptiveZoom?: boolean | undefined;
    /**
     * Default true. Turn the torch on while a QR
     * is present but failing to decode, and off again when the scene is empty.
     */
    autoTorch?: boolean | undefined;
};
