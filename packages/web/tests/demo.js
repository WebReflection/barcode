/**
 * Live demo — the public scan()/render() surface only.
 *
 *   scan({ into })   → camera + default UI, resolves once with a Code
 *   scan({ image })  → still image (file picker / camera roll)
 *   render(code)     → SVGSVGElement re-encoding the value
 *
 * Mode filtering is the single biggest performance knob: "qr" reads in
 * single-digit milliseconds per frame, "everything" also runs the exotic
 * 2D families and the ten 1D decoders on the same frame.
 */

import barcode from "../api/main.js";
import cssHref from "../css.css?url";

/** @typedef {"qr" | "barcode" | "datamatrix" | "pdf417" | "all"} Mode */
/** @typedef {"ultrawide" | "wide" | "default"} Lens */
/** @typedef {{ symbology: string, text: string, raw: Uint8Array, position: Array<{x: number, y: number}>, score?: number }} Code */

const MODES = {
  qr: { type: "qr", hint: "QR & Micro QR — the fast path." },
  barcode: { type: "barcode", hint: "The ten 1D symbologies." },
  datamatrix: { type: ["datamatrix"], hint: "Invoice DataMatrix (DMRE) — decoded on every frame." },
  pdf417: { type: ["pdf417"], hint: "PDF417 (boarding passes, tickets) — decoded on every frame." },
  all: { type: undefined, hint: "All 17 symbologies — the exotic tiers alternate." },
};

const { scan, render } = barcode();

// The default UI's stylesheet (the demo links it like a consumer would).
document.head.append(Object.assign(document.createElement("link"), {
  rel: "stylesheet",
  href: cssHref,
}));

/**
 * @param {string} id
 * @returns {HTMLElement}
 */
const el = (id) => /** @type {HTMLElement} */ (document.getElementById(id));

const scanBtn = /** @type {HTMLButtonElement} */ (el("scan"));
const fileInput = /** @type {HTMLInputElement} */ (el("file"));
const lensSelect = /** @type {HTMLSelectElement} */ (el("lens"));
const statusEl = el("status");
const resultEl = el("result");
const symEl = el("sym");
const msEl = el("ms");
const svgBox = el("svg-box");
const textEl = /** @type {HTMLElement} */ (el("text"));
const copyBtn = /** @type {HTMLButtonElement} */ (el("copy"));
const againBtn = /** @type {HTMLButtonElement} */ (el("again"));
const historyEl = el("history");
const samplesEl = el("samples");
const insecureEl = el("insecure");
const bigDialog = /** @type {HTMLDialogElement} */ (el("big"));
const bigSvg = el("big-svg");
const bigLabel = el("big-label");

/** @type {Mode} */
// "Everything" is the default: the confirmation law + tier scheduling make
// it usable on live frames, and a user who never picks a mode should not
// silently lose 12 symbologies. A stored choice still wins.
let mode = "all";
/** @type {string} */
let lens = "";
/** @type {boolean} */
let busy = false;

// --- persisted preferences (mode + lens are per-user choices) --------------

try {
  const storedMode = localStorage.getItem("barcode-demo-mode");
  if (storedMode === "qr" || storedMode === "barcode" || storedMode === "datamatrix" || storedMode === "pdf417" || storedMode === "all") mode = storedMode;
  lens = localStorage.getItem("barcode-demo-lens") ?? "";
} catch { /* storage can be blocked — defaults are fine */ }
lensSelect.value = lens;

/** @param {Mode} next */
function setMode(next) {
  mode = next;
  try { localStorage.setItem("barcode-demo-mode", next); } catch { /* ignore */ }
  for (const node of document.querySelectorAll("[data-mode]")) {
    const btn = /** @type {HTMLButtonElement} */ (node);
    btn.setAttribute("aria-pressed", String(btn.dataset.mode === next));
  }
  if (!busy) setStatus(MODES[next].hint);
}

for (const node of document.querySelectorAll("[data-mode]")) {
  const btn = /** @type {HTMLButtonElement} */ (node);
  btn.addEventListener("click", () => {
    if (btn.dataset.mode === "qr" || btn.dataset.mode === "barcode" || btn.dataset.mode === "datamatrix" || btn.dataset.mode === "pdf417" || btn.dataset.mode === "all") {
      setMode(btn.dataset.mode);
    }
  });
}
setMode(mode);

lensSelect.addEventListener("change", () => {
  lens = lensSelect.value;
  try { localStorage.setItem("barcode-demo-lens", lens); } catch { /* ignore */ }
});

// --- status / errors ------------------------------------------------------

/**
 * @param {string} message
 * @param {"ok" | "err" | ""} [kind]
 */
function setStatus(message, kind = "") {
  statusEl.textContent = message;
  if (kind) statusEl.dataset.kind = kind;
  else delete statusEl.dataset.kind;
}

/** @param {unknown} err */
const messageOf = (err) => (err instanceof Error ? err.message : String(err));

/** @param {boolean} on */
function setBusy(on) {
  busy = on;
  scanBtn.disabled = on;
  scanBtn.textContent = on ? "Scanning…" : "Scan with camera";
}

// --- results --------------------------------------------------------------

/** @type {Array<{ code: Code, ms: number }>} */
const history = [];

/**
 * @param {Code} code
 * @param {number} ms
 */
