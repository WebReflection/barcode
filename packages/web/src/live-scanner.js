/**
 * LiveScanner: the core live-camera scanner used by the published SDK
 * (`api/main.js`). Owns the camera lifecycle, frame pump, two-frame
 * confirmation state machine, re-confirmation close-up hint, auto-torch,
 * digital/native zoom, and injected decoding.
 *
 * Dev/diagnostic features (the legacy internal pipeline worker, camera
 * lens switching, focus controls, mask observer, stats, hints) live in
 * the `Scanner` subclass (`src/scanner.js`) so `dist/main.js` never
 * bundles unused code.
 */

import {
  currentZoom,
  openCamera,
  setTorch,
  setZoom,
  trackCapabilities,
} from "./camera.js";
import { CENTERED_SCORE, PERIPHERAL_CAP } from "./rank.js";

/** @typedef {import("./camera.js").LensPreference} LensPreference */
/** @typedef {import("./camera.js").TrackCaps} TrackCaps */

/** @typedef {Object} DetectedCode
 * @property {string} text
 * @property {Uint8Array} raw Exact payload bytes (binary QR byte mode etc.).
 * @property {string} symbology Canonical lowercase name ("qrcode", "ean13", ...).
 * @property {Array<{x: number, y: number}>} position Corner quad in frame pixel coordinates.
 * @property {number} [score] Reticle priority from the worker (see src/rank.js).
 */

/** Decode-effort level, shared with the worker protocol. */
/** @typedef {"low" | "auto" | "high"} Accuracy */

/** Common frame-decode reply shape (injected worker or pipeline worker). */
/** @typedef {{
 *   results: DetectedCode[],
 *   error?: string,
 *   elapsedMs?: number,
 *   qrPresence?: number,
 *   partial?: boolean,
 *   checked?: string[] | null,
 *   frameW?: number,
 *   frameH?: number,
 *   bytesPerRow?: number,
 *   mask?: ArrayBufferLike,
 * }} LiveFrameResult */

/** @typedef {Object} LiveScannerOptions
 * @property {HTMLVideoElement} video
 * @property {LensPreference} [lens]
 * @property {{ zoom?: number, torch?: boolean }} [camera]
 *   Capability-gated track constraints applied after the camera opens:
 *   `zoom` (optical/digital range from getCapabilities) and `torch`
 *   (strobe — lights the scene from the phone, killing self-shadowing).
 * @property {string[]} [formats] Decode-tier hint: only look for these
 *   symbologies. Names are the normalized lowercase forms results carry
 *   ("qrcode", "ean13", "code128", ...; "qr" is an alias). Omit to scan
 *   for everything.
 * @property {(bitmap: ImageBitmap, formats?: string[], accuracy?: Accuracy) => Promise<LiveFrameResult>} [decode]
 *   Injected decoder — used by the published artifact (frames decode
 *   through the bound worker; no extra pipeline file).
 * @property {(results: DetectedCode[]) => void} [onResults]
 * @property {(err: Error) => void} [onError]
 *   Fired once when 3 consecutive frames fail to decode — the pipeline
 *   itself is broken (dead worker/binding), not the scene. The scanner
 *   stops itself right after; confirmation state dies with it.
 * @property {number} [maxFps] Target pump rate; the camera still delivers at its own rate.
 * @property {Accuracy} [accuracy] Default "auto".
 * @property {number} [budget] Default 500 (ms). Hard per-frame decode
 *   ceiling for live scanning — the confirmation law's clock.
 * @property {"square" | "full"} [crop] Default "full". "square" grabs the
 *   center square of each frame.
 * @property {boolean} [adaptiveZoom] Grow zoom when a detected code occupies too little of the frame,
 *   and fall back to native 1x a few seconds after decoding stops.
 * @property {boolean} [autoTorch] Default true. Turn the torch on while a QR
 *   is present but failing to decode, and off again when the scene is empty.
 */

/**
 * Symbologies expensive enough to deserve a close-up re-confirmation.
 * The linear tier scans the full frame in ~1ms, and a 1D quad is a thin
 * horizontal band: cropping to it can cut the bar height, so the
 * re-confirmation never lands (a white-on-black Code128 measured this).
 * Results carry canonical lowercase names (see `Symbology::name`).
 */
