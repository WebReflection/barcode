/**
 * Scanner: owns camera lifecycle, the GPU binarizer, the frame pump and
 * the adaptive-zoom policy. Decoders plug in via `onFrame` (a WASM module
 * lands here next); results come back through `onResults`.
 */

import {
  currentZoom,
  focusInfo,
  listBackLenses,
  openCamera,
  setFocusContinuous,
  setFocusDistance,
  setTorch,
  setZoom,
  trackCapabilities,
  waitForFocusSettle,
} from "./camera.js";
import { familyOf } from "./live-schedule.js";

/** @typedef {import("./camera.js").CameraLens} CameraLens */
/** @typedef {import("./camera.js").LensPreference} LensPreference */
/** @typedef {import("./camera.js").TrackCaps} TrackCaps */
/** @typedef {import("./camera.js").CameraAlternatives} CameraAlternatives */
/** @typedef {import("./camera.js").FocusInfo} FocusInfo */

/** @typedef {Object} DetectedCode
 * @property {string} text
 * @property {Uint8Array} raw Exact payload bytes (binary QR byte mode etc.).
 * @property {string} symbology
 * @property {Array<{x: number, y: number}>} position Corner quad in frame pixel coordinates.
 */

/** @typedef {Object} ScanHint
 * @property {"no-decode"} reason
 * @property {number} elapsedMs Since scanning started (or the last decode).
 * @property {CameraAlternatives} cameras Other cameras the Web can use (may be empty).
 * @property {FocusInfo} focus Current focus state — `changing: true` means AF is still hunting.
 */

/** Decode-effort level, shared with the worker protocol. */
/** @typedef {"low" | "auto" | "high"} Accuracy */

/** @typedef {import("./pipeline-worker.js").FrameResultMsg} FrameResultMsg */

/** The pipeline worker's reply union. */
/** @typedef {{ type: "ready", kind?: string, symbologies?: string[] } | { type: "error", message: string } | FrameResultMsg} PipelineReply */

/** @typedef {Object} ScannerOptions
 * @property {HTMLVideoElement} video
 * @property {LensPreference} [lens]
 * @property {{ zoom?: number, torch?: boolean }} [camera]
 *   Capability-gated track constraints applied after the camera opens:
 *   `zoom` (optical/digital range from getCapabilities) and `torch`
 *   (strobe — lights the scene from the phone, killing self-shadowing).
 * @property {import("./gpu/binarizer.js").BinarizerOptions} [binarizer]
 * @property {string[]} [formats] Decode-tier hint: only look for these
 *   symbologies. Names are the normalized lowercase forms results carry
 *   ("qrcode", "ean13", "code128", ...; "qr" is an alias). Omit to scan
 *   for everything. Unknown names reject `start()` — typos must fail
 *   loudly, not scan nothing. This is a *decode-tier* selection only: it
 *   never changes capture behavior (lens/zoom/torch are camera options).
 * @property {(frame: import("./gpu/binarizer.js").BinarizedFrame) => void} [onFrame]
 * @property {(bitmap: ImageBitmap, formats?: string[], accuracy?: Accuracy) => Promise<{results: DetectedCode[], error?: string, elapsedMs?: number, qrPresence?: number, partial?: boolean, checked?: string[] | null}>} [decode]
 *   Injected decoder — used by the published artifact (exactly 3 files:
 *   main.js, worker.js, wasm.wasm; no fourth pipeline file). When absent,
 *   the internal pipeline worker is spawned (dev/demo default).
 *   Observer: fires per frame with the binarized mask (transferred back
 *   from the worker, zero-copy) — for debug drawing and instrumentation.
 *   Decoding itself happens inside the worker; results arrive via
 *   `onResults`.
 * @property {(results: DetectedCode[]) => void} [onResults]
 * @property {(err: Error) => void} [onError]
 *   Fired once when 3 consecutive frames fail to decode — the pipeline
 *   itself is broken (dead worker/binding), not the scene. The scanner
 *   stops itself right after; confirmation state dies with it.
 * @property {(hint: ScanHint) => void} [onHint]
 *   Fired when frames flow but nothing decodes for `suggestAfterMs` — and
 *   only when other cameras exist. Use it to suggest switching cameras.
 * @property {number} [suggestAfterMs] Default 3000; 0 disables hints.
 * @property {number} [maxFps] Target pump rate; the camera still delivers at its own rate.
 * @property {Accuracy} [accuracy] Default "auto". "high" always sweeps a
 *   second Bradley threshold per frame (glare AND shadow variants);
 *   "auto" sweeps only when a pass finds nothing; "low" never does.
 *   Confirmation itself is the same two-frame state machine in all modes.
 * @property {number} [budget] Default 500 (ms). Hard per-frame decode
 *   ceiling for live scanning — the confirmation law's clock: a frame
 *   whose wall time exceeds it is discarded (and invalidates pending
 *   evidence) *unless it carries candidates*, which still count — slow
 *   hardware + slow symbologies must stay readable. Pass 5000 for
 *   still-image scanning. Must match the worker's own decode budget,
 *   which is calibrated to the same value.
 * @property {"square" | "full"} [crop] Default "full". "square" grabs the
 *   center square of each frame: a portrait phone stream would otherwise
 *   binarize a tall thin mask that crushes small codes (a micro QR ends
 *   up with a handful of pixels). The default UI pairs this with a square
 *   viewfinder — what the user sees is exactly what decodes.
 * @property {"ultrawide" | "wide" | "default"} [lens] Camera lens
 *   preference, best-effort classification from the back lenses.
 * @property {boolean} [adaptiveZoom] Grow zoom when a detected code occupies too little of the frame,
 *   and fall back to native 1x a few seconds after decoding stops. Stands
 *   down entirely when `camera.zoom` is set explicitly (never fight the user).
 * @property {boolean} [autoTorch] Default true. Turn the torch on while a QR
 *   is present but failing to decode (marginal contrast — the field-proven
 *   fix), and off again when the scene is empty. If the frame carries no QR
 *   signal (linear-only formats), fall back to a no-decode timer. A manual
 *   `overrideTorch()` stands the automation down for the session.
 * @property {boolean} [worker] Run binarization + decoding in a pipeline
 *   worker (default true). Frames are transferred as ImageBitmaps and the
 *   mask is transferred back — zero-copy, no shared buffers, no special
 *   headers. `start()` rejects if the worker cannot start (workers are a
 *   hard requirement: every browser that can run this SDK has them).
 * @property {URL | string | Worker} [pipelineWorker] The pipeline worker's
 *   module URL, or an already-created Worker. Default:
 *   "./pipeline-worker.js" next to this module (dev/demo). The demo's
 *   production build passes a bundled URL, because the literal path is
 *   deliberately kept in a variable: emitting it would make esbuild add a
 *   fourth asset to the published main.js.
 * @property {(stats: ScannerStats) => void} [onStats]
 */

