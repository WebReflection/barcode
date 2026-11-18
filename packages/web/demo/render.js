/**
 * One-click render() playground: every encoder option, then a real
 * scan(render(...)) round-trip on the same session/worker.
 */
import barcode from "../api/main.js";
// Vite emits the worker as its own chunk; the source default URL would
// point at an unbundled api/worker.js asset in a production build.
import workerUrl from "../api/worker.js?worker&url";

const { render, scan } = barcode({ worker: workerUrl });

/** @type {Record<string, string>} */
const SAMPLES = {
  qrcode: "https://example.com/barcode?hello=world",
  microqr: "HELLO",
  datamatrix: "DM-DOCS-001",
  aztec: "AZTEC-DOCS-001",
  pdf417: "PDF417-DOCS-001",
  ean13: "4006381333931",
  ean8: "96385074",
  upca: "036000291452",
  upce: "042100005264",
  code39: "ABC-123",
  code93: "TEST93",
  code128: "CODE128-DOCS",
  itf: "12345670",
  codabar: "40156",
  databar: "0123456789012",
  databarltd: "0123456789012",
  databarstk: "0123456789012",
};

/** @type {[string, string][]} */
const SYMBOLOGIES = [
  ["qrcode", "QR Code"],
  ["microqr", "Micro QR"],
  ["datamatrix", "Data Matrix"],
  ["aztec", "Aztec"],
  ["pdf417", "PDF417"],
  ["ean13", "EAN-13"],
  ["ean8", "EAN-8"],
  ["upca", "UPC-A"],
  ["upce", "UPC-E"],
  ["code39", "Code 39"],
  ["code93", "Code 93"],
  ["code128", "Code 128"],
  ["itf", "ITF (Interleaved 2 of 5)"],
  ["codabar", "Codabar"],
  ["databar", "GS1 DataBar Omni"],
  ["databarltd", "GS1 DataBar Limited"],
  ["databarstk", "GS1 DataBar Stacked"],
];

const $ = (/** @type {string} */ id) =>
  /** @type {HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement} */ (
    document.getElementById(id)
  );

const select = /** @type {HTMLSelectElement} */ (document.getElementById("symbology"));
for (const [value, label] of SYMBOLOGIES) {
  const option = document.createElement("option");
  option.value = value;
  option.textContent = label;
  select.append(option);
}

const statusEl = /** @type {HTMLElement} */ (document.getElementById("status"));
const previewEl = /** @type {HTMLElement} */ (document.getElementById("preview"));
const scanBtn = /** @type {HTMLButtonElement} */ (document.getElementById("scanback"));
const downloadBtn = /** @type {HTMLButtonElement} */ (document.getElementById("download"));

/** @param {string} message @param {"" | "error" | "pass"} [kind] */
function status(message, kind = "") {
  statusEl.textContent = message;
  statusEl.className = `status ${kind}`;
}

const num = (/** @type {string} */ id) => {
  const value = $(id).value.trim();
  return value === "" ? null : Number(value);
};

function options() {
  /** @type {import("../main.d.ts").RenderOptions} */
  const opts = { module: num("module") ?? 4 };
  const height = num("height");
  if (height != null) opts.height = height;

  const sym = select.value;
  if (sym === "qrcode") {
    const ec = /** @type {HTMLSelectElement} */ ($("ec")).value;
    if (ec) opts.ec = /** @type {"L" | "M" | "Q" | "H"} */ (ec);
    const mask = num("mask");
    if (mask != null) opts.mask = mask;
    const version = num("version");
    if (version != null) opts.version = version;
  }
  if (sym === "pdf417") {
    const level = num("level");
    if (level != null) opts.level = level;
    const columns = num("columns");
    if (columns != null) opts.columns = columns;
  }
  if (sym === "datamatrix") {
    const rows = num("rows");
    const cols = num("cols");
    if (rows != null || cols != null) {
      if (rows == null || cols == null) {
        throw new TypeError("Data Matrix rows and cols go together");
      }
      opts.rows = rows;
      opts.cols = cols;
    }
  }
  return opts;
}

/** @param {string} sym */
function syncFields(sym) {
  document.getElementById("qr-opts")?.classList.toggle("hidden", sym !== "qrcode");
  document.getElementById("pdf-opts")?.classList.toggle("hidden", sym !== "pdf417");
  document.getElementById("dm-opts")?.classList.toggle("hidden", sym !== "datamatrix");
}

/** @type {SVGSVGElement | null} */
let current = null;
let timer = 0;

async function update() {
  const sym = select.value;
  syncFields(sym);
  const text = $("text").value;
  if (!text) {
    previewEl.replaceChildren();
    current = null;
    status("Type a payload to render.");
    return;
  }
  try {
    const svg = await render({ symbology: sym, text }, options());
    if (!svg) throw new Error("render returned null");
    current = svg;
    previewEl.replaceChildren(svg);
    status(`${sym} · ${svg.viewBox.baseVal.width}×${svg.viewBox.baseVal.height} viewBox units`);
  } catch (err) {
    current = null;
    previewEl.replaceChildren();
    status(/** @type {Error} */ (err).message || String(err), "error");
  }
}

function schedule() {
  clearTimeout(timer);
  timer = setTimeout(update, 120);
}

for (const id of ["module", "height", "ec", "mask", "version", "level", "columns", "rows", "cols"]) {
  $(id).addEventListener("input", schedule);
}
$("text").addEventListener("input", schedule);
select.addEventListener("change", () => {
  const sample = SAMPLES[select.value];
  if (sample) $("text").value = sample;
  syncFields(select.value);
  schedule();
});

document.getElementById("sample")?.addEventListener("click", () => {
  $("text").value = SAMPLES[select.value] ?? "";
  schedule();
});

/**
 * Rasterize an SVG element on a generous white canvas.
 * @param {SVGSVGElement} svg
 */
async function rasterize(svg) {
  const source =
    "data:image/svg+xml;charset=utf-8," +
    encodeURIComponent(new XMLSerializer().serializeToString(svg));
  const img = new Image();
  img.src = source;
  await img.decode();
  const scale = Math.min(3, 900 / Math.max(img.naturalWidth, img.naturalHeight));
  const w = Math.max(1, Math.round(img.naturalWidth * scale));
  const h = Math.max(1, Math.round(img.naturalHeight * scale));
  const canvas = document.createElement("canvas");
  canvas.width = w + 80;
  canvas.height = h + 80;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas 2d unavailable");
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(img, 40, 40, w, h);
  return canvas;
}

scanBtn.addEventListener("click", async () => {
  if (!current) return;
  scanBtn.disabled = true;
  status("Scanning the rendered SVG back…");
  const t0 = performance.now();
  try {
    const code = await scan({ image: await rasterize(current) });
    const ms = Math.round(performance.now() - t0);
    if (!code) {
      status(`No read (${ms}ms) — try a larger module or a simpler payload.`, "error");
    } else if (code.text === $("text").value) {
      status(`Round-trip OK · ${code.symbology} · ${ms}ms`, "pass");
    } else {
      status(`Read a different value (${ms}ms): ${JSON.stringify(code.text)}`, "error");
    }
  } catch (err) {
    status(/** @type {Error} */ (err).message || String(err), "error");
  } finally {
    scanBtn.disabled = false;
  }
});

downloadBtn.addEventListener("click", () => {
  if (!current) return;
  const blob = new Blob([new XMLSerializer().serializeToString(current)], {
    type: "image/svg+xml",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `${select.value}.svg`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});

select.value = "qrcode";
$("text").value = SAMPLES.qrcode ?? "";
syncFields("qrcode");
update();
