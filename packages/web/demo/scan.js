/**
 * The public-API demo: ONE import, ONE session, ONE worker.
 *
 *   import barcode from "@webreflection/barcode/main";
 *   import "@webreflection/barcode/css";
 *
 *   const { scan, render } = barcode();
 *   const code = await scan();      // SDK dialog: camera, torch, ✕
 *   const svg = await render(code);  // the SAME worker
 *
 * There is nothing else to wire: `main.js` boots the bound `worker.js`
 * (decode + encode) and both `scan()` and `render()` go through it. If the
 * app already owns a worker that imports `worker.js` (alongside its own
 * work), pass it once with `barcode({ worker })` instead.
 *
 * The diagnostics (fps, binarized mask, zoom/focus) deliberately live on
 * the dev page (index.html), not here: a demo should teach the API users
 * get, not our internals.
 *
 * The page keeps only URL escape hatches: ?formats=, ?accuracy=high,
 * ?zoom= and ?torch=1 (used by tools/e2e/fakecam.mjs too).
 */
import barcode from "../api/main.js";
import "../css.css";
// The bound worker's URL. The published main.js resolves "./worker.js" next
// to itself at RUN time (the dist/ files are the contract); a bundler
// build must hand it the emitted worker chunk instead, or the page would
// look for a non-existent assets/worker.js and silently never decode.
import workerUrl from "../api/worker.js?worker&url";

const startBtn = /** @type {HTMLButtonElement} */ (document.getElementById("start"));
const statusEl = /** @type {HTMLElement} */ (document.getElementById("status"));
const resultEl = /** @type {HTMLElement} */ (document.getElementById("result"));
const symbologyEl = /** @type {HTMLElement} */ (document.getElementById("symbology"));
const valueEl = /** @type {HTMLElement} */ (document.getElementById("value"));
const againBtn = /** @type {HTMLButtonElement} */ (document.getElementById("again"));
const resultSvgEl = /** @type {HTMLElement} */ (document.getElementById("result-svg"));

/** ONE session per page/SPA: it owns the worker for the page's lifetime. */
const { scan, render } = barcode({ lens: "wide", worker: workerUrl });

// The fake-camera e2e renders its specimens through the page's own encoder:
// this is the same `render` consumers use, on the same single worker.
(/** @type {any} */ (globalThis)).__barcodeRender = (
  /** @type {{ type: string, value: string }} */ { type, value },
) => render({ symbology: type, text: value });

/** @param {string} msg @param {boolean} [isError] */
function setStatus(msg, isError = false) {
  statusEl.textContent = msg;
  statusEl.classList.toggle("error", isError);
}

/** @param {Error} err */
function showError(err) {
  setStatus(humanError(err), true);
  startBtn.style.display = "";
  startBtn.disabled = false;
  startBtn.textContent = "Try again";
}

/** @param {Error} err */
function humanError(err) {
  const msg = err.message || String(err);
  if (/NotFoundError|Requested device not found/i.test(msg)) {
    return "No camera found on this device.";
  }
  if (/NotAllowedError|Permission/i.test(msg)) {
    return "Camera permission was denied. Allow camera access and try again.";
  }
  if (/NotReadableError|Could not start video/i.test(msg)) {
    return "The camera is in use by another app. Close it and try again.";
  }
  if (/secure|https/i.test(msg)) {
    return "Camera access requires HTTPS (or localhost).";
  }
  return msg;
}

/** @param {import("../main.d.ts").Code} code */
function showResult(code) {
  symbologyEl.textContent = code.symbology;
  valueEl.textContent = code.text;
  resultSvgEl.replaceChildren();
  resultEl.style.display = "block";
  againBtn.style.display = "inline-block";
  setStatus("");
  // Re-present the found code as a clean SVG through the SAME worker.
  // The value is already visible; the SVG lands when the worker answers.
  void render(code)
    .then((node) => {
      if (node) resultSvgEl.replaceChildren(node);
    })
    .catch(() => {}); // unsupported symbology: value-only view is fine
}

/** One `scan()`: the SDK dialog is the viewfinder (camera, torch, ✕). */
async function start() {
  const params = new URLSearchParams(location.search);
  const formats = (params.get("formats") ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  const accuracy = params.get("accuracy") === "high" ? "high" : "auto";
  const zoom = params.get("zoom") ? Number(params.get("zoom")) : undefined;
  const torch = params.get("torch") === "1" ? true : undefined;

  startBtn.disabled = true;
  startBtn.textContent = "Starting…";
  resultEl.style.display = "none";
  resultSvgEl.replaceChildren();
  againBtn.style.display = "none";
  setStatus("");
  try {
    const code = await scan({
      ...(formats.length ? { type: formats } : {}),
      accuracy,
      ...(zoom != null && Number.isFinite(zoom) ? { zoom } : {}),
      ...(torch ? { torch } : {}),
    });
    if (!code) {
      // graceful stop (the dialog's ✕): back to the start button
      startBtn.style.display = "";
      startBtn.disabled = false;
      startBtn.textContent = "Scan a barcode";
      return;
    }
    startBtn.disabled = false;
    startBtn.style.display = "none";
    showResult(code);
  } catch (err) {
    showError(/** @type {Error} */ (err));
  }
}

startBtn.addEventListener("click", () => void start());

againBtn.addEventListener("click", () => {
  resultEl.style.display = "none";
  resultSvgEl.replaceChildren();
  againBtn.style.display = "none";
  // Another scan on the SAME session: the worker is already warm, there is
  // no worker spawn and no wasm init here.
  void start();
});
