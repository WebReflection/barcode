// The SDK main thread. One session per page/SPA:
//
//   import barcode from './api/main.js';
//   const { scan, render, worker } = barcode({ type: 'barcode' });
//
//   const controller = new AbortController;
//   stopButton.onclick = () => controller.abort();
//   const code = await scan({ video, signal: controller.signal });
//   // code: { symbology, text, raw, position, score } — or undefined when
//   // the scan ended without a read (abort, or the still latency expired)
//   if (code) await render(code);
//
// Contract:
//   - barcode(options) boots the session instantly: a string/URL worker
//     spawns right here, a caller-created Worker is used as-is, the
//     MessagePort is handed over immediately (calls queue until the worker
//     binds), and the returned `worker` is always the live Worker instance
//     (ours or the caller's). Handing the same worker to a second session
//     throws: the worker side binds exactly once, ever ('duplicated
//     worker'). The session is the only owner: abort never terminates it.
//   - scan(options) resolves ONCE with the best code (most centered
//     first, larger symbol as tiebreak — each result carries its own
//     score) or `undefined` — a graceful end never throws. Real errors
//     (camera denied, broken pipeline) reject.
//   - Modes, mutually exclusive: { into } mounts the default UI there
//     (default: document.body, with its own ✕ that aborts the internal
//     signal); { video } hooks a caller-provided element (bring your
//     own signal to stop early); { image } scans a still — canvas,
//     ImageBitmap, HTMLImageElement, SVG, Blob/File — in one shot.
//   - The user-provided signal is chained into an internal controller
//     (one per scan, fresh each time): user abort, UI ✕ and any other
//     graceful stop converge on the same teardown → `undefined`.
//   - Session options (`type`, `accuracy`) are defaults every scan
//     inherits; per-scan options win. `latency` is per-scan only, and
//     its default depends on the mode: 500ms live, 5000ms still.
//   - The worker is strictly serial: an aborted still decode may delay
//     the next scan by up to its own latency cap (live: half a second).

import binding, { options as transfer } from '@webreflection/bindings/message-port';
import { svg } from '@webreflection/utils/dom/content';
import { Scanner } from '../src/scanner.js';
import { bitmapToSVG, encodableText } from '../src/svg.js';
import { normalizeRenderOptions } from '../src/render-options.js';

/** Workers that already bound a session: a second session must not post
 *  a port into a listener that no longer exists. */
const handled = new WeakSet();

/**
 * A symbology name, in the lowercase canonical form the encoders speak.
 * @typedef {'qrcode' | 'microqr' | 'aztec' | 'datamatrix' | 'pdf417' |
 *   'upca' | 'ean13' | 'ean8' | 'upce' | 'code39' | 'code32' | 'code93' | 'code128' |
 *   'itf' | 'codabar' | 'databar' | 'databarltd' | 'databarstk' |
 *   'databarexpanded'} Symbology
 */

/**
 * What to decode: a coarse group ('qr', 'barcode'), an exact symbology
 * name, or a list mixing both. Anything unknown is an exact name.
 * @typedef {string | string[]} TypeOption
 */

/** @typedef {'low' | 'auto' | 'high'} Accuracy */

/** @typedef {'ultrawide' | 'wide' | 'default'} Lens */

/**
 * Where the pipeline lives: a worker script URL (spawned here as a
 * module worker) or an already-created Worker used as-is. The returned
 * `worker` is always the live Worker instance either way.
 * @typedef {string | URL | Worker} WorkerSource
 */

/**
 * @typedef {{
 *   worker?: WorkerSource,
 *   type?: TypeOption,
 *   accuracy?: Accuracy,
 *   lens?: Lens,
 * }} BarcodeOptions
 */

/**
 * One decoded symbol, as resolved by scan() (and accepted by render()).
 * @typedef {{
 *   symbology: string,
 *   text: string,
 *   raw: Uint8Array,
 *   position: Array<{x: number, y: number}>,
 *   score?: number,
 * }} Code
 */

/**
 * Still-image inputs accepted by scan({ image }).
 * @typedef {HTMLCanvasElement | OffscreenCanvas | ImageBitmap |
 *   HTMLImageElement | SVGSVGElement | Blob} StillImage
 */