async function showResult(code, ms) {
  const rounded = Math.round(ms);
  resultEl.hidden = false;
  symEl.textContent = code.symbology;
  msEl.textContent = `${rounded} ms`;
  textEl.textContent = code.text;
  svgBox.replaceChildren();
  setStatus(`Read ${code.symbology} in ${rounded} ms.`, "ok");
  pushHistory(code, rounded);
  resultEl.scrollIntoView({ block: "nearest", behavior: "smooth" });

  try {
    const svg = await render(code);
    if (svg) {
      svg.style.setProperty("--barcode-ink", "#0b0d10");
      svg.style.setProperty("--barcode-paper", "#ffffff");
      svgBox.append(svg);
    }
  } catch (err) {
    // A read must not become an error just because re-encoding failed.
    console.warn("render failed:", err);
  }
}

/**
 * @param {Code} code
 * @param {number} ms
 */
function pushHistory(code, ms) {
  history.unshift({ code, ms });
  history.length = Math.min(history.length, 8);

  const title = document.createElement("h2");
  title.textContent = "Recent reads";
  historyEl.replaceChildren(title);

  for (const entry of history) {
    const row = document.createElement("div");
    row.className = "read";
    const sym = document.createElement("span");
    sym.className = "sym";
    sym.textContent = entry.code.symbology;
    const val = document.createElement("span");
    val.className = "val";
    val.textContent = entry.code.text;
    const time = document.createElement("span");
    time.className = "ms";
    time.textContent = `${entry.ms} ms`;
    row.append(sym, val, time);
    historyEl.append(row);
  }
}

// --- scanning -------------------------------------------------------------

async function cameraScan() {
  if (busy) return;
  setBusy(true);
  setStatus("Opening the camera…");
  const t0 = performance.now();
  try {
    const code = await scan({
      type: MODES[mode].type,
      lens: lens === "" ? undefined : /** @type {Lens} */ (lens),
    });
    if (!code) {
      setStatus("No read — scan cancelled.");
      return;
    }
    await showResult(code, performance.now() - t0);
  } catch (err) {
    setStatus(`Camera unavailable: ${messageOf(err)}`, "err");
  } finally {
    setBusy(false);
  }
}

scanBtn.addEventListener("click", () => { void cameraScan(); });

againBtn.addEventListener("click", () => {
  resultEl.hidden = true;
  void cameraScan();
});

fileInput.addEventListener("change", async () => {
  const file = fileInput.files?.[0];
  if (!file || busy) return;
  setBusy(true);
  setStatus(`Decoding ${file.name || "photo"}…`);
  const t0 = performance.now();
  try {
    const code = await scan({ image: file, type: MODES[mode].type });
    if (code) await showResult(code, performance.now() - t0);
    else setStatus("No code found in that photo.", "err");
  } catch (err) {
    setStatus(`Photo decode failed: ${messageOf(err)}`, "err");
  } finally {
    fileInput.value = "";
    setBusy(false);
  }
});

copyBtn.addEventListener("click", async () => {
  const value = textEl.textContent ?? "";
  let ok = false;
  try {
    await navigator.clipboard.writeText(value);
    ok = true;
  } catch {
    // insecure origins have no clipboard API — fall back to a range copy
    const range = document.createRange();
    range.selectNodeContents(textEl);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    try { ok = document.execCommand("copy"); } catch { ok = false; }
    selection?.removeAllRanges();
  }
  copyBtn.textContent = ok ? "Copied" : "Copy failed";
  setTimeout(() => { copyBtn.textContent = "Copy"; }, 1400);
});

// --- camera availability banner ------------------------------------------

const cameraReady = window.isSecureContext && !!navigator.mediaDevices?.getUserMedia;
if (!cameraReady) {
  insecureEl.hidden = false;
  scanBtn.disabled = true;
  scanBtn.textContent = "Camera needs HTTPS or localhost";
  setStatus("Photo scanning still works — use the photo button.", "err");
}

// --- self-contained fixtures (the encoder half of the SDK) ----------------

/** @type {Array<[string, { symbology: string, text: string }]>} */
const SAMPLES = [
  ["QR Code", { symbology: "qrcode", text: "https://webreflection.github.io/barcode/" }],
  ["Data Matrix", { symbology: "datamatrix", text: "BARCODE-WEB" }],
  ["Aztec", { symbology: "aztec", text: "AZTEC DEMO" }],
  ["PDF417", { symbology: "pdf417", text: "PDF417 DEMO" }],
  ["EAN-13", { symbology: "ean13", text: "4006381333931" }],
  ["Code 128", { symbology: "code128", text: "CODE128-DEMO" }],
];

/**
 * @param {SVGSVGElement} svg
 * @param {string} label
 * @param {string} value
 */
function enlarge(svg, label, value) {
  bigSvg.replaceChildren(svg.cloneNode(true));
  bigLabel.textContent = `${label} — ${value}`;
  bigDialog.showModal();
}

for (const [label, input] of SAMPLES) {
  try {
    const svg = await render(input);
    if (!svg) continue;
    const card = document.createElement("button");
    card.type = "button";
    card.className = "sample";
    card.setAttribute("aria-label", `Enlarge ${label} sample`);
    const holder = document.createElement("div");
    holder.append(svg);
    const caption = document.createElement("span");
    caption.textContent = label;
    card.append(holder, caption);
    card.addEventListener("click", () => enlarge(svg, label, input.text));
    samplesEl.append(card);
  } catch (err) {
    console.warn(`sample ${label} failed:`, err);
  }
}
