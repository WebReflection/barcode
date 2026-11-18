/**
 * Camera access with lens awareness.
 *
 * Ground truth (verified against iOS 16.3+ / Android Chrome behavior):
 * - iOS exposes every rear lens via enumerateDevices() since 16.3, but
 *   labels are LOCALIZED ("Back Camera" / "Rückkamera") — never pattern
 *   match on exact names, only on lens-type keywords, and treat order as
 *   a heuristic, not a guarantee.
 * - Android Chrome exposes individual rear lenses as separate devices.
 * - Both platforms expose `zoom` on the track; on iPhones the default
 *   back "camera" may be a virtual device whose zoom range starts at
 *   0.5 (= ultra-wide field of view). Adaptive zoom can therefore reach
 *   the wide lens without switching devices.
 */
/** @typedef {"ultrawide" | "wide" | "default"} LensPreference */
/** @typedef {Object} CameraLens
 * @property {string} deviceId
 * @property {string} label
 * @property {string} groupId
 * @property {"ultrawide" | "wide" | "tele" | "unknown"} kind Best-effort classification; `unknown` means label gave no hint.
 */
/** @typedef {Object} CameraAlternatives
 * @property {{ deviceId: string, label: string, kind: string, focusDistance: { min: number, max: number } | null }[]} cameras
 * @property {string} currentDeviceId
 */
/** @typedef {Object} OpenCameraOptions
 * @property {LensPreference} [lens]
 * @property {string} [deviceId]
 * @property {number} [idealWidth] Requested capture size. Phones may return anything.
 * @property {number} [idealHeight]
 * @property {number} [zoom] Capability-capped camera zoom factor (1 = none).
 * @property {boolean} [torch] Strobe light, where the track supports it.
 */
/** @typedef {MediaTrackCapabilities & { zoom?: { min: number, max: number, step?: number }, torch?: boolean, focusMode?: string[], focusDistance?: { min: number, max: number, step?: number }, pointsOfInterest?: boolean }} TrackCaps */
/** Get a permission-granting stream first; labels are empty before that. */
export function ensurePermission(): Promise<void>;
/** @returns {Promise<MediaDeviceInfo[]>} */
export function listVideoInputs(): Promise<MediaDeviceInfo[]>;
/** @returns {Promise<CameraLens[]>} */
export function listBackLenses(): Promise<CameraLens[]>;
/**
 * Open the rear camera. Resolution: prefer explicit deviceId, then the
 * lens keyword heuristic, then facingMode (ideal, not exact — iOS Safari
 * 16.4+ returns the virtual camera for facingMode, which is fine as a
 * fallback since adaptive zoom reaches every lens through it).
 * @param {OpenCameraOptions} [opts]
 * @returns {Promise<MediaStream>}
 */
export function openCamera(opts?: OpenCameraOptions): Promise<MediaStream>;
/** @param {MediaStreamTrack} track @returns {TrackCaps} */
export function trackCapabilities(track: MediaStreamTrack): TrackCaps;
/**
 * The DOM lib's `MediaTrackConstraintSet` predates the camera
 * capability keys every target browser exposes (zoom, torch,
 * focusMode); this is the honest widening for them.
 * @typedef {MediaTrackConstraintSet & {
 *   zoom?: number,
 *   torch?: boolean,
 *   focusMode?: string,
 *   focusDistance?: number,
 * }} CameraTrackConstraint
 */
/**
 * Zoom via applyConstraints, clamped to the lens' real range.
 * @param {MediaStreamTrack} track
 * @param {number} factor
 * @returns {Promise<number | null>} the applied value, or null if unsupported.
 */
export function setZoom(track: MediaStreamTrack, factor: number): Promise<number | null>;
/** @param {MediaStreamTrack} track @returns {number | null} */
export function currentZoom(track: MediaStreamTrack): number | null;
/** @param {MediaStreamTrack} track @param {boolean} on @returns {Promise<boolean>} */
export function setTorch(track: MediaStreamTrack, on: boolean): Promise<boolean>;
/**
 * Switch to manual focus at a fixed distance (meters). Barcode scanning at
 * a known working distance benefits hugely: continuous AF hunts forever on
 * low-contrast targets (a barcode is exactly that).
 * @param {MediaStreamTrack} track
 * @param {number} meters
 * @returns {Promise<boolean>} false if the lens cannot focus manually.
 */
export function setFocusDistance(track: MediaStreamTrack, meters: number): Promise<boolean>;
/** Resume continuous autofocus. @param {MediaStreamTrack} track @returns {Promise<boolean>} */
export function setFocusContinuous(track: MediaStreamTrack): Promise<boolean>;
/** @typedef {Object} FocusInfo
 * @property {string | null} mode
 * @property {number | null} distance Current focus distance in meters.
 * @property {{ min: number, max: number } | null} range
 * @property {boolean} changing True while AF is hunting (distance still moving).
 */
/**
 * Snapshot the focus state; `changing` is derived by comparing two samples.
 * @param {MediaStreamTrack} track
 * @returns {Promise<FocusInfo>}
 */
export function focusInfo(track: MediaStreamTrack): Promise<FocusInfo>;
/**
 * Wait until the lens stops refocusing (AF settle after zoom/lens changes),
 * or ~2s elapse.
 * @param {MediaStreamTrack} track
 * @returns {Promise<void>}
 */
export function waitForFocusSettle(track: MediaStreamTrack): Promise<void>;
export type LensPreference = "ultrawide" | "wide" | "default";
export type CameraLens = {
    deviceId: string;
    label: string;
    groupId: string;
    /**
     * Best-effort classification; `unknown` means label gave no hint.
     */
    kind: "ultrawide" | "wide" | "tele" | "unknown";
};
export type CameraAlternatives = {
    cameras: {
        deviceId: string;
        label: string;
        kind: string;
        focusDistance: {
            min: number;
            max: number;
        } | null;
    }[];
    currentDeviceId: string;
};
export type OpenCameraOptions = {
    lens?: LensPreference | undefined;
    deviceId?: string | undefined;
    /**
     * Requested capture size. Phones may return anything.
     */
    idealWidth?: number | undefined;
    idealHeight?: number | undefined;
    /**
     * Capability-capped camera zoom factor (1 = none).
     */
    zoom?: number | undefined;
    /**
     * Strobe light, where the track supports it.
     */
    torch?: boolean | undefined;
};
export type TrackCaps = MediaTrackCapabilities & {
    zoom?: {
        min: number;
        max: number;
        step?: number;
    };
    torch?: boolean;
    focusMode?: string[];
    focusDistance?: {
        min: number;
        max: number;
        step?: number;
    };
    pointsOfInterest?: boolean;
};
/**
 * The DOM lib's `MediaTrackConstraintSet` predates the camera
 * capability keys every target browser exposes (zoom, torch,
 * focusMode); this is the honest widening for them.
 */
export type CameraTrackConstraint = MediaTrackConstraintSet & {
    zoom?: number;
    torch?: boolean;
    focusMode?: string;
    focusDistance?: number;
};
export type FocusInfo = {
    mode: string | null;
    /**
     * Current focus distance in meters.
     */
    distance: number | null;
    range: {
        min: number;
        max: number;
    } | null;
    /**
     * True while AF is hunting (distance still moving).
     */
    changing: boolean;
};