/**
 * @typedef {{
 *   into?: string | Node,
 *   video?: HTMLVideoElement,
 *   image?: StillImage,
 *   signal?: AbortSignal,
 *   latency?: number,
 *   type?: TypeOption,
 *   accuracy?: Accuracy,
 *   lens?: Lens,
 *   crop?: 'square' | 'full',
 *   zoom?: number,
 *   torch?: boolean,
 * }} ScanOptions
 */

/**
 * Symbol layout for `render()`: module size, 1D height, QR error
 * correction / mask / version, PDF417 EC / columns, Data Matrix size.
 * Every field is optional and keeps its default. The full contract and
 * ranges live in `src/render-options.js`.
 * @typedef {import('../src/render-options.js').RenderOptions} RenderOptions
 */

/**
 * @typedef {{
 *   symbology: string,
 *   text?: string,
 *   raw?: Uint8Array,
 * }} RenderInput
 */

/**
 * The bound-worker surface, as produced by
 * {@link https://github.com/WebReflection/bindings|@webreflection/bindings}:
 * every handler name becomes an async method; `[options]` carries the
 * per-call transfer list.
 * @typedef {{
 *   [transfer]: (opts?: { transfer?: Transferable[] }) => void,
 *   decode: (msg: DecodeRequest) => Promise<DecodeResponse>,
 *   encode: (msg: EncodeRequest) => Promise<EncodeResponse>,
 * }} Bindings
 */

/** The worker owns the wire protocol; both sides type against it. */
/** @typedef {import('./worker.js').DecodeRequest} DecodeRequest */
/** @typedef {import('./worker.js').DecodeResponse} DecodeResponse */
/** @typedef {import('./worker.js').EncodeRequest} EncodeRequest */
/** @typedef {import('./worker.js').EncodeResponse} EncodeResponse */
/** @typedef {import('./worker.js').RankedCode} RankedCode */

/** Coarse `type` groups; any other value is an exact symbology name. */
/** @type {Record<string, Symbology[]>} */
const GROUPS = {
  qr: ['qrcode', 'microqr'],
  barcode: [
    'upca', 'ean13', 'ean8', 'upce', 'code39', 'code32', 'code93', 'code128',
    'itf', 'codabar', 'databar', 'databarltd', 'databarstk', 'databarexpanded',
  ],
};

/** `type` in, normalized lowercase `formats` list out (or undefined). */
const resolveType = (/** @type {TypeOption | undefined} */ type) => {
  if (type == null) return undefined;
  const list = Array.isArray(type) ? type : [type];
  return list.flatMap((/** @type {string} */ t) => {
    const name = String(t).trim().toLowerCase();
    return /** @type {string[]} */ (GROUPS[name] ?? [name]);
  });
};

const loadImage = (/** @type {string} */ src) =>
  new Promise((/** @type {(v: HTMLImageElement) => void} */ res, /** @type {(e: Error) => void} */ rej) => {
    const img = new Image();
    img.onload = () => res(img);
    img.onerror = () => rej(new TypeError('image failed to load'));
    img.src = src;
  });

/** Anything drawable → canvas at its natural size → ImageBitmap. */
const viaCanvas = (/** @type {CanvasImageSource} */ source, /** @type {number} */ width, /** @type {number} */ height) => {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  canvas.getContext('2d')?.drawImage(source, 0, 0);
  return createImageBitmap(canvas);
};

/** Any accepted still input → ImageBitmap (the decode's unit of work). */
const toBitmap = async (/** @type {StillImage} */ image) => {
  if (
    image instanceof HTMLCanvasElement ||
    image instanceof Blob ||
    image instanceof ImageBitmap ||
    (typeof OffscreenCanvas !== 'undefined' && image instanceof OffscreenCanvas)
  ) return createImageBitmap(image);
  if (image instanceof HTMLImageElement) {
    if (!image.complete) await image.decode();
    return viaCanvas(image, image.naturalWidth, image.naturalHeight);
  }
  if (typeof SVGSVGElement !== 'undefined' && image instanceof SVGSVGElement) {
    // serialize → blob URL → Image → canvas
    const xml = new XMLSerializer().serializeToString(image);
    const url = URL.createObjectURL(new Blob([xml], { type: 'image/svg+xml' }));
    try {
      const img = await loadImage(url);
      return viaCanvas(img, img.naturalWidth, img.naturalHeight);
    } finally {
      URL.revokeObjectURL(url);
    }
  }
  throw new TypeError('scan: unsupported image input');
};