/** @typedef {Object} ScannerStats
 * @property {number} fps
 * @property {number} frameMs
 * @property {number} frameW
 * @property {number} frameH
 * @property {number | null} zoom
 * @property {number} dropped Frames skipped while the pipeline was busy or maxFps throttled.
 */

/** Decode-result symbology ("DataBar Stacked") → canonical format name. */
const FORMAT_OF = /** @type {Record<string, string>} */ ({
  qrcode: "qrcode", microqr: "microqr", microqrcode: "microqr",
  datamatrix: "datamatrix", aztec: "aztec", pdf417: "pdf417",
  ean13: "ean13", ean8: "ean8", upca: "upca", upce: "upce",
  code39: "code39", code93: "code93", code128: "code128", itf: "itf",
  codabar: "codabar", databar: "databar", databarltd: "databarltd",
  databarstacked: "databarstk", databarstk: "databarstk",
});

/**
 * Symbologies expensive enough to deserve a close-up re-confirmation.
 * The linear tier scans the full frame in ~1ms, and a 1D quad is a thin
 * horizontal band: cropping to it can cut the bar height, so the
 * re-confirmation never lands (a white-on-black Code128 measured this).
 */
const HINTED = new Set(["QRCode", "QRCode Micro", "Data Matrix", "DataMatrix", "Aztec", "PDF417"]);

const ZOOM_TARGET_FRACTION = 0.4; // want the code >= 40% of frame width
const ZOOM_COOLDOWN_MS = 1200;
// Auto-framing policy constants. Field-proven: torch is the contrast lever
// (fixes dim scenes AND self-shadowing); digital zoom adds no detail on
// sensor-crop cameras, so it exists only for framing stability and is
// reset to native once the interaction is over.
const TORCH_AFTER_MARGINAL_MS = 1200; // code present but not decoding -> light it
const TORCH_FALLBACK_MS = 5000; // no decode (no QR signal available) -> light it
const TORCH_OFF_AFTER_CLEAR_MS = 2000; // scene empty again -> battery back
const TORCH_MIN_TOGGLE_MS = 1000; // anti-flap
const ZOOM_RESET_AFTER_MS = 3000; // interaction over -> back to native 1x

