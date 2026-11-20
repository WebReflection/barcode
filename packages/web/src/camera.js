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
export async function ensurePermission() {
  // Prefer the environment camera: this call is usually a side effect of
  // needing device LABELS (Android Chrome enumerates one blank placeholder
  // until a stream has been opened), and opening the selfie camera just to
  // learn the labels is a visible, unnecessary flash.
  const s = await navigator.mediaDevices.getUserMedia({
    video: { facingMode: { ideal: "environment" } },
    audio: false,
  });
  s.getTracks().forEach((t) => t.stop());
}

/** @returns {Promise<MediaDeviceInfo[]>} */
export async function listVideoInputs() {
  const devices = await navigator.mediaDevices.enumerateDevices();
  return devices.filter((d) => d.kind === "videoinput");
}

/** @returns {Promise<CameraLens[]>} */
export async function listBackLenses() {
  let inputs = await listVideoInputs();
  // A freshly granted permission (or a page that has never opened a
  // stream) enumerates a single placeholder with an EMPTY deviceId/label.
  // Picking that as `{ exact: "" }` is an OverconstrainedError, so
  // exercise the permission once and re-enumerate.
  if (inputs.some((d) => !d.deviceId)) {
    try {
      await ensurePermission();
    } catch {
      // openCamera will surface the real permission error.
    }
    inputs = await listVideoInputs();
    // The permission probe just stopped a camera; Android releases it
    // asynchronously, so the first real open needs a beat.
    await sleep(150);
  }
  /** @type {CameraLens[]} */
  const lenses = [];
  for (const d of inputs) {
    if (!d.deviceId) continue;
    const label = d.label.toLowerCase();
    const isFront =
      /front|frontkamera|kamera vorne|vorwärts|selfie|facetime|前|前置/.test(label);
    if (isFront) continue;
    lenses.push({
      deviceId: d.deviceId,
      label: d.label,
      groupId: d.groupId,
      kind: classifyLens(label),
    });
  }
  return lenses;
}

/**
 * @param {string} label
 * @returns {CameraLens["kind"]}
 */
function classifyLens(label) {
  if (/ultra|ultrawide|0[.,]5x|0[.,]5\b/.test(label)) return "ultrawide";
  if (/tele|zoom|2x|3x|5x/.test(label)) return "tele";
  if (/wide|haupt|main/.test(label)) return "wide";
  // Android Chrome labels every rear lens "camera N, facing back": no
  // wide/ultra hint at all. Keep it unknown so the capability probe
  // decides instead of trusting enumeration order.
  if (/facing back|facing rear/.test(label)) return "unknown";
  if (/back|rear|rück/.test(label)) return "wide";
  return "unknown";
}

/** Probe result cache: one short stream per device for the page lifetime. */
/** @type {Map<string, TrackCaps | null>} */
const probeCache = new Map();

/** Android releases a camera asynchronously: give it a beat between opens. */
const sleep = (/** @type {number} */ ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Read a lens' capabilities without committing to it. Android labels do
 * not say which rear device is the main/close-focus lens and
 * `facingMode: environment` is not reliable across WebViews (a WebView
 * test app picked its fixed-focus ultra-wide here), so the only honest
 * signal is the track itself.
 * @param {string} deviceId
 * @returns {Promise<TrackCaps | null>}
 */
async function probeTrackCaps(deviceId) {
  if (probeCache.has(deviceId)) return probeCache.get(deviceId) ?? null;
  /** @type {TrackCaps | null} */
  let caps = null;
  // A previous camera may still be releasing (NotReadableError). One
  // spaced retry turns a transient release race into a real answer.
  for (let attempt = 0; attempt < 2 && !caps; attempt++) {
    if (attempt) await sleep(300);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        video: { deviceId: { exact: deviceId }, width: { ideal: 320 }, height: { ideal: 240 } },
        audio: false,
      });
      const track = stream.getVideoTracks()[0];
      caps = track ? trackCapabilities(track) : null;
      stream.getTracks().forEach((t) => t.stop());
    } catch {
      // A busy or forbidden device: retry once, then let the caller fall back.
    }
  }
  probeCache.set(deviceId, caps);
  return caps;
}

/** Lower `camera N` labels first; labels without a number last. */
function labelIndex(/** @type {CameraLens} */ lens) {
  const index = Number(/\bcamera\s+(\d+)/i.exec(lens.label)?.[1]);
  return Number.isFinite(index) ? index : 99;
}

