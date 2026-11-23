/**
 * Scanner: full dev/diagnostic live scanner extending `LiveScanner` with
 * the internal pipeline worker (`src/pipeline-worker.js`), mask observer,
 * frame stats, camera-switch hints, lens switching, and focus controls.
 *
 * The published SDK (`api/main.js`) imports `LiveScanner` directly so
 * `dist/main.js` never bundles unused internal-worker or diagnostic paths.
 */

import {
  currentZoom,
  focusInfo,
  listBackLenses,
  openCamera,
  setFocusContinuous,
  setFocusDistance,
  setTorch,
  waitForFocusSettle,
} from "./camera.js";
import { LiveScanner } from "./live-scanner.js";

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
  /** @type {ScannerOptions & { suggestAfterMs: number }} */
  #scannerOpts;
  /** @type {number} */ #fpsEMA = 0;
  /** @type {number} */ #lastHintAt = 0;
  /** @type {number} */ #scannedSince = 0;
  /** @type {Worker | null} */ #worker = null;
  /** Last value applied to the track (null = untouched). */
  /** @type {boolean | null} */ #torchApplied = null;
  /**
   * The single in-flight frame request, or null.
   * @type {{ promise: Promise<FrameResultMsg>, resolve: (m: FrameResultMsg) => void, reject: (e: Error) => void, timer: number } | null}
   */ #pending = null;
  /** Which binarizer the pipeline worker actually got ("gpu" | "cpu"). */
  #binarizerKind = "init";
  /** The symbology list the loaded wasm reports (set on worker ready). */
  /** @type {string[]} */ #symbologies = [];
  /** Frames skipped while busy / above maxFps — the serial pipeline's drop count. */
  #dropped = 0;

  /** @param {ScannerOptions} opts */
  constructor(opts) {
    super(opts);
    this.#scannerOpts = /** @type {ScannerOptions & { suggestAfterMs: number }} */ ({
      suggestAfterMs: 3000,
      ...opts,
    });
  }

  /**
   * What this Scanner instance is actually running, for feature detection
   * and diagnostics.
   * @returns {{ binarizer: string, symbologies: string[], formats: string[] | null }}
   */
  get capabilities() {
    return {
      binarizer: this.#binarizerKind,
      symbologies: [...this.#symbologies],
      formats: this._opts.formats ?? null,
    };
  }

  /** Which binarizer the pipeline worker actually got: "gpu", "cpu" or "init". */
  get binarizerKind() {
    return this.#binarizerKind;
  }

  /** @protected @override */
  async _onStart() {
    if (!this._opts.decode) {
      if (this.#scannerOpts.worker === false) {
        throw new Error("The scanner requires the pipeline worker (worker: false is not supported).");
      }
      // Resume after suspend(): the worker (and its binarizer + wasm) is
      // still alive. Options are fixed for the instance's lifetime.
      if (!this.#worker) {
        // NOTE: the worker path is kept in a variable on purpose — a literal
        // first argument would make esbuild emit pipeline-worker.js as an asset.
        const pipelineWorkerPath = "./pipeline-worker.js";
        const custom = this.#scannerOpts.pipelineWorker;
        this.#worker =
          custom instanceof Worker
            ? custom
            : new Worker(new URL(custom ?? pipelineWorkerPath, import.meta.url), { type: "module" });
        try {
          await this.#initWorker();
        } catch (err) {
          this.#worker = null;
          throw err;
        }
      }
    }
    this.#scannedSince = performance.now();
    this.#lastHintAt = 0;
  }

  /** @protected @override */
  _onDrop() {
    this.#dropped++;
  }

  /** @protected @override @param {HTMLVideoElement} video @returns {Promise<LiveFrameResult>} */
  _decode(video) {
    return this._opts.decode ? super._decode(video) : this.#decodeInWorker(video);
  }

  /** @protected @override @param {LiveFrameResult} msg */
  async _onFrame(msg) {
    if (msg.mask && this.#scannerOpts.onFrame) {
      const frame = /** @type {import("./gpu/binarizer.js").BinarizedFrame} */ ({
        width: msg.frameW,
        height: msg.frameH,
        data: new Uint8Array(msg.mask),
        bytesPerRow: msg.bytesPerRow,
      });
      await this.#scannerOpts.onFrame(frame);
    }
  }

  /** @protected @override @param {LiveFrameResult} msg @param {number} t0 */
  _onTick(msg, t0) {
    this.#maybeHint();
    const dt = performance.now() - t0;
    this.#fpsEMA =
      this.#fpsEMA === 0 ? 1000 / dt : 0.9 * this.#fpsEMA + 0.1 * (1000 / dt);
    this.#scannerOpts.onStats?.({
      fps: this.#fpsEMA,
      frameMs: dt,
      frameW: msg.frameW ?? 0,
      frameH: msg.frameH ?? 0,
      zoom: this.zoomFactor,
      dropped: this.#dropped,
    });
  }

  /**
   * Fire the no-decode hint: frames are flowing, nothing decoded for
   * `suggestAfterMs`, and other cameras actually exist. Cooldown 10s.
   */
  #maybeHint() {
    const window = this.#scannerOpts.suggestAfterMs;
    if (!window || !this.#scannerOpts.onHint) return;
    const now = performance.now();
    if (now - this.#lastHintAt < 10000) return;
    const since = this._lastDecodeAt || this.#scannedSince;
    if (now - since < window) return;
    this.#lastHintAt = now;
    void (async () => {
      const focus = await focusInfo(this._track());
      const track = this._track();
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
      this.#scannerOpts.onHint?.({
        reason: "no-decode",
        elapsedMs: now - since,
        cameras: { cameras, currentDeviceId: current },
        focus,
      });
    })();
  }

  /**
   * The torch's last-applied state: true/false after an explicit or
   * automated set, null while untouched. Lets UI reflect "auto" vs "on".
   * @returns {boolean | null}
   */
  get torchApplied() {
    return this.#torchApplied;
  }

  /** @protected @override @param {MediaStreamTrack} track @param {boolean} on */
  async _applyTorch(track, on) {
    const ok = await super._applyTorch(track, on);
    if (ok) this.#torchApplied = on;
    return ok;
  }

  #initWorker() {
    const w = /** @type {Worker} */ (this.#worker);
    const { promise, resolve, reject } = Promise.withResolvers();
    const to = setTimeout(() => reject(new Error("worker init timeout")), 8000);
    w.addEventListener("message", (/** @type {MessageEvent} */ ev) => {
      const m = /** @type {PipelineReply} */ (ev.data);
      if (m?.type === "ready") {
        clearTimeout(to);
        this.#binarizerKind = /** @type {string} */ (m.kind ?? "unknown");
        this.#symbologies = /** @type {string[]} */ (Array.isArray(m.symbologies) ? m.symbologies : []);
        const active = /** @type {string[] | undefined} */ (this._opts.formats);
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
      opts: { ...(this.#scannerOpts.binarizer ?? {}), accuracy: this._opts.accuracy },
    });
    return promise;
  }

  /**
   * Transfer one video frame into the pipeline worker and await the decode result.
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
    return this._grab(video).then(
      (bitmap) => {
        const hint = this._lastGrab;
        w.postMessage(
          {
            type: "frame",
            bitmap,
            opts: {
              ...(this.#scannerOpts.binarizer ?? {}),
              formats: hint?.format ? [hint.format] : this._opts.formats,
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

  /** @param {boolean} on @returns {Promise<boolean>} */
  async setTorch(on) {
    return this._running ? setTorch(this._track(), on) : false;
  }

  /** The camera's current zoom factor, or null when the track has none. */
  get zoom() {
    return this._running ? currentZoom(this._track()) : null;
  }

  /**
   * Set the zoom factor, optionally waiting for autofocus to settle on
   * native track zoom.
   *
   * @override
   * @param {number} factor
   * @param {boolean} [settle]
   * @returns {Promise<number | null>}
   */
  async zoomTo(factor, settle = true) {
    const applied = await super.zoomTo(factor);
    if (applied !== null && settle && this._nativeZoom) {
      await waitForFocusSettle(this._track());
    }
    return applied;
  }

  /**
   * Switch lens at runtime; keeps the GPU pipeline and pump alive.
   * Waits for the lens to settle focus before returning.
   * @param {LensPreference} lens
   */
  async setLens(lens) {
    this._opts.lens = lens;
    if (!this._running) return;
    this._stream?.getTracks().forEach((t) => t.stop());
    this._stream = await openCamera({ lens, ...this._opts.camera });
    this._opts.video.srcObject = this._stream;
    await this._opts.video.play();
    await waitForFocusSettle(this._track());
  }

  /**
   * Lock focus at a fixed distance (meters).
   * @param {number} meters
   * @returns {Promise<boolean>}
   */
  async setFocusDistance(meters) {
    return this._running ? setFocusDistance(this._track(), meters) : false;
  }

  /** Resume continuous autofocus. @returns {Promise<boolean>} */
  async setFocusContinuous() {
    return this._running ? setFocusContinuous(this._track()) : false;
  }

  /** Focus snapshot (mode, distance, whether AF is still hunting). @returns {Promise<FocusInfo>} */
  async focusInfo() {
    return focusInfo(this._track());
  }

  /** @returns {Promise<CameraLens[]>} */
  async lenses() {
    return listBackLenses();
  }

  /**
   * Stop scanning — camera closed, pump halted, pending frame rejected —
   * but KEEP the pipeline worker (binarizer + wasm) alive.
   */
  async suspend() {
    await super.stop();
    if (this.#pending) {
      clearTimeout(this.#pending.timer);
      const p = this.#pending;
      this.#pending = null;
      p.reject(new Error("scanner stopped"));
    }
  }

  /** Full teardown: `suspend()` plus terminating the pipeline worker. @override */
  async stop() {
    await this.suspend();
    this.#worker?.terminate();
    this.#worker = null;
  }
}