export default (/** @type {BarcodeOptions} */ opts = {}) => {
  const { worker, type, accuracy, lens } = opts;

  // The session spine: the worker spawns right here, its MessagePort is
  // handed over in the same tick, and the binding proxy is ready for calls
  // immediately (port1 queues them until the worker binds).
  let /** @type {Worker} */ w;
  if (worker == null) {
    // Keep the literal inside `new Worker`: it is standard JavaScript, and
    // a bundler that understands worker entries (Vite) emits ./worker.js
    // and its sibling wasm.wasm from the worker's own import.meta.url.
    w = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
  } else if (typeof worker === 'string' || worker instanceof URL) {
    w = new Worker(worker, { type: 'module' });
  } else if (worker instanceof Worker) {
    w = worker;
    // a fresh Worker cannot be in `handled`: only a caller-supplied one can
    if (handled.has(w)) throw new Error('duplicated worker');
  } else {
    throw new TypeError('barcode: worker must be a string, a URL, or a Worker');
  }
  handled.add(w);
  const { port1, port2 } = new MessageChannel;
  const utils = /** @type {Bindings} */ (binding(port1, {}));
  // The worker binds this port on the first message it receives. The
  // worker entry must import the SDK statically at the top: a lazily
  // imported worker module may miss the port, and that is not recoverable.
  w.postMessage(null, { transfer: [port2] });

  // Session-level defaults every scan inherits; per-scan options win.
  const sessionType = type;
  const sessionAccuracy = accuracy;
  const sessionLens = lens;

  // One scan at a time: the worker is strictly serial and the
  // confirmation law assumes a single live source. render() has no such
  // limit — it is stateless and safe to fire N times in parallel.
  let activeScan = false;

  /**
   * One complete scan. Resolves once, with the best code or `undefined`.
   *
   * @param {ScanOptions} [options]
   *   Defaults to `{ into: document.body }` — the default UI on the page.
   *   Camera knobs: `lens` (session default), `crop` ("square" for the
   *   default UI — what you frame is what decodes — "full" otherwise),
   *   `zoom` initial factor, `torch` initial state (the default UI also
   *   carries a toggle when the track supports it). The default UI's
   *   two-finger pinch drives the track zoom and stands adaptive zoom
   *   down; pass `{ video }` to bring your own UI and your own gestures.
   * @returns {Promise<Code | undefined>}
   */
  const scan = (options = { into: document.body }) => {
    const {
      into, video, image, signal, latency,
      lens = sessionLens, crop, zoom, torch,
      type = sessionType, accuracy = sessionAccuracy ?? 'auto',
    } = options;
    const { promise, resolve, reject } = Promise.withResolvers();
    let settled = false;
    const internal = new AbortController();
    // Idempotent graceful stop: abort() twice is a no-op anyway, and the
    // aborted check keeps the internal listener contract obvious.
    const stop = () => {
      if (!internal.signal.aborted) internal.abort();
    };
    const settle = (
      /** @type {(v?: unknown) => void} */ fn,
      /** @type {unknown} */ arg,
    ) => {
      if (!settled) {
        settled = true;
        activeScan = false;
        // A settled scan owns nothing: the caller's signal must not keep
        // this scan's closure alive.
        signal?.removeEventListener('abort', stop);
        fn(arg);
      }
    };

    // A rejected call must NOT touch `activeScan`: it never owned it.
    if (activeScan)
      return Promise.reject(new Error('scan: another scan is already in progress'));
    // Option errors reject like every other scan error (they used to
    // throw synchronously, bypassing the promise contract).
    if (signal && typeof signal.addEventListener !== 'function')
      return Promise.reject(new TypeError('scan: signal must be an AbortSignal (pass controller.signal)'));

    const modes = Number(image != null) + Number(video != null) + Number(into != null);
    if (modes > 1)
      return Promise.reject(new TypeError('scan: pick one of into, video, image'));
    activeScan = true;

    // One graceful exit for every mode: whoever aborts — the user's
    // controller or our UI's ✕ — settles `undefined`. Errors are the
    // only rejections. The worker is never touched by an abort.
    /** @type {() => void} */
    const onInternalAbort = () => settle(resolve, undefined);
    internal.signal.addEventListener('abort', onInternalAbort, { once: true });
    if (signal) {
      if (signal.aborted) stop();
      else signal.addEventListener('abort', stop, { once: true });
    }
    if (settled) return promise;

    if (image != null) {
      // still path: one shot, no confirmation — latency law defaults
      // to 5000ms and is the per-scan escape hatch
      (async () => {
        const bitmap = await toBitmap(image);
        if (settled) {
          bitmap.close?.();
          return;
        }
        utils[transfer]({ transfer: [bitmap] });
        const res = await utils.decode({
          bitmap,
          formats: resolveType(type),
          still: true,
          accuracy,
          latency: latency ?? 5000,
        });
        if (settled) return;
        if (res?.type === 'error') settle(reject, new Error(res.message));
        else settle(resolve, res?.codes?.[0]);
      })().catch((err) => settle(reject, err));
      return promise;
    }

    // live path: camera frames + the confirmation law (Scanner)
    (async () => {
      const hasInto = into != null;
      const target = hasInto ? into : document.body;
      /** @type {HTMLVideoElement | null} */ let v = video ?? null;
      /** @type {HTMLDialogElement | null} */ let dialog = null;
      /** @type {HTMLButtonElement | null} */ let torchBtn = null;
      /** @type {HTMLInputElement | null} */ let zoomInput = null;
      /** @type {HTMLElement | null} */ let zoomLabel = null;
      /** @type {HTMLElement | null} */ let zoomText = null;
      /** @type {HTMLElement | null} */ let progressFill = null;
      /** @type {Scanner | null} */ let scanner;
      /**
       * Digital zoom must be visible: the preview shows exactly the
       * center crop the decoder receives. Native track zoom already
       * zooms the camera preview, so this is a no-op there.
       * @param {number} factor
       */
      const syncPreview = (factor) => {
        if (!scanner || scanner.hasZoom()) return;
        const z = Math.min(4, Math.max(1, factor));
        if (v) /** @type {HTMLVideoElement} */ (v).style.transform = `scale(${z})`;
        if (zoomText) zoomText.textContent = `Zoom ${z.toFixed(1)}×`;
      };
      const teardown = () => {
        internal.signal.removeEventListener('abort', onAbort);
        try { scanner?.stop(); } catch (_) {}
        dialog?.remove();
        dialog = null;
      };
      const onAbort = () => teardown();
      internal.signal.addEventListener('abort', onAbort, { once: true });

      if (!video) {
        // The default UI: a square viewfinder (side = min(86vw, 86dvh,
        // 440px) — never taller than wide on portrait, never wider than
        // tall on landscape), corner brackets, a scan line, a hint, a
        // torch toggle when the track supports one, and our ✕ — the
        // internal controller is ours to abort. The stage is cropped
        // square at grab time: what the user frames is what decodes.
        const host = typeof target === 'string' ? document.querySelector(target) : target;
        if (!host)
          throw new TypeError(`scan: no element matches ${JSON.stringify(target)}`);
        dialog = /** @type {HTMLElement} */ (host).appendChild(document.createElement('dialog'));
        dialog.className = 'barcode-ui';
        const videoEl = Object.assign(document.createElement('video'), {
          autoplay: true, playsInline: true, muted: true,
        });
        const stage = dialog.appendChild(document.createElement('div'));
        stage.className = 'barcode-ui-stage';
        v = stage.appendChild(videoEl);
        const corners = stage.appendChild(document.createElement('div'));
        corners.className = 'barcode-ui-corners';
        corners.setAttribute('aria-hidden', 'true');
        for (let i = 0; i < 4; i++) corners.appendChild(document.createElement('i'));
        const line = stage.appendChild(document.createElement('div'));
        line.className = 'barcode-ui-line';
        line.setAttribute('aria-hidden', 'true');
        // Read feedback: 0% while hunting, 50% on the first frame that
        // read (one read is fine, two to confirm — not yet), 100% on
        // confirmation, where the dialog waits for its exit animation
        // (≤300ms) before closing.
        const progress = stage.appendChild(document.createElement('div'));
        progress.className = 'barcode-ui-progress';
        progress.setAttribute('aria-hidden', 'true');
        progressFill = progress.appendChild(document.createElement('i'));
        const hint = dialog.appendChild(document.createElement('p'));
        hint.className = 'barcode-ui-hint';
        hint.textContent = 'Center the code in the frame';
        const torchBtnEl = Object.assign(document.createElement('button'), {
          type: 'button', className: 'barcode-ui-torch',
          'aria-label': 'Toggle torch', hidden: true,
        });
        // inline SVG via @webreflection/utils/dom/content — no innerHTML
        torchBtnEl.append(svg(
          '<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">' +
          '<path d="M13 2 4.5 13.5h5L10 22l9-12h-5.5L13 2z" fill="currentColor"/></svg>',
        ));
        torchBtn = dialog.appendChild(torchBtnEl);
        // Native track zoom is the preferred gesture; when the camera has
        // no zoom capability (Android WebView, desktop webcams) the same
        // pinch drives a digital center crop and this slider is shown.
        zoomLabel = dialog.appendChild(document.createElement('label'));
        zoomLabel.className = 'barcode-ui-zoom';
        zoomText = zoomLabel.appendChild(document.createElement('span'));
        zoomText.className = 'barcode-ui-zoom-label';
        zoomText.textContent = 'Zoom 1.0×';
        zoomInput = /** @type {HTMLInputElement} */ (zoomLabel.appendChild(document.createElement('input')));
        Object.assign(zoomInput, {
          type: 'range',
          min: '1', max: '4', step: '0.1', value: '1',
          'aria-label': 'Zoom',
        });
        zoomLabel.hidden = true;
        const close = dialog.appendChild(document.createElement('button'));
        close.type = 'button';
        close.className = 'barcode-ui-close';
        close.textContent = '✕';
        close.setAttribute('aria-label', 'Stop scanning');
        close.addEventListener('click', stop, { once: true });
        // Escape closes a modal <dialog> on its own: without this the
        // camera and the decode pump kept running invisibly and scan()
        // stayed pending forever. Same graceful exit as ✕.
        dialog.addEventListener('cancel', stop);
        dialog.showModal();

        // Pinch-to-zoom owns the viewfinder's touch gesture: two pointers
        // drive the CAMERA's track zoom, not a CSS scale. Scaling the
        // preview would not help the decoder — it reads the stream's
        // pixels — while a track zoom is exactly what puts more pixels on
        // a code too small to read at 1x. css.css sets `touch-action:
        // none` on the dialog/stage so the page itself cannot zoom; the
        // non-passive touchmove listener is the belt for iOS Safari
        // builds that still page-zoom from inside a modal.
        /** @type {Map<number, { x: number, y: number }>} */
        const pointers = new Map();
        /** @type {{ ids: number[], distance: number, zoom: number } | null} */
        let pinch = null;
        /** Latest requested factor; pointermoves coalesce into one constraint. */
        /** @type {number | null} */
        let pendingZoom = null;
        let applyingZoom = false;
        const applyZoom = async () => {
          if (applyingZoom) return;
          applyingZoom = true;
          try {
            while (pendingZoom != null) {
              const factor = pendingZoom;
              pendingZoom = null;
              if (!scanner || settled) break;
              await scanner.zoomTo(factor, false);
            }
          } catch {
            // camera closed mid-gesture: teardown already owns the exit
          } finally {
            applyingZoom = false;
          }
        };
        /** @param {number} factor */
        const requestZoom = (factor) => {
          if (!Number.isFinite(factor)) return;
          if (zoomInput) {
            /** @type {HTMLInputElement} */ (zoomInput).value =
              String(Math.min(4, Math.max(1, factor)));
          }
          syncPreview(factor);
          pendingZoom = factor;
          void applyZoom();
        };
        const startPinch = () => {
          const [first, second] = [...pointers];
          if (!first || !second) return;
          const [idA, a] = first;
          const [idB, b] = second;
          const distance = Math.hypot(a.x - b.x, a.y - b.y);
          // zoomFactor is the native constraint when available and the
          // digital crop otherwise; it is never null, so pinch always has
          // a base factor.
          const zoom = scanner?.zoomFactor ?? 1;
          if (distance > 0) pinch = { ids: [idA, idB], distance, zoom };
        };
        stage.addEventListener('pointerdown', (event) => {
          pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
          try { stage.setPointerCapture(event.pointerId); } catch (_) {}
          startPinch();
          event.preventDefault();
        });
        stage.addEventListener('pointermove', (event) => {
          const point = pointers.get(event.pointerId);
          if (!point) return;
          point.x = event.clientX;
          point.y = event.clientY;
          if (pinch) {
            const [a, b] = pinch.ids.map((id) => pointers.get(id));
            const distance = a && b ? Math.hypot(a.x - b.x, a.y - b.y) : 0;
            if (distance > 0) requestZoom(pinch.zoom * (distance / pinch.distance));
          }
          event.preventDefault();
        });
        /** @param {PointerEvent} event */
        const endPointer = (event) => {
          pointers.delete(event.pointerId);
          if (pinch && !pinch.ids.every((id) => pointers.has(id))) pinch = null;
          // three pointers down, one lifted: the remaining pair can pinch
          startPinch();
          event.preventDefault();
        };
        stage.addEventListener('pointerup', endPointer);
        stage.addEventListener('pointercancel', endPointer);
        /** @param {TouchEvent} event */
        const stopPageZoom = (event) => {
          if (event.touches.length > 1) event.preventDefault();
        };
        dialog.addEventListener('touchmove', stopPageZoom, { passive: false });
      }

      scanner = new Scanner({
        video: /** @type {HTMLVideoElement} */ (v),
        budget: latency ?? 500,
        formats: resolveType(type),
        accuracy,
        lens: lens ?? undefined,
        // square is THE default: whatever the viewfinder frames is
        // exactly what decodes — "full" is the explicit opt-out
        crop: crop ?? 'square',
        camera: { zoom, torch },
        onResults: (/** @type {RankedCode[]} */ codes) => {
          // Confirmation: stop decoding now; if the default UI is up,
          // let it show 100% and wait for its exit animation (≤300ms,
          // driven by the real `animationend` with a fallback timer —
          // a missed event can never hang the scan) before closing.
          const best = codes[0];
          try { scanner?.stop(); } catch (_) {}
          // The scan is over: the caller's abort may not tear down a UI
          // that is playing its exit animation, and the confirmed read
          // already won — an abort now must not turn it into `undefined`.
          for (const fn of [onAbort, onInternalAbort])
            internal.signal.removeEventListener('abort', fn);
          const ui = /** @type {HTMLDialogElement} */ (dialog);
          const fill = /** @type {HTMLElement} */ (progressFill);
          if (!ui) {
            settle(resolve, best);
            return;
          }
          fill.style.width = '100%';
          ui.classList.add('barcode-ui-done');
          let closed = false;
          const close = () => {
            if (closed) return;
            closed = true;
            ui.remove();
            settle(resolve, best);
          };
          ui.addEventListener('animationend', close, { once: true });
          setTimeout(close, 320);
        },
        onError: (/** @type {Error} */ err) => {
          teardown();
          settle(reject, err);
        },
        // Artifact contract: frames decode through the bound worker
        // (worker.js + wasm.wasm) — no extra pipeline file.
        decode: async (bitmap, fmts, acc) => {
          const push = /** @type {Record<symbol, ((opts: { transfer?: Transferable[] }) => void) | undefined>} */ (utils)[transfer];
          push?.({ transfer: [bitmap] });
          const res = /** @type {DecodeResponse} */ (await utils.decode({ bitmap, formats: fmts, still: false, accuracy: acc }));
          // a frame carried candidates: the confirmation law is halfway
          if (dialog && progressFill && !settled && res?.type === 'result' && res.codes.length && !dialog.classList.contains('barcode-ui-done'))
            progressFill.style.width = '50%';
          return res?.type === 'error'
            ? { results: [], error: res.message }
            : { results: res.codes ?? [], qrPresence: res.qrPresence, partial: res.partial === true, checked: res.checked ?? null, frameW: res.frameW, frameH: res.frameH };
        },
      });
      try {
        await scanner.start();
        // The slider exists only when the camera cannot zoom natively;
        // pinch drives both paths (see zoomTo in scanner.js).
        if (!video && zoomLabel && zoomInput && !scanner.hasZoom()) {
          const slider = /** @type {HTMLInputElement} */ (zoomInput);
          /** @type {HTMLElement} */ (zoomLabel).hidden = false;
          slider.addEventListener('input', () => {
            syncPreview(Number(slider.value));
            void scanner.zoomTo(Number(slider.value), false);
          });
        }
        // the torch toggle only exists when the track can actually do it
        if (!video && torchBtn && scanner.hasTorch()) {
          const btn = /** @type {HTMLButtonElement} */ (torchBtn);
          btn.hidden = false;
          // The camera already applied the caller's initial `torch`
          // option: the toggle must show that state, not a hard false.
          let on = torch === true;
          btn.classList.toggle('on', on);
          btn.addEventListener('click', () => {
            on = !on;
            // Manual wins: the Scanner's auto-torch policy stands down for
            // the session. setTorch() alone would let the policy turn the
            // light back on a second later on a marginal QR frame.
            void scanner.overrideTorch(on);
            btn.classList.toggle('on', on);
          });
        }
      } catch (err) {
        teardown();
        settle(reject, /** @type {Error} */ (err));
      }
    })().catch((err) => settle(reject, err));
    return promise;
  };

  /**
   * Render a scan result (or a bare {symbology, text} / {symbology,
   * raw}) as an SVGSVGElement, styleable via CSS custom properties
   * (--barcode-ink, --barcode-paper). `undefined`/empty → null.
   * Stateless: safe to fire N times in parallel.
   *
   * A non-empty `raw` (Uint8Array of exact payload bytes) wins over
   * `text`: binary payloads survive only there, and re-rendering a
   * scan result from its `text` would silently re-encode the Latin-1
   * display form. For symbologies without binary encoders `raw` must
   * be the UTF-8 of the intended text (as it always is on scan
   * results) — the worker falls back to the text path.
   *
   * `opts` keeps the historical defaults when a field is omitted:
   *   module (4), height (30 * module)
   *   ec "L"|"M"|"Q"|"H" (M), mask 0..7 (2), version 1..40 (smallest)
   *   level 0..8 (PDF417 default), columns 1..30 (auto)
   *   rows/cols (Data Matrix ECC200 size incl. borders; smallest square)
   * Out-of-range values throw a TypeError here, before the worker.
   *
   * @param {Code | RenderInput | null | undefined} result
   * @param {RenderOptions} [opts]
   * @returns {Promise<SVGSVGElement | null>}
   */
  const render = async (result, opts = {}) => {
    const raw = result?.raw instanceof Uint8Array && result.raw.length ? result.raw : null;
    const text = typeof result?.text === 'string' ? result.text : '';
    if (!result || (!raw && !text)) return null;
    // Decode results are canonical already; hand-built inputs may use any
    // case, so this boundary still normalizes once for the encoder.
    const symbology = String(result.symbology ?? '').trim().toLowerCase();
    // Throws a clear TypeError for out-of-range / unknown options.
    const layout = normalizeRenderOptions(opts);
    const module = opts.module ?? 4;
    // binary-capable symbologies encode the exact bytes; for everything
    // else a `raw` must be the UTF-8 of the intended text (as it always
    // is on scan results) — the text path is byte-identical then
    const useRaw = raw && (BINARY_RENDER.has(symbology) || !text);
    // GS1 DataBar scan text is `(01)` + 14 digits; encoders take the
    // 13-digit body (see encodableText).
    const payload = encodableText(symbology, text);
    const fx = await utils.encode({
      symbology,
      text: payload,
      raw: useRaw ? raw : null,
      module,
      height: opts.height ?? 30 * module,
      ...layout,
    });
    return bitmapToSVG(fx, text || `binary: ${raw?.length ?? 0} bytes`);
  };

  return { scan, render, worker: w };
};

/** Symbologies whose encoders accept exact byte payloads. */
const BINARY_RENDER = new Set(['qrcode', 'microqr', 'datamatrix', 'aztec', 'pdf417']);