/**
 * Score a rear lens for *scanning*: continuous autofocus and a close
 * focus range put pixels on a barcode held at arm's length. A camera
 * that only shoots landscapes (fixed focus, no continuous AF) is the
 * wrong default even when the platform calls it "environment".
 * @param {CameraLens} lens
 * @param {TrackCaps | null} caps
 */
function scanScore(lens, caps) {
  if (!caps) return -1;
  let score = 0;
  if (caps.focusMode?.includes("continuous")) score += 1000;
  const range = caps.focusDistance;
  if (range) {
    // Meters. A wide range covers both a close barcode and a shelf.
    score += Math.min(20, Math.max(0, range.max - range.min)) * 25;
  }
  if (caps.zoom) score += 200;
  const index = labelIndex(lens);
  if (index < 99) score += 20 / (index + 1);
  return score;
}

/**
 * Pick the rear lens worth scanning with.
 * - `ultrawide` → the classified ultra-wide when the label says so.
 * - `wide` / `default` → the best scanning lens by capability probe;
 *   ambiguous labels never fall back to enumeration order alone.
 * @param {CameraLens[]} lenses
 * @param {LensPreference} want
 * @returns {Promise<CameraLens | null>}
 */
async function pickBackLens(lenses, want) {
  const ultra = lenses.filter((l) => l.kind === "ultrawide");
  if (want === "ultrawide") return ultra[0] ?? null;
  const candidates = lenses.filter((l) => l.kind !== "ultrawide");
  if (!candidates.length) return ultra[0] ?? null;
  if (candidates.length === 1) return candidates[0] ?? null;
  // Sequential, lowest label index first: two concurrent getUserMedia
  // calls for different rear cameras race on Android, and the camera
  // that wins that race is not the one that wins the scan.
  const ranked = [...candidates].sort((a, b) => labelIndex(a) - labelIndex(b));
  /** @type {{ lens: CameraLens, score: number } | null} */
  let best = null;
  for (const lens of ranked) {
    const score = scanScore(lens, await probeTrackCaps(lens.deviceId));
    if (!best || score > best.score) best = { lens, score };
    await sleep(150);
  }
  return best?.lens ?? ranked[0] ?? null;
}

/**
 * Open the rear camera. Resolution: prefer explicit deviceId, then the
 * best rear lens for scanning (continuous AF + close focus, capability
 * probed), then facingMode as the last resort.
 * @param {OpenCameraOptions} [opts]
 * @returns {Promise<MediaStream>}
 */
export async function openCamera(opts = {}) {
  const { idealWidth = 1920, idealHeight = 1080 } = opts;
  /** @type {MediaTrackConstraints} */
  const video = {
    width: { ideal: idealWidth },
    height: { ideal: idealHeight },
  };

  if (opts.deviceId) {
    video.deviceId = { exact: opts.deviceId };
  } else {
    const lenses = await listBackLenses();
    const pick = await pickBackLens(lenses, opts.lens === "ultrawide" ? "ultrawide" : "wide");
    if (pick) {
      video.deviceId = { exact: pick.deviceId };
    } else {
      // Last resort: the platform's environment camera (iOS Safari 16.4+
      // may return the virtual camera; adaptive zoom reaches the others).
      video.facingMode = { ideal: "environment" };
    }
  }

  // The capability probe stopped its stream a moment ago; Android may
  // still be releasing it. Retry a transient NotReadableError instead
  // of failing the scan for a camera that is about to be free.
  /** @type {MediaStream} */
  let stream;
  for (let attempt = 0; ; attempt++) {
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video, audio: false });
      break;
    } catch (err) {
      const name = err instanceof DOMException ? err.name : "";
      if (attempt >= 2 || (name !== "NotReadableError" && name !== "AbortError")) throw err;
      await sleep(300);
    }
  }

  // Zoom / torch are capability-gated track constraints (Android Chrome
  // and most native-camera-backed browsers expose both; desktop webcams
  // usually neither). Applied best-effort: absent support is not an
  // error — the scanner just runs at 1x unlit.
  const track = stream.getVideoTracks()[0];
  if (track && typeof track.applyConstraints === "function") {
    const caps = /** @type {TrackCaps} */ (track.getCapabilities?.() ?? {});
    /** @type {MediaTrackConstraintSet[]} */
    const advanced = [];
    if (opts.zoom != null && caps.zoom) {
      const z = Math.min(Math.max(opts.zoom, caps.zoom.min), caps.zoom.max);
      advanced.push(/** @type {MediaTrackConstraintSet} */ ({ zoom: z }));
    }
    if (opts.torch != null && caps.torch) {
      advanced.push(/** @type {MediaTrackConstraintSet} */ ({ torch: opts.torch }));
    }
    if (advanced.length) {
      try {
        await track.applyConstraints({ advanced });
      } catch {
        // Some browsers accept the capability but reject the combined
        // constraint set; try each on its own before giving up.
        for (const c of advanced) {
          try { await track.applyConstraints({ advanced: [c] }); } catch { /* best effort */ }
        }
      }
    }
  }
  return stream;
}