export class Scanner {
  /** @type {ScannerOptions & { maxFps: number, adaptiveZoom: boolean }} */
  #opts;
  /** @type {MediaStream | null} */ #stream = null;
  /** @type {boolean} */ #running = false;
  /** @type {boolean} */ #busy = false;
  /** One pump loop per instance: start() after suspend() must not add one. */
  /** @type {boolean} */ #pumping = false;
  /** @type {number} */ #lastTs = 0;
  /** @type {number} */ #lastZoomAt = 0;
  /** @type {number} */ #fpsEMA = 0;
  /** @type {number} */ #lastDecodeAt = 0;
  /** @type {number} */ #lastHintAt = 0;
  /** @type {number} */ #scannedSince = 0;
  /** @type {Worker | null} */ #worker = null;
  /** Auto-framing state: the torch automation stands down once the user touches it. */
  #torchTouched = false;
  /** @type {boolean} */ #torchAuto = false;
  /** Last value applied to the track (null = untouched). */
  /** @type {boolean | null} */ #torchApplied = null;
  /** @type {number} */ #torchLastToggleAt = 0;
  /** @type {number} */ #marginalSince = 0;
  /** @type {number} */ #clearSince = 0;
  /** @type {number} */ #noDecodeSince = 0;
  /**
   * The single in-flight frame request, or null. Frames are strictly
   * serial: the pump only sends when the previous frame has answered, so
   * no id map is needed — one slot with Promise.withResolvers() is enough.
   * (If decode ever goes concurrent — e.g. multi-worker — switch to
   * @webreflection/utils next-resolver, which is this same pattern with
   * an id map.)
   * @type {{ promise: Promise<FrameResultMsg>, resolve: (m: FrameResultMsg) => void, reject: (e: Error) => void, timer: number } | null}
   */ #pending = null;
  /** Which binarizer the pipeline worker actually got ("gpu" | "cpu"). */
  #binarizerKind = "init";
  /** The symbology list the loaded wasm reports (set on worker ready). */
  /** @type {string[]} */ #symbologies = [];
  // --- Confirmation state machine (main thread counts and approves; the
  // worker validates and returns details — the architecture split). Every
  // distinct value (symbology+text) accumulates its own consecutive-frame
  // streak; a value missing from a frame is dropped, and a frame slower
  // than `budget` is discarded AND invalidates all streaks — unless it
  // carries candidates, which still count (slow hardware + slow
  // symbologies must stay readable; accuracy is the 2-frame law's job).
  // The first value to reach 2 consecutive frames wins — highest ranked
  // (the worker sorts centered-first) when several reach 2 in the same
  // frame. No timers: the ~1s fire latency emerges from the 500ms cap.
  /** Per-value consecutive-frame streaks ("symbology\0text" → count). */
  /** @type {Map<string, number>} */ #streaks = new Map();
  /** Consecutive failed frames (decoder throws) — 3 in a row is fatal. */
  #failStreak = 0;
  /** Frames skipped while busy / above maxFps — the serial pipeline's drop count. */
  #dropped = 0;
  /**
   * Re-confirmation hint from the previous first read: subsequent frames
   * are cropped to the code's quad and decode ONLY its symbology. "Scan
   * twice" means counter-validate what was read — not re-run the whole
   * stack for everything that was not.
   * @type {{ x: number, y: number, w: number, h: number, format: string | null, frames: number } | null}
   */
  #hint = null;
  /** The hint the most recent grab used (for position mapping). */
  /** @type {{ x: number, y: number, w: number, h: number, format: string | null, frames: number } | null} */
  #lastGrab = null;