const HINTED = new Set(["qrcode", "microqr", "datamatrix", "aztec", "pdf417"]);

/**
 * Reticle test for the live policies. A caller-injected decoder (and the
 * e2e mocks) may omit `score` entirely: absence is "unknown", which must
 * be treated as aimed-at rather than peripheral, or a foreign decoder
 * would inherit the surrounding-code confirmation delay.
 * @param {DetectedCode} code
 */
const aimed = (code) => code.score == null || code.score >= CENTERED_SCORE;

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

export class LiveScanner {
  /** @protected @type {LiveScannerOptions & { maxFps: number, adaptiveZoom: boolean }} */
  _opts;
  /** @protected @type {MediaStream | null} */
  _stream = null;
  /** @protected @type {boolean} */
  _running = false;
  /** @type {boolean} */ #busy = false;
  /** One pump loop per instance: start() after stop()/suspend() must not add one. */
  /** @type {boolean} */ #pumping = false;
  /** @type {number} */ #lastTs = 0;
  /** @type {number} */ #lastZoomAt = -Infinity;
  /** @protected @type {number} */
  _lastDecodeAt = 0;
  /** Auto-framing state: the torch automation stands down once the user touches it. */
  #torchTouched = false;
  /** Same rule for zoom: a manual pinch/zoomTo() wins over adaptive zoom. */
  #zoomTouched = false;
  /** @type {boolean} */ #torchAuto = false;
  /** @type {number} */ #torchLastToggleAt = -Infinity;
  /** @type {number} */ #marginalSince = 0;
  /** @type {number} */ #clearSince = 0;
  /** @type {number} */ #noDecodeSince = 0;
  /** Per-value consecutive-frame streaks ("symbology\0text" → count). */
  /** @type {Map<string, { symbology: string, streak: number }>} */ #streaks = new Map();
  /** Consecutive failed frames (decoder throws) — 3 in a row is fatal. */
  #failStreak = 0;
  /**
   * Re-confirmation hint from the previous first read: subsequent frames
   * are cropped to the code's quad and decode ONLY its symbology.
   * @type {{ x: number, y: number, w: number, h: number, format: string | null, frames: number } | null}
   */
  #hint = null;
  /** The hint the most recent grab used (for position mapping). */
  /** @protected @type {{ x: number, y: number, w: number, h: number, format: string | null, frames: number } | null} */
  _lastGrab = null;
  /** Digital crop factor used when the track exposes no native zoom. */
  #digitalZoom = 1;
  /** Whether the last zoomTo() applied a native track zoom constraint. */
  /** @protected @type {boolean} */
  _nativeZoom = false;

  /** @param {LiveScannerOptions} opts */
  constructor(opts) {
    this._opts = /** @type {LiveScannerOptions & { maxFps: number, adaptiveZoom: boolean }} */ ({
      maxFps: 30,
      adaptiveZoom: true,
      accuracy: "auto",
      budget: 500,
      ...opts,
    });
    // Validate `formats` early: a typo should reject start(), not silently
    // scan nothing (the wasm layer re-validates and throws per frame).
    if (this._opts.formats != null) {
      if (
        !Array.isArray(this._opts.formats) ||
        this._opts.formats.some((f) => typeof f !== "string")
      ) {
        throw new Error("formats must be an array of symbology name strings");
      }
      this._opts.formats = this._opts.formats.map((f) => f.trim().toLowerCase());
    }
  }

  /** Subclass hook run after the camera stream starts and plays. @protected */
  async _onStart() {}

  /** Subclass hook run when a frame is dropped while busy or over maxFps. @protected */
  _onDrop() {}

  /** Subclass hook run with the raw decode reply before `onResults`. @protected @param {LiveFrameResult} _msg */
  async _onFrame(_msg) {}

  /** Subclass hook run at the end of each decoded frame. @protected @param {LiveFrameResult} _msg @param {number} _t0 */
  _onTick(_msg, _t0) {}