/** @param {MediaStreamTrack} track @returns {TrackCaps} */
export function trackCapabilities(track) {
  return /** @type {TrackCaps} */ (track.getCapabilities?.() ?? {});
}

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
export async function setZoom(track, factor) {
  const caps = trackCapabilities(track);
  if (!caps.zoom) return null;
  const z = Math.min(caps.zoom.max, Math.max(caps.zoom.min, factor));
  await track.applyConstraints({ advanced: [/** @type {CameraTrackConstraint} */ ({ zoom: z })] });
  return z;
}

/** @param {MediaStreamTrack} track @returns {number | null} */
export function currentZoom(track) {
  const s = /** @type {MediaTrackSettings & { zoom?: number }} */ (track.getSettings());
  return s.zoom ?? null;
}

/** @param {MediaStreamTrack} track @param {boolean} on @returns {Promise<boolean>} */
export async function setTorch(track, on) {
  const caps = trackCapabilities(track);
  if (!caps.torch) return false;
  await track.applyConstraints({ advanced: [/** @type {CameraTrackConstraint} */ ({ torch: on })] });
  return true;
}

/**
 * Switch to manual focus at a fixed distance (meters). Barcode scanning at
 * a known working distance benefits hugely: continuous AF hunts forever on
 * low-contrast targets (a barcode is exactly that).
 * @param {MediaStreamTrack} track
 * @param {number} meters
 * @returns {Promise<boolean>} false if the lens cannot focus manually.
 */
export async function setFocusDistance(track, meters) {
  const caps = trackCapabilities(track);
  if (!caps.focusDistance) return false;
  const fd = caps.focusDistance;
  const d = Math.min(fd.max, Math.max(fd.min, meters));
  try {
    await track.applyConstraints({
      advanced: [/** @type {CameraTrackConstraint} */ ({ focusMode: "manual", focusDistance: d })],
    });
    return true;
  } catch {
    return false;
  }
}

/** Resume continuous autofocus. @param {MediaStreamTrack} track @returns {Promise<boolean>} */
export async function setFocusContinuous(track) {
  const caps = trackCapabilities(track);
  if (!caps.focusMode || !caps.focusMode.includes("continuous")) return false;
  try {
    await track.applyConstraints({ advanced: [/** @type {CameraTrackConstraint} */ ({ focusMode: "continuous" })] });
    return true;
  } catch {
    return false;
  }
}

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
export async function focusInfo(track) {
  const caps = trackCapabilities(track);
  const s = /** @type {MediaTrackSettings & { focusMode?: string, focusDistance?: number }} */ (track.getSettings());
  const d1 = s.focusDistance ?? null;
  await new Promise((r) => setTimeout(r, 150));
  const s2 = /** @type {MediaTrackSettings & { focusDistance?: number }} */ (track.getSettings());
  const d2 = s2.focusDistance ?? null;
  return {
    mode: s.focusMode ?? null,
    distance: d1,
    range: caps.focusDistance ? { min: caps.focusDistance.min, max: caps.focusDistance.max } : null,
    changing: d1 !== null && d2 !== null && Math.abs(d2 - d1) > 0.005,
  };
}

/**
 * Wait until the lens stops refocusing (AF settle after zoom/lens changes),
 * or ~2s elapse.
 * @param {MediaStreamTrack} track
 * @returns {Promise<void>}
 */
export async function waitForFocusSettle(track) {
  const deadline = performance.now() + 2000;
  let prev = null;
  while (performance.now() < deadline) {
    const s = /** @type {MediaTrackSettings & { focusDistance?: number }} */ (track.getSettings());
    const d = s.focusDistance ?? null;
    if (prev !== null && d !== null && Math.abs(d - prev) < 0.005 && prev !== 0) return;
    prev = d;
    await new Promise((r) => setTimeout(r, 150));
  }
}