  /** @param {ScannerOptions} opts */
  constructor(opts) {
    this.#opts = /** @type {ScannerOptions & { maxFps: number, adaptiveZoom: boolean }} */ ({ maxFps: 30, adaptiveZoom: true, suggestAfterMs: 3000, accuracy: "auto", budget: 500, ...opts });
    // Validate `formats` early: a typo should reject start(), not silently
    // scan nothing (the wasm layer re-validates and throws per frame).
    if (this.#opts.formats != null) {
      if (
        !Array.isArray(this.#opts.formats) ||
        this.#opts.formats.some((f) => typeof f !== "string")
      ) {
        throw new Error("formats must be an array of symbology name strings");
      }
      this.#opts.formats = this.#opts.formats.map((f) => f.trim().toLowerCase());
    }
  }

  /**
   * What this Scanner instance is actually running, for feature detection
   * and diagnostics: the binarizer tier the pipeline got, the symbology
   * list the loaded wasm decodes, and the active formats filter (null =
   * everything). GPU-only features (perspective, once built) belong here.
   * @returns {{ binarizer: string, symbologies: string[], formats: string[] | null }}
   */
  get capabilities() {
    return {
      binarizer: this.#binarizerKind,
      symbologies: [...this.#symbologies],
      formats: this.#opts.formats ?? null,
    };
  }

  /** Which binarizer the pipeline worker actually got: "gpu", "cpu" or "init". */
  get binarizerKind() {
    return this.#binarizerKind;
  }

  async start() {
    if (this.#running) return;
    const video = this.#opts.video;
    this.#stream = await openCamera({ lens: this.#opts.lens, ...this.#opts.camera });
    video.srcObject = this.#stream;
    video.muted = true;
    video.playsInline = true;
    await video.play();

    // The GPU binarizer + WASM live in a dedicated worker; frames are
    // transferred as ImageBitmaps (zero-copy) and the mask is transferred
    // back. Workers are a hard requirement, not an optimization: they
    // keep the camera preview and UI free of decode work.
    if (this.#opts.decode) {
      // Decode is injected (published artifact): nothing to handshake.
    } else {
      if (this.#opts.worker === false) {
        throw new Error("The scanner requires the pipeline worker (worker: false is not supported).");
      }
      // Resume after suspend(): the worker (and its binarizer + wasm) is
      // still alive. Options are fixed for the instance's lifetime.
      if (!this.#worker) {
        // NOTE: the worker path is kept in a variable on purpose — a literal
        // first argument would make esbuild emit pipeline-worker.js as a
        // fourth asset when bundling the published main.js.
        const pipelineWorkerPath = "./pipeline-worker.js";
        const custom = this.#opts.pipelineWorker;
        this.#worker =
          custom instanceof Worker
            ? custom
            : new Worker(new URL(custom ?? pipelineWorkerPath, import.meta.url), { type: "module" });
        try {
          await this.#initWorker();
        } catch (err) {
          // Don't leak the camera on a failed handshake.
          this.#stream.getTracks().forEach((t) => t.stop());
          this.#stream = null;
          this.#worker = null;
          throw err;
        }
      }
    }
    this.#running = true;
    this.#lastTs = performance.now();
    this.#scannedSince = performance.now();
    this.#lastDecodeAt = 0;
    this.#lastHintAt = 0;
    if (!this.#pumping) {
      this.#pumping = true;
      void this.#pump();
    }
  }

  async #pump() {
    // A suspended/stopped scanner has no pump: the next start() begins a new
    // one. (The loop is per instance; it used to be per Scanner object.)
    if (!this.#running) {
      this.#pumping = false;
      return;
    }
    const video = this.#opts.video;

    // Schedule the next tick first: a slow decode must not stall the pump.
    // requestVideoFrameCallback (all modern browsers) fires per camera frame;
    // setTimeout(33ms) is the fallback.
    if ("requestVideoFrameCallback" in video) {
      (/** @type {HTMLVideoElement & { requestVideoFrameCallback: (cb: () => void) => number }} */ (video)).requestVideoFrameCallback(() => void this.#pump());
    } else {
      setTimeout(() => void this.#pump(), 33);
    }

    if (this.#busy) {
      this.#dropped++;
      return;
    }
    if (video.videoWidth === 0) return;
    const minInterval = 1000 / this.#opts.maxFps;
    if (performance.now() - this.#lastTs < minInterval) {
      this.#dropped++;
      return;
    }
    this.#busy = true;

    const t0 = performance.now();
    try {
      const msg = this.#opts.decode
        ? await this.#decodeInjected(video)
        : await this.#decodeInWorker(video);
      if (msg.error) throw new Error(msg.error);
      /** @type {DetectedCode[]} */
      const raw = msg.results;
      // The confirmation law: wall time around the await decides whether
      // this frame counts. Over-budget frames are discarded AND
      // invalidate pending — "main counts and approves, worker validates
      // and returns details". The adaptive policies below still see raw.
      const wallMs = performance.now() - t0;
      // If this frame was a hinted close-up, map its results back to full
      // frame coordinates before anything consumes them.
      this.#mapFromHint(raw, msg);
      const results = this.#confirm(raw, wallMs > (this.#opts.budget ?? 500), msg.checked ?? (msg.partial === true ? [] : null));
      this.#updateHint(raw, results, msg);      // The mask is transferred back (zero-copy) for observers.
      /** @type {import("./gpu/binarizer.js").BinarizedFrame | null} */
      let frame = null;
      if (msg.mask) {
        frame = /** @type {import("./gpu/binarizer.js").BinarizedFrame} */ ({
          width: msg.frameW,
          height: msg.frameH,
          data: new Uint8Array(msg.mask),
          bytesPerRow: msg.bytesPerRow,
        });
        await this.#opts.onFrame?.(frame);
      }
      if (raw.length) this.#lastDecodeAt = performance.now();
      this.#failStreak = 0;
      if (results.length) {
        this.#opts.onResults?.(results);
      }
      if (this.#opts.adaptiveZoom && frame) void this.#maybeZoom(raw, frame);
      if (this.#opts.adaptiveZoom && frame) void this.#maybeZoomReset(raw, frame);
      void this.#autoTorchPolicy(msg, raw.length > 0);
      this.#maybeHint();
      const dt = performance.now() - t0;
      this.#fpsEMA =
        this.#fpsEMA === 0 ? 1000 / dt : 0.9 * this.#fpsEMA + 0.1 * (1000 / dt);
      this.#opts.onStats?.({
        fps: this.#fpsEMA,
        frameMs: dt,
        frameW: frame?.width ?? 0,
        frameH: frame?.height ?? 0,
        zoom: currentZoom(this.#track()),
        dropped: this.#dropped,
      });
    } catch (err) {
      // Transient frame failures retry; 3 consecutive mean the pipeline
      // itself is broken (dead worker, dead binding) — surface it instead
      // of spinning forever. The confirmation state dies with it.
      console.error("frame failed:", err);
      if (++this.#failStreak >= 3) {
        this.#streaks.clear();
        this.#opts.onError?.(/** @type {Error} */ (err));
        this.stop();
      }
    } finally {
      this.#lastTs = performance.now();
      this.#busy = false;
    }
  }

  /**
   * Fire the no-decode hint: frames are flowing, nothing decoded for
   * `suggestAfterMs`, and other cameras actually exist. Cooldown 10s.
   */
  #maybeHint() {
    const window = this.#opts.suggestAfterMs;
    if (!window || !this.#opts.onHint) return;
    const now = performance.now();
    if (now - this.#lastHintAt < 10000) return;
    const since = this.#lastDecodeAt || this.#scannedSince;
    if (now - since < window) return;
    this.#lastHintAt = now;
    void (async () => {
      const focus = await focusInfo(this.#track());
      const track = this.#track();
      const current = /** @type {MediaTrackSettings & { deviceId?: string }} */ (track.getSettings()).deviceId ?? "";
      const lenses = await listBackLenses();
      const cameras = lenses
        .filter((l) => l.deviceId !== current)
        .map((l) => ({
          deviceId: l.deviceId,
          label: l.label,
          kind: l.kind,
          focusDistance: /** @type {{ min: number, max: number } | null} */ (null),
        }));
      this.#opts.onHint?.({
        reason: "no-decode",
        elapsedMs: now - since,
        cameras: { cameras, currentDeviceId: current },
        focus,
      });
    })();
  }

  /**
   * If a code was found but is small in frame, zoom in to raise decode odds.
   * @param {DetectedCode[]} results
   * @param {{ width: number, height: number }} frame Mask dimensions.
   */
  /**
   * Auto-framing torch policy. The signal hierarchy:
   *  - QR presence 1 (finders found, decode failed): the code is THERE and
   *    marginal — light it after a short confirmation window.
   *  - QR presence 0 (no finders): scene empty — battery back.
   *  - No signal (linear-only formats): fall back to a no-decode timer.
   * Any manual overrideTorch() call stands the automation down for the
   * session; the user wins, always.
   * @param {FrameResultMsg} msg Raw frameResult from the worker (carries qrPresence).
   * @param {boolean} decoded
   */
  async #autoTorchPolicy(msg, decoded) {
    if (this.#opts.autoTorch === false || this.#torchTouched || !this.#running) return;
    const track = this.#track();
    const caps = trackCapabilities(track);
    if (!caps.torch) return;
    const now = performance.now();
    if (now - this.#torchLastToggleAt < TORCH_MIN_TOGGLE_MS) return;

    /** @type {number | undefined} */
    const signal = msg?.qrPresence;
    /** @type {boolean | null} */
    let want = null;
    if (typeof signal === "number") {
      if (signal === 1) {
        if (!this.#marginalSince) this.#marginalSince = now;
        want = now - this.#marginalSince >= TORCH_AFTER_MARGINAL_MS ? true : null;
      } else {
        this.#marginalSince = 0;
      }
      if (signal === 0) {
        if (!this.#clearSince) this.#clearSince = now;
        if (this.#torchAuto && now - this.#clearSince >= TORCH_OFF_AFTER_CLEAR_MS) want = false;
      } else {
        this.#clearSince = 0;
      }
    } else {
      // No QR tier this frame: time-based fallback.
      if (!decoded) {
        if (!this.#noDecodeSince) this.#noDecodeSince = now;
        want = now - this.#noDecodeSince >= TORCH_FALLBACK_MS ? true : null;
      } else {
        this.#noDecodeSince = 0;
        want = false;
      }
    }
    // A successful decode always clears the marginal timer.
    if (decoded) this.#marginalSince = 0;

    if (want == null || want === this.#torchAuto) return;
    this.#torchLastToggleAt = now;
    this.#torchAuto = want;
    await this.#applyTorch(track, want);
  }

  /**
   * The torch's last-applied state: true/false after an explicit or
   * automated set, null while untouched. Lets UI reflect "auto" vs "on".
   * @returns {boolean | null}
   */
  get torchApplied() {
    return this.#torchApplied;
  }

  async #applyTorch(/** @type {MediaStreamTrack} */ track, /** @type {boolean} */ on) {
    const ok = await setTorch(track, on);
    if (ok) this.#torchApplied = on;
    return ok;
  }

  /**
   * Manual torch override: sets the torch AND stands the automation down
   * for the session — the user wins, always.
   * @param {boolean} on
   * @returns {Promise<boolean>}
   */
  async overrideTorch(on) {
    this.#torchTouched = true;
    this.#torchAuto = false;
    return this.#running ? this.#applyTorch(this.#track(), on) : false;
  }

  /**
   * Zoom reset: after the interaction ends (no decodes for a while), fall
   * back to native 1x so the next code can be found wide. Never runs when
   * the user set camera.zoom explicitly.
   * @param {DetectedCode[]} results
   * @param {import("./gpu/binarizer.js").BinarizedFrame} frame
   */
  async #maybeZoomReset(results, frame) {
    void frame;
    if (results.length) return;
    if (this.#opts.camera?.zoom != null) return;
    const now = performance.now();
    if (now - this.#lastZoomAt < ZOOM_COOLDOWN_MS) return;
    if (!this.#lastDecodeAt || now - this.#lastDecodeAt < ZOOM_RESET_AFTER_MS) return;
    const track = this.#track();
    const caps = trackCapabilities(track);
    if (!caps.zoom) return;
    const zoomCaps = /** @type {NonNullable<TrackCaps["zoom"]>} */ (caps.zoom);
    const z = currentZoom(track) ?? zoomCaps.min;
    if (z <= zoomCaps.min * 1.02) return;
    this.#lastZoomAt = now;
    await setZoom(track, zoomCaps.min);
  }

  /**
   * Zoom in toward a decoded-but-small code: keep growing until the code
   * fills the target fraction of the frame (framing stability — see the
   * field findings on why zoom is never for detail).
   * @param {DetectedCode[]} results
   * @param {import("./gpu/binarizer.js").BinarizedFrame} frame
   */
  async #maybeZoom(results, frame) {
    if (!results.length) return;
    // Never fight an explicit camera.zoom from the options.
    if (this.#opts.camera?.zoom != null) return;
    const now = performance.now();
    if (now - this.#lastZoomAt < ZOOM_COOLDOWN_MS) return;
    const track = this.#track();
    const caps = trackCapabilities(track);
    if (!caps.zoom) return;

    let maxSide = 0;
    for (const r of results) {
      const xs = r.position.map((p) => p.x);
      const ys = r.position.map((p) => p.y);
      maxSide = Math.max(
        maxSide,
        Math.max(...xs) - Math.min(...xs),
        Math.max(...ys) - Math.min(...ys),
      );
    }
    const fraction = maxSide / frame.width;
    if (fraction >= ZOOM_TARGET_FRACTION) return;

    const zoomCaps = /** @type {NonNullable<TrackCaps["zoom"]>} */ (caps.zoom);
    const z = currentZoom(track) ?? zoomCaps.min;
    const target = Math.min(zoomCaps.max, z * (ZOOM_TARGET_FRACTION / fraction));
    if (target <= z * 1.05) return;
    this.#lastZoomAt = now;
    await setZoom(track, target);
  }

  /** Wire the worker handshake; resolves when the pipeline is ready. */
  /**
   * The confirmation state machine — the final policy (notes/STATUS.md),
   * generalized to per-value streaks: every distinct symbology+value
   * seen accumulates its own 2-consecutive-frame evidence, each reset
   * independently when it misses a frame; a frame slower than the
   * budget is discarded AND invalidates all streaks. Returns the single
   * winner — the best-ranked value that has reached 2 — the frame it
   * first does, and every frame after while it keeps agreeing.
   * @param {DetectedCode[]} results ranked best-first by the worker
   * @param {boolean} slow true when the frame's wall time exceeded the budget
   * @param {string[] | null} [checked] the families this frame actually
   *   decoded: null = everything requested ran (a full frame); an array =
   *   the scheduled subset that ran (a cheap frame). Values from families
   *   that ran count normally — presence adds, absence clears — while
   *   values from skipped families are frozen: their absence is no
   *   evidence (the skipped tier may still hold them). `[]` therefore
   *   freezes every streak, and `null` observes everything.
   * @returns {DetectedCode[]}
   */
  #confirm(results, slow, checked = null) {
    // Result symbologies (Rust debug names) and `checked` (canonical
    // family names) share one vocabulary: "QRCodeMicro" -> "microqr",
    // "DataBarStacked" -> "databarstk", "DataBarLimited" -> "databarltd".
    const ran = checked == null ? null : new Set(checked.map(familyOf));
    const keyOf = (/** @type {DetectedCode} */ r) => r.symbology + "\u0000" + r.text;
    if (slow && results.length === 0) {
      // Over-budget frames carry stale evidence — but only when they
      // found nothing: discard them and invalidate the streaks whose
      // family ran. A skipped family keeps its evidence (same rule the
      // normal path uses); a full frame (`ran == null`) clears all. A
      // slow frame CARRYING candidates still counts — a symbology that
      // costs seconds on slow hardware would otherwise never confirm
      // (the frame is stale by its own decode time, and the 2-frame law
      // keeps the accuracy guarantee).
      if (ran == null) this.#streaks.clear();
      else {
        for (const key of this.#streaks.keys()) {
          if (ran.has(familyOf(key.split("\u0000")[0]))) this.#streaks.delete(key);
        }
      }
      return [];
    }
    const byKey = new Map(results.map((r) => [keyOf(r), r]));
    // missing values lose their streak — but only when their family
    // actually ran this frame; a value whose family was skipped keeps it
    for (const key of this.#streaks.keys()) {
      if (byKey.has(key)) continue;
      if (ran == null || ran.has(familyOf(key.split("\u0000")[0]))) this.#streaks.delete(key);
    }
    // present values count the frame when their family ran
    for (const [key] of byKey) {
      if (ran != null && !ran.has(familyOf(key.split("\u0000")[0]))) continue;
      this.#streaks.set(key, (this.#streaks.get(key) ?? 0) + 1);
    }
    // results are ranked (centered first): the first value to have
    // earned 2 consecutive frames wins the race
    const winner = results.find((r) => (this.#streaks.get(keyOf(r)) ?? 0) >= 2);
    return winner ? [winner] : [];
  }

  /**
   * Frame grab honoring `crop: "square"` — the center square of the
   * video. A portrait phone stream otherwise binarizes a tall thin
   * mask that crushes small codes (a micro QR ends up with a handful
   * of pixels after the maxSize downscale); the square keeps the
   * pixel density where the user is aiming, and matches the default
   * UI's square viewfinder — what they frame is what decodes.
   * @param {HTMLVideoElement} video
   */
  /**
   * Map positions decoded from a hinted close-up back to full-frame
   * pixels. The binarizer pads small sources 1:1 (mask px = crop px) and
   * downscales only sources larger than maxSize.
   * @param {DetectedCode[]} results
   * @param {{ frameW?: number, frameH?: number }} msg
   */
  #mapFromHint(results, msg) {
    const grab = this.#lastGrab;
    if (!grab || !results.length) return;
    const maskW = msg.frameW ?? 0;
    const maskH = msg.frameH ?? 0;
    if (!maskW || !maskH) return;
    const sx = grab.w <= maskW ? 1 : grab.w / maskW;
    const sy = grab.h <= maskH ? 1 : grab.h / maskH;
    for (const r of results) {
      if (!r.position?.length) continue;
      r.position = r.position.map((/** @type {{x: number, y: number}} */ p) => ({
        x: grab.x + p.x * sx,
        y: grab.y + p.y * sy,
      }));
    }
  }

  /**
   * Seed a re-confirmation hint after the first read of a code (the 50%
   * progress frame): the next few frames crop to its quad and decode only
   * its symbology. Cleared on confirmation or when the frames run out.
   * @param {DetectedCode[]} raw
   * @param {DetectedCode[]} results
   * @param {{ frameW?: number, frameH?: number }} msg
   */
  #updateHint(raw, results, msg) {
    if (results.length) {
      this.#hint = null;
      return;
    }
    if (this.#hint || !raw.length) return;
    const r = raw[0];
    if (!r) return;
    // Only the expensive 2D families deserve a close-up (see HINTED).
    if (!HINTED.has(String(r.symbology))) return;
    const video = this.#opts.video;
    const vw = video.videoWidth;
    const vh = video.videoHeight;
    const maskW = msg.frameW ?? 0;
    const maskH = msg.frameH ?? 0;
    if (!vw || !vh || !maskW || !maskH || !r.position?.length) return;
    const square = this.#opts.crop === "square";
    const side = square ? Math.min(vw, vh) : 0;
    const offX = square ? (vw - side) / 2 : 0;
    const offY = square ? (vh - side) / 2 : 0;
    const srcW = square ? side : vw;
    const srcH = square ? side : vh;
    const sx = srcW / maskW;
    const sy = srcH / maskH;
    const xs = r.position.map((/** @type {{x: number}} */ p) => p.x * sx + offX);
    const ys = r.position.map((/** @type {{y: number}} */ p) => p.y * sy + offY);
    let x = Math.min(...xs);
    let y = Math.min(...ys);
    let w = Math.max(...xs) - x;
    let h = Math.max(...ys) - y;
    if (!(w > 0 && h > 0)) return;
    // Margin for handheld drift between frames.
    const mx = Math.max(16, w * 0.25);
    const my = Math.max(16, h * 0.25);
    x = Math.max(0, Math.round(x - mx));
    y = Math.max(0, Math.round(y - my));
    w = Math.min(vw - x, Math.round(w + 2 * mx));
    h = Math.min(vh - y, Math.round(h + 2 * my));
    if (w < 32 || h < 16) return;
    const name = String(r.symbology).toLowerCase().replace(/[^a-z0-9]/g, "");
    this.#hint = { x, y, w, h, format: FORMAT_OF[name] ?? null, frames: 5 };
  }

  /**
   * @param {HTMLVideoElement} video
   * @returns {Promise<ImageBitmap>}
   */
  #grab(video) {
    this.#lastGrab = null;
    const hint = this.#hint;
    if (hint && hint.frames > 0) {
      hint.frames -= 1;
      this.#lastGrab = hint;
      if (hint.frames === 0) this.#hint = null;
      return createImageBitmap(video, hint.x, hint.y, hint.w, hint.h);
    }
    if (this.#opts.crop !== "square") return createImageBitmap(video);
    const s = Math.min(video.videoWidth, video.videoHeight);
    if (!s) return createImageBitmap(video);
    return createImageBitmap(
      video,
      (video.videoWidth - s) / 2,
      (video.videoHeight - s) / 2,
      s,
      s,
    );
  }

  /**
   * One video frame through an injected decoder (the published artifact
   * passes the bound worker's decode — same strictly-serial contract as
   * the pipeline worker: one request at a time, bitmap owned by the
   * decoder and closed after use).
   * @param {HTMLVideoElement} video
   */
  async #decodeInjected(video) {
    const bitmap = await this.#grab(video);
    try {
      const hint = this.#lastGrab;
      const res = await /** @type {NonNullable<ScannerOptions["decode"]>} */ (this.#opts.decode)(
        bitmap,
        hint?.format ? [hint.format] : this.#opts.formats,
        this.#opts.accuracy,
      );
      if (res?.error) throw new Error(res.error);
      return {
        results: res?.results ?? [],
        qrPresence: res?.qrPresence,
        partial: res?.partial === true,
        checked: res?.checked ?? null,
      };
    } finally {
      bitmap.close?.();
    }
  }

  #initWorker() {
    const w = /** @type {Worker} */ (this.#worker);
    const { promise, resolve, reject } = Promise.withResolvers();
    const to = setTimeout(() => reject(new Error("worker init timeout")), 8000);
    // One persistent listener for the whole session: the bootstrap
    // handshake resolves the init promise, decode responses resolve the
    // single in-flight frame request.
    w.addEventListener("message", (/** @type {MessageEvent} */ ev) => {
      const m = /** @type {PipelineReply} */ (ev.data);
      if (m?.type === "ready") {
        clearTimeout(to);
        this.#binarizerKind = /** @type {string} */ (m.kind ?? "unknown");
        this.#symbologies = /** @type {string[]} */ (Array.isArray(m.symbologies) ? m.symbologies : []);
        // A typo'd formats entry must fail start(), not spam per-frame
        // errors forever (the wasm layer would reject on every frame).
        const active = /** @type {string[] | undefined} */ (this.#opts.formats);
        if (active?.length && this.#symbologies.length) {
          const unknown = active.filter((f) => !this.#symbologies.includes(f));
          if (unknown.length) {
            reject(new Error(
              `unknown symbology ${unknown.join(", ")} — supported: ${this.#symbologies.join(", ")}`,
            ));
            return;
          }
        }
        resolve(undefined);
      } else if (m?.type === "error") {
        clearTimeout(to);
        reject(new Error(m.message));
      } else if (m?.type === "frameResult") {
        const p = this.#pending;
        if (p) {
          clearTimeout(p.timer);
          this.#pending = null;
          p.resolve(m);
        }
      }
    });
    w.addEventListener("error", (/** @type {ErrorEvent} */ ev) => {
      clearTimeout(to);
      reject(new Error(ev.message ?? "worker error"));
    });
    w.postMessage({
      type: "init",
      opts: { ...(this.#opts.binarizer ?? {}), accuracy: this.#opts.accuracy },
    });
    return promise;
  }

  /**
   * Transfer one video frame into the worker and await the decode result.
   * The ImageBitmap handoff is a zero-copy transfer — the worker owns it
   * and closes it after use. The pump only calls this when the previous
   * frame has answered (strictly serial, one request at a time).
   * @param {HTMLVideoElement} video
   */
  #decodeInWorker(video) {
    if (this.#pending) return Promise.reject(new Error("frame already in flight"));
    const w = /** @type {Worker} */ (this.#worker);
    const { promise, resolve, reject } = Promise.withResolvers();
    /** @type {{ promise: Promise<FrameResultMsg>, resolve: (m: FrameResultMsg) => void, reject: (e: Error) => void, timer: number }} */
    const pending = { promise, resolve, reject, timer: 0 };
    pending.timer = setTimeout(() => {
      this.#pending = null;
      reject(new Error("worker decode timeout"));
    }, 5000);
    this.#pending = pending;
    return this.#grab(video).then(
      (bitmap) => {
        const hint = this.#lastGrab;
        w.postMessage(
          {
            type: "frame",
            bitmap,
            opts: {
              ...(this.#opts.binarizer ?? {}),
              formats: hint?.format ? [hint.format] : this.#opts.formats,
            },
          },
          [bitmap],
        );
        return promise;
      },
      (/** @type {Error} */ err) => {
        clearTimeout(pending.timer);
        this.#pending = null;
        throw err;
      },
    );
  }

  /** @returns {MediaStreamTrack} */
  #track() {
    const t = this.#stream?.getVideoTracks()[0];
    if (!t) throw new Error("Scanner not started.");
    return t;
  }

  hasTorch() {
    return this.#running ? trackCapabilities(this.#track()).torch === true : false;
  }

  /** @param {boolean} on @returns {Promise<boolean>} */
  async setTorch(on) {
    return this.#running ? setTorch(this.#track(), on) : false;
  }

  /** @param {number} factor @returns {Promise<number | null>} */
  async zoomTo(factor) {
    const r = this.#running ? await setZoom(this.#track(), factor) : null;
    if (r !== null && this.#running) await waitForFocusSettle(this.#track());
    return r;
  }

  /**
   * Switch lens at runtime; keeps the GPU pipeline and pump alive.
   * Waits for the lens to settle focus before returning.
   * @param {LensPreference} lens
   */
  async setLens(lens) {
    this.#opts.lens = lens;
    if (!this.#running) return;
    this.#stream?.getTracks().forEach((t) => t.stop());
    this.#stream = await openCamera({ lens, ...this.#opts.camera });
    this.#opts.video.srcObject = this.#stream;
    await this.#opts.video.play();
    await waitForFocusSettle(this.#track());
  }

  /**
   * Lock focus at a fixed distance (meters) — the reliable way to scan a
   * known working distance: continuous AF hunts on low-contrast targets.
   * @param {number} meters
   * @returns {Promise<boolean>}
   */
  async setFocusDistance(meters) {
    return this.#running ? setFocusDistance(this.#track(), meters) : false;
  }

  /** Resume continuous autofocus. @returns {Promise<boolean>} */
  async setFocusContinuous() {
    return this.#running ? setFocusContinuous(this.#track()) : false;
  }

  /** Focus snapshot (mode, distance, whether AF is still hunting). @returns {Promise<FocusInfo>} */
  async focusInfo() {
    return focusInfo(this.#track());
  }

  /** @returns {Promise<CameraLens[]>} */
  async lenses() {
    return listBackLenses();
  }

  /**
   * Stop scanning — camera closed, pump halted, pending frame rejected —
   * but KEEP the pipeline worker (binarizer + wasm) alive. The next
   * `start()` reuses it, so a repeat scan in a long-lived UI pays only the
   * camera reopen, not a worker spawn + wasm init. Use `stop()` for the
   * full teardown. Instance options (binarizer/formats) are fixed for the
   * worker's lifetime, so reuse the same Scanner.
   */
  async suspend() {
    this.#running = false;
    this.#stream?.getTracks().forEach((t) => t.stop());
    this.#stream = null;
    this.#opts.video.srcObject = null;
    if (this.#pending) {
      clearTimeout(this.#pending.timer);
      const p = this.#pending;
      this.#pending = null;
      p.reject(new Error("scanner stopped"));
    }
    // Confirmation state dies with the session: a restart must not
    // inherit streaks from the previous run.
    this.#streaks.clear();
    this.#hint = null;
    this.#lastGrab = null;
    this.#failStreak = 0;
  }

  /** Full teardown: `suspend()` plus terminating the pipeline worker. */
  async stop() {
    await this.suspend();
    this.#worker?.terminate();
    this.#worker = null;
  }
}