  async start() {
    if (this._running) return;
    const video = this._opts.video;
    this._stream = await openCamera({ lens: this._opts.lens, ...this._opts.camera });
    video.srcObject = this._stream;
    video.muted = true;
    video.playsInline = true;
    await video.play();
    try {
      await this._onStart();
    } catch (err) {
      // Don't leak the camera if subclass startup (e.g. worker handshake) fails.
      this._stream.getTracks().forEach((t) => t.stop());
      this._stream = null;
      video.srcObject = null;
      throw err;
    }
    this._running = true;
    this.#lastTs = performance.now();
    this._lastDecodeAt = 0;
    if (!this.#pumping) {
      this.#pumping = true;
      void this.#pump();
    }
  }

  async #pump() {
    // A stopped/suspended scanner has no pump: the next start() begins a new one.
    if (!this._running) {
      this.#pumping = false;
      return;
    }
    const video = this._opts.video;

    // Schedule the next tick first: a slow decode must not stall the pump.
    // requestVideoFrameCallback (all modern browsers) fires per camera frame;
    // setTimeout(33ms) is the fallback.
    if ("requestVideoFrameCallback" in video) {
      (/** @type {HTMLVideoElement & { requestVideoFrameCallback: (cb: () => void) => number }} */ (video)).requestVideoFrameCallback(() => void this.#pump());
    } else {
      setTimeout(() => void this.#pump(), 33);
    }

    if (this.#busy) {
      this._onDrop();
      return;
    }
    if (video.videoWidth === 0) return;
    const minInterval = 1000 / this._opts.maxFps;
    if (performance.now() - this.#lastTs < minInterval) {
      this._onDrop();
      return;
    }
    this.#busy = true;

    const t0 = performance.now();
    try {
      const msg = await this._decode(video);
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
      // A close-up that reads nothing is not worth its remaining frames.
      // The re-confirmation crop exists to speed up a code the full frame
      // already found; when the crop clips the code (a tall 2D symbol
      // inside the ±25% margin), holding the full frame back for five
      // frames makes a readable symbol unconfirmable.
      if (this._lastGrab?.format && raw.length === 0) this.#hint = null;
      const results = this.#confirm(
        raw,
        wallMs > (this._opts.budget ?? 500),
        msg.checked ?? (msg.partial === true ? [] : null),
      );
      this.#updateHint(raw, results, msg);
      await this._onFrame(msg);
      if (raw.length) this._lastDecodeAt = performance.now();
      this.#failStreak = 0;
      if (results.length) {
        this._opts.onResults?.(results);
      }
      if (this._opts.adaptiveZoom && msg.frameW && msg.frameH) {
        const refW = this._lastGrab
          ? (this._opts.crop === "square" ? Math.min(video.videoWidth, video.videoHeight) : video.videoWidth)
          : msg.frameW;
        if (refW > 0) void this.#maybeZoom(raw, refW);
        void this.#maybeZoomReset(raw);
      }
      void this.#autoTorchPolicy(msg, raw.length > 0);
      this._onTick(msg, t0);
    } catch (err) {
      // Transient frame failures retry; 3 consecutive mean the pipeline
      // itself is broken (dead worker, dead binding) — surface it instead
      // of spinning forever. The confirmation state dies with it.
      console.error("frame failed:", err);
      if (++this.#failStreak >= 3) {
        this.#streaks.clear();
        this._opts.onError?.(/** @type {Error} */ (err));
        void this.stop();
      }
    } finally {
      this.#lastTs = performance.now();
      this.#busy = false;
    }
  }

  /**
   * Auto-framing torch policy. The signal hierarchy:
   *  - QR presence 1 (finders found, decode failed): the code is THERE and
   *    marginal — light it after a short confirmation window.
   *  - QR presence 0 (no finders): scene empty — battery back.
   *  - No signal (linear-only formats): fall back to a no-decode timer.
   * Any manual overrideTorch() call stands the automation down for the
   * session; the user wins, always.
   * @param {LiveFrameResult} msg
   * @param {boolean} decoded
   */
  async #autoTorchPolicy(msg, decoded) {
    if (this._opts.autoTorch === false || this.#torchTouched || !this._running) return;
    const track = this._track();
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
    await this._applyTorch(track, want);
  }

  /** @protected @param {MediaStreamTrack} track @param {boolean} on */
  async _applyTorch(track, on) {
    return setTorch(track, on);
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
    return this._running ? this._applyTorch(this._track(), on) : false;
  }

