/**
 * scan({ image }) playground: a dropped/picked file or a generated
 * sample, decoded by the same session/worker as the camera demo.
 */
import barcode from "../api/main.js";
// The same-origin worker shim (see worker-url.js).
import workerUrl from "./worker-url.js";

const { scan, render } = barcode({ worker: workerUrl });

/** ?type=datamatrix (or any canonical name / comma list) pins the scan. */
const type = new URLSearchParams(location.search).get("type");
/** ?latency=ms overrides the still budget (regression/debug runs). */
const latency = Number(new URLSearchParams(location.search).get("latency")) || undefined;

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));
const drop = $("drop");
const fileInput = /** @type {HTMLInputElement} */ (document.getElementById("file"));
const statusEl = $("status");
const previewEl = $("preview");
const resultEl = $("result");
const symEl = $("sym");
const valueEl = $("value");
const metaEl = $("meta");

/** @param {string} message @param {"" | "error" | "pass"} [kind] */
function status(message, kind = "") {
  statusEl.textContent = message;
  statusEl.className = `status ${kind}`;
}

function resetResult() {
  resultEl.classList.add("hidden");
  symEl.textContent = "";
  valueEl.textContent = "";
  metaEl.textContent = "";
}

/** @param {import("../main.d.ts").Code} code @param {number} ms */
function showResult(code, ms) {
  resultEl.classList.remove("hidden");
  symEl.textContent = code.symbology;
  valueEl.textContent = code.text || `(binary: ${code.raw?.length ?? 0} bytes)`;
  metaEl.textContent = [
    `${ms}ms`,
    code.raw instanceof Uint8Array ? `${code.raw.length} payload bytes` : "",
    typeof code.score === "number" ? `score ${code.score.toFixed(3)}` : "",
  ].filter(Boolean).join(" · ");
}

/**
 * @param {Blob | HTMLCanvasElement | ImageBitmap | HTMLImageElement | SVGSVGElement} input
 * @param {number} t0
 */
async function runScan(input, t0 = performance.now()) {
  status("Scanning…");
  resetResult();
  try {
    const code = await scan({
      image: input,
      ...(type ? { type: type.split(",").map((name) => name.trim()).filter(Boolean) } : {}),
      ...(latency ? { latency } : {}),
    });
    const ms = Math.round(performance.now() - t0);
    if (!code) {
      status(`No code found (${ms}ms). Try a sharper or larger image.`, "error");
      return;
    }
    showResult(code, ms);
    status(`Read ${code.symbology} in ${ms}ms.`, "pass");
    // Re-encode the payload as a clean SVG: scan → render, same worker.
    try {
      const svg = await render(code);
      if (svg) {
        previewEl.replaceChildren(svg);
        const label = document.createElement("div");
        label.className = "note";
        label.textContent = "Re-rendered from the decoded payload:";
        label.style.cssText = "margin-bottom:8px;color:#93a4b5;font-size:12px";
        const box = document.createElement("div");
        box.style.cssText = "width:100%;display:flex;flex-direction:column;align-items:center";
        box.append(label, svg);
        previewEl.replaceChildren(box);
      }
    } catch { /* preview is optional */ }
  } catch (err) {
    status(/** @type {Error} */ (err).message || String(err), "error");
  }
}

/** @param {File} file */
async function scanFile(file) {
  if (!file.type.startsWith("image/")) {
    status(`Not an image file: ${file.type || "unknown"}`, "error");
    return;
  }
  const url = URL.createObjectURL(file);
  const img = new Image();
  img.src = url;
  try {
    await img.decode();
    previewEl.replaceChildren(img);
    status(`Loaded ${file.name} (${Math.round(file.size / 1024)} KB)…`);
    await runScan(file);
  } finally {
    URL.revokeObjectURL(url);
  }
}

/**
 * @param {SVGSVGElement} svg
 */
async function rasterize(svg) {
  const source =
    "data:image/svg+xml;charset=utf-8," +
    encodeURIComponent(new XMLSerializer().serializeToString(svg));
  const img = new Image();
  img.src = source;
  await img.decode();
  const scale = Math.min(4, 1000 / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(img.naturalWidth * scale)) + 80;
  canvas.height = Math.max(1, Math.round(img.naturalHeight * scale)) + 80;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas 2d unavailable");
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(img, 40, 40, canvas.width - 80, canvas.height - 80);
  return canvas;
}

/**
 * @param {string} symbology
 * @param {string} text
 */
async function sample(symbology, text) {
  status("Generating a sample…");
  resetResult();
  try {
    const svg = await render({ symbology, text });
    if (!svg) throw new Error("render returned null");
    previewEl.replaceChildren(svg);
    const canvas = await rasterize(svg);
    await runScan(canvas);
  } catch (err) {
    status(/** @type {Error} */ (err).message || String(err), "error");
  }
}

drop.addEventListener("click", () => fileInput.click());
drop.addEventListener("keydown", (event) => {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    fileInput.click();
  }
});
fileInput.addEventListener("change", () => {
  const file = fileInput.files?.[0];
  if (file) scanFile(file);
});
for (const type of ["dragenter", "dragover"]) {
  drop.addEventListener(type, (event) => {
    event.preventDefault();
    drop.classList.add("drag");
  });
}
for (const type of ["dragleave", "drop"]) {
  drop.addEventListener(type, (event) => {
    event.preventDefault();
    drop.classList.remove("drag");
  });
}
drop.addEventListener("drop", (event) => {
  const file = /** @type {DragEvent} */ (event).dataTransfer?.files?.[0];
  if (file) scanFile(file);
});

document.getElementById("sample-qr")?.addEventListener("click", () =>
  sample("qrcode", "https://example.com/scanned-from-a-photo"));
document.getElementById("sample-c128")?.addEventListener("click", () =>
  sample("code128", "STILL-DEMO-001"));
document.getElementById("sample-dm")?.addEventListener("click", () =>
  sample("datamatrix", "DM-STILL-001"));
