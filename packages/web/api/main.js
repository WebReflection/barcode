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
//     spawns right here, a caller-created Worker is used as-is, a
//     port-less ping/ready handshake then introduces the MessagePort, and
//     the returned `worker` is always the live Worker instance (ours or
//     the caller's). Handing the same worker to a second session throws:
//     the worker side binds exactly once, ever ('duplicated worker').
//     The session is the only owner: abort never terminates the worker.
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
import { bitmapToSVG } from '../src/svg.js';
import { normalizeRenderOptions } from '../src/render-options.js';

/** Workers that already crossed a handshake: a second session must not
 *  post a port into a listener that no longer exists. */
const handled = new WeakSet();

/**
 * A symbology name, in the lowercase canonical form the encoders speak.
 * @typedef {'qrcode' | 'microqr' | 'aztec' | 'datamatrix' | 'pdf417' |
 *   'upca' | 'ean13' | 'ean8' | 'upce' | 'code39' | 'code93' | 'code128' |
 *   'itf' | 'codabar' | 'databar' | 'databarltd' | 'databarstk'} Symbology
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
    'upca', 'ean13', 'ean8', 'upce', 'code39', 'code93', 'code128',
    'itf', 'codabar', 'databar', 'databarltd', 'databarstk',
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

/** @type {URL | undefined} */
let distWorker;
/** @type {string | undefined} */
let distWasm;
if (typeof __BARCODE_DIST__ === 'string') {
  // Dist bundle: worker.js and wasm.wasm sit beside main.js. The wasm URL
  // is sent in the readiness ping so consumer bundlers (Vite/Rollup) see
  // `new URL('./wasm.wasm', import.meta.url)` and emit the asset, without
  // putting a query on the worker URL (which some dev servers dislike).
  distWorker = new URL('./worker.js', import.meta.url);
  distWasm = new URL('./wasm.wasm', import.meta.url).href;
  // A fragment survives on module Workers where some dev servers mishandle
  // query strings; the worker reads it at MODULE EVALUATION time (its wasm
  // singleton boots at import time, before any message can arrive).
  distWorker.hash = 'wasm=' + encodeURIComponent(distWasm);
}

export default (/** @type {BarcodeOptions} */ opts = {}) => {
  const customWorker = opts.worker != null;
  const { worker = distWorker ?? new URL('./worker.js', import.meta.url), type, accuracy, lens } = opts;

  // The session spine: create the binding proxy now, introduce the port
  // as soon as the worker answers the ping (below). Calls made before the
  // port crosses are queued on port1; nothing here is lazy and barcode()
  // returns immediately.
  let /** @type {Worker} */ w;
  if (typeof worker === 'string' || worker instanceof URL)
    w = new Worker(worker, { type: 'module' });
  else if (worker instanceof Worker)
    w = worker;
  else
    throw new TypeError('barcode: worker must be a string, a URL, or a Worker');
  if (handled.has(w)) throw new Error('duplicated worker');
  handled.add(w);
  const { port1, port2 } = new MessageChannel;
  const utils = /** @type {Bindings} */ (binding(port1, {}));

  // Introduce the port only after the worker says it is listening. A
  // worker script starts asynchronously; a MessagePort posted in the same
  // tick as `new Worker(...)` can be dispatched while its module graph
  // (wasm, GPU binarizer) is still evaluating — before worker.js installs
  // the capture — and is then lost. A port-less ping travels safely,
  // worker.js answers with READY, and only then does the port cross.
  // Messages sent on `port1` before the peer binds are queued, so
  // scan()/render() called immediately are safe.
  let introduced = false;
  const ping = () => {
    if (introduced) return;
    w.postMessage({
      type: 'barcode:ping',
      // Only the default worker belongs to this package's dist; a caller's
      // worker must keep resolving its own adjacent wasm.
      wasm: customWorker ? null : distWasm,
    });
  };
  /** @param {MessageEvent} event */
  const introduce = (event) => {
    if (event.data?.type !== 'barcode:ready') return;
    if (introduced) return;
    introduced = true;
    w.removeEventListener('message', introduce);
    clearInterval(pingTimer);
    w.postMessage(null, { transfer: [port2] });
  };
  w.addEventListener('message', introduce);
  // Repeat the port-less ping: a worker whose module graph is still
  // evaluating can miss the first one (message events are not held back
  // by a pending dynamic import). Every ping is harmless.
  ping();
  const pingTimer = setInterval(ping, 250);
  // Safety net for a worker that predates the ping protocol: never hang
  // forever on a handshake. The old behavior posted the port at once.
  setTimeout(() => {
    if (introduced) return;
    introduced = true;
    w.removeEventListener('message', introduce);
    clearInterval(pingTimer);
    w.postMessage(null, { transfer: [port2] });
  }, 10_000);

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
   *   carries a toggle when the track supports it).
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
    const settle = (
      /** @type {(v?: unknown) => void} */ fn,
      /** @type {unknown} */ arg,
    ) => {
      if (!settled) {
        settled = true;
        activeScan = false;
        fn(arg);
      }
    };

    if (activeScan) {
      settle(reject, new Error('scan: another scan is already in progress'));
      return promise;
    }

    if (signal && (typeof signal !== 'object' || typeof signal.addEventListener !== 'function'))
      throw new TypeError('scan: signal must be an AbortSignal (pass controller.signal)');

    const modes = Number(image != null) + Number(video != null) + Number(into != null);
    if (modes > 1) {
      settle(reject, new TypeError('scan: pick one of into, video, image'));
      return promise;
    }
    activeScan = true;

    // One graceful exit for every mode: whoever aborts — the user's
    // controller or our UI's ✕ — settles `undefined`. Errors are the
    // only rejections. The worker is never touched by an abort.
    const internal = new AbortController();
    const stop = () => {
      if (!internal.signal.aborted) internal.abort();
    };
    internal.signal.addEventListener('abort', () => settle(resolve, undefined), { once: true });
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
      /** @type {HTMLElement | null} */ let progressFill = null;
      /** @type {Scanner | null} */ let scanner;
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
        const close = dialog.appendChild(document.createElement('button'));
        close.type = 'button';
        close.className = 'barcode-ui-close';
        close.textContent = '✕';
        close.setAttribute('aria-label', 'Stop scanning');
        close.addEventListener('click', stop, { once: true });
        dialog.showModal();
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
          internal.signal.removeEventListener('abort', onAbort);
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
        // the torch toggle only exists when the track can actually do it
        if (!video && torchBtn && scanner.hasTorch()) {
          const btn = /** @type {HTMLButtonElement} */ (torchBtn);
          btn.hidden = false;
          let on = false;
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
    const fx = await utils.encode({
      symbology,
      text,
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