  /**
   * Zoom reset: after the interaction ends (no decodes for a while), fall
   * back to native 1x so the next code can be found wide. Never runs when
   * the user set camera.zoom explicitly.
   * @param {DetectedCode[]} results
   */
  async #maybeZoomReset(results) {
    if (results.length || this.#zoomTouched) return;
    if (this._opts.camera?.zoom != null) return;
    const now = performance.now();
    if (now - this.#lastZoomAt < ZOOM_COOLDOWN_MS) return;
    if (!this._lastDecodeAt || now - this._lastDecodeAt < ZOOM_RESET_AFTER_MS) return;
    const track = this._track();
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
   * fills the target fraction of the frame.
   * @param {DetectedCode[]} results
   * @param {number} frameWidth Mask width in pixels.
   */
  async #maybeZoom(results, frameWidth) {
    if (!results.length || this.#zoomTouched) return;
    // Never fight an explicit camera.zoom from the options.
    if (this._opts.camera?.zoom != null) return;
    const now = performance.now();
    if (now - this.#lastZoomAt < ZOOM_COOLDOWN_MS) return;
    const track = this._track();
    const caps = trackCapabilities(track);
    if (!caps.zoom) return;

    // A surrounding code must not drive the framing: zooming toward a
    // corner QR would push the barcode the user is framing out of view.
    const targets = results.filter(aimed);
    if (!targets.length) return;
    let maxSide = 0;
    for (const r of targets) {
      const xs = r.position.map((p) => p.x);
      const ys = r.position.map((p) => p.y);
      maxSide = Math.max(
        maxSide,
        Math.max(...xs) - Math.min(...xs),
        Math.max(...ys) - Math.min(...ys),
      );
    }
    const fraction = maxSide / frameWidth;
    if (fraction >= ZOOM_TARGET_FRACTION) return;

    const zoomCaps = /** @type {NonNullable<TrackCaps["zoom"]>} */ (caps.zoom);
    const z = currentZoom(track) ?? zoomCaps.min;
    const target = Math.min(zoomCaps.max, z * (ZOOM_TARGET_FRACTION / fraction));
    if (target <= z * 1.05) return;
    this.#lastZoomAt = now;
    this.#hint = null;
    await setZoom(track, target);
  }

  /**
   * Two-consecutive-frame confirmation state machine (see notes/STATUS.md).
   * @param {DetectedCode[]} results ranked best-first by the worker
   * @param {boolean} slow true when the frame's wall time exceeded the budget
   * @param {string[] | null} [checked] families decoded this frame (null = all)
   * @returns {DetectedCode[]}
   */
  #confirm(results, slow, checked = null) {
    const ran = checked == null ? null : new Set(checked);
    const keyOf = (/** @type {DetectedCode} */ r) => r.symbology + "\u0000" + r.text;
    if (slow && results.length === 0) {
      if (ran == null) this.#streaks.clear();
      else {
        for (const [key, entry] of this.#streaks) {
          if (ran.has(entry.symbology)) this.#streaks.delete(key);
        }
      }
      return [];
    }
    const byKey = new Map(results.map((r) => [keyOf(r), r]));
    for (const [key, entry] of this.#streaks) {
      if (byKey.has(key)) continue;
      if (ran == null || ran.has(entry.symbology)) this.#streaks.delete(key);
    }
    for (const [key, r] of byKey) {
      if (ran != null && !ran.has(r.symbology)) continue;
      const entry = this.#streaks.get(key);
      if (entry) entry.streak += 1;
      else this.#streaks.set(key, { symbology: r.symbology, streak: 1 });
    }
    // Frames a value needs before it may end the scan. The reticle is the
    // promise: a code under the center point confirms as fast as ever (2
    // frames), while a surrounding code needs more agreement — enough to
    // never hijack a scan whose centered code is still settling, little
    // enough that a lone peripheral code still confirms (~0.2-0.4s).
    const streak = (/** @type {DetectedCode} */ r) => this.#streaks.get(keyOf(r))?.streak ?? 0;
    // A centered candidate one frame away is racing: a surrounding read
    // then needs two MORE frames. A margin, not a veto — a centered code
    // that flickers must never be able to stall a readable scan forever.
    const racing = results.some((r) => aimed(r) && streak(r) < 2);
    const need = (/** @type {DetectedCode} */ r) =>
      (aimed(r) ? 2 : (r.score ?? 0) >= PERIPHERAL_CAP ? 3 : 4) +
      (racing && !aimed(r) ? 2 : 0);
    // `results` is ranked best-first, so when several values reach their
    // requirement on the same frame the centered one wins the tie.
    const winner = results.find((r) => streak(r) >= need(r));
    return winner ? [winner] : [];
  }

  /**
   * Map positions decoded from a hinted close-up back to full-frame pixels.
   * @param {DetectedCode[]} results
   * @param {{ frameW?: number, frameH?: number }} msg
   */
  #mapFromHint(results, msg) {
    const grab = this._lastGrab;
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
   * Seed a re-confirmation hint after the first read of an expensive 2D code.
   * @param {DetectedCode[]} raw
   * @param {DetectedCode[]} results
   * @param {{ frameW?: number, frameH?: number }} msg
   */
  #updateHint(raw, results, msg) {
    if (this.#digitalZoom > 1) {
      this.#hint = null;
      return;
    }
    if (results.length) {
      this.#hint = null;
      return;
    }
    if (this.#hint || !raw.length) return;
    // Never lock frame 2 onto a SURROUNDING 2D code (a corner QR used to
    // own the close-up for 5 frames so a centered barcode was never seen).
    // Once a lone off-center 2D symbol has already survived 2 full frames
    // with nothing centered in view (e.g. a slightly off-center invoice DM),
    // allow the close-up so it finishes confirming fast instead of paying
    // full-frame DM sweeps on every frame.
    const lone = raw.length === 1 ? raw.at(0) : null;
    const r =
      raw.find((c) => HINTED.has(String(c.symbology)) && aimed(c)) ??
      (lone &&
      HINTED.has(String(lone.symbology)) &&
      (this.#streaks.get(`${lone.symbology}\u0000${lone.text}`)?.streak ?? 0) >= 2
        ? lone
        : null);
    if (!r) return;
    const video = this._opts.video;
    const vw = video.videoWidth;
    const vh = video.videoHeight;
    const maskW = msg.frameW ?? 0;
    const maskH = msg.frameH ?? 0;
    if (!vw || !vh || !maskW || !maskH || !r.position?.length) return;
    const square = this._opts.crop === "square";
    const side = square ? Math.min(vw, vh) : 0;
    const offX = square ? (vw - side) / 2 : 0;
    const offY = square ? (vh - side) / 2 : 0;
    const srcW = square ? side : vw;
    const srcH = square ? side : vh;
    const sx = srcW <= maskW ? 1 : srcW / maskW;
    const sy = srcH <= maskH ? 1 : srcH / maskH;
    const xs = r.position.map((/** @type {{x: number}} */ p) => p.x * sx + offX);
    const ys = r.position.map((/** @type {{y: number}} */ p) => p.y * sy + offY);
    let x = Math.min(...xs);
    let y = Math.min(...ys);
    let w = Math.max(...xs) - x;
    let h = Math.max(...ys) - y;
    if (!(w > 0 && h > 0)) return;
    // A code that already fills the frame gains nothing from a close-up,
    // and the ±25% margin can only clip it: a tall 2D symbol measured a
    // 508px crop around a 560px symbol, which decoded nowhere while the
    // full frame read every time. The full frame is the better opinion.
    const frameSide = Math.min(vw, vh) || 1;
    if (Math.max(w, h) > frameSide * 0.4) return;
    const mx = Math.max(16, w * 0.25);
    const my = Math.max(16, h * 0.25);
    x = Math.max(0, Math.round(x - mx));
    y = Math.max(0, Math.round(y - my));
    w = Math.min(vw - x, Math.round(w + 2 * mx));
    h = Math.min(vh - y, Math.round(h + 2 * my));
    if (w < 32 || h < 16) return;
    this.#hint = { x, y, w, h, format: r.symbology, frames: 5 };
  }

  /**
   * Grab a video frame honoring the active hint crop, `crop: "square"`,
   * and digital zoom.
   * @protected
   * @param {HTMLVideoElement} video
   * @returns {Promise<ImageBitmap>}
   */
  _grab(video) {
    this._lastGrab = null;
    const hint = this.#hint;
    if (hint && hint.frames > 0) {
      hint.frames -= 1;
      this._lastGrab = hint;
      if (hint.frames === 0) this.#hint = null;
      return createImageBitmap(video, hint.x, hint.y, hint.w, hint.h);
    }
    const vw = video.videoWidth;
    const vh = video.videoHeight;
    const zoom = Math.max(1, this.#digitalZoom);
    if (this._opts.crop !== "square") {
      if (zoom === 1) return createImageBitmap(video);
      const w = vw / zoom;
      const h = vh / zoom;
      const x = (vw - w) / 2;
      const y = (vh - h) / 2;
      this._lastGrab = { x, y, w, h, format: null, frames: 0 };
      return createImageBitmap(video, x, y, w, h);
    }
    const s = Math.min(vw, vh);
    if (!s) return createImageBitmap(video);
    const side = s / zoom;
    const x = (vw - side) / 2;
    const y = (vh - side) / 2;
    if (zoom > 1) this._lastGrab = { x, y, w: side, h: side, format: null, frames: 0 };
    return createImageBitmap(video, x, y, side, side);
  }

  /**
   * One video frame through the injected decoder (strictly serial: one
   * request at a time, bitmap owned by the decoder and closed after use).
   * @protected
   * @param {HTMLVideoElement} video
   * @returns {Promise<LiveFrameResult>}
   */
  async _decode(video) {
    const bitmap = await this._grab(video);
    try {
      const hint = this._lastGrab;
      const res = await /** @type {NonNullable<LiveScannerOptions["decode"]>} */ (this._opts.decode)(
        bitmap,
        hint?.format ? [hint.format] : this._opts.formats,
        this._opts.accuracy,
      );
      if (res?.error) throw new Error(res.error);
      return {
        results: res?.results ?? [],
        qrPresence: res?.qrPresence,
        partial: res?.partial === true,
        checked: res?.checked ?? null,
        frameW: res?.frameW,
        frameH: res?.frameH,
      };
    } finally {
      bitmap.close?.();
    }
  }

  /** @protected @returns {MediaStreamTrack} */
  _track() {
    const t = this._stream?.getVideoTracks()[0];
    if (!t) throw new Error("Scanner not started.");
    return t;
  }

  hasTorch() {
    return this._running ? trackCapabilities(this._track()).torch === true : false;
  }

  /** Whether the camera track exposes its own zoom constraint. */
  hasZoom() {
    return this._running ? !!trackCapabilities(this._track()).zoom : false;
  }

  /**
   * The effective zoom: the camera constraint when supported, otherwise
   * the digital center crop. Never null — the default UI's pinch and
   * slider both drive this.
   */
  get zoomFactor() {
    if (!this._running) return 1;
    return currentZoom(this._track()) ?? this.#digitalZoom;
  }

  /**
   * Set the zoom factor. Uses the camera track's own `zoom` constraint
   * when it has one; otherwise crops the center of the source (1–4x).
   * A manual zoom stands the adaptive policy down for the session.
   *
   * @param {number} factor
   * @returns {Promise<number | null>} the applied track value, the
   *   digital factor, or null when the scanner is not running.
   */
  async zoomTo(factor) {
    this.#zoomTouched = true;
    this._nativeZoom = false;
    if (!this._running) return null;
    const track = this._track();
    /** @type {number | null} */
    let applied = null;
    try {
      applied = await setZoom(track, factor);
    } catch {
      applied = null;
    }
    if (applied !== null) {
      this.#digitalZoom = 1;
      this.#hint = null;
      this._nativeZoom = true;
      return applied;
    }
    const z = Math.min(4, Math.max(1, factor));
    this.#digitalZoom = z;
    this.#hint = null;
    return z;
  }

  /** Stop scanning: close camera, halt pump, clear confirmation state. */
  async stop() {
    this._running = false;
    this._stream?.getTracks().forEach((t) => t.stop());
    this._stream = null;
    this._opts.video.srcObject = null;
    this.#streaks.clear();
    this.#hint = null;
    this._lastGrab = null;
    this.#failStreak = 0;
  }
}
