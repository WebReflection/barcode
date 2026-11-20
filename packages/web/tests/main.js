import { Scanner } from "../src/index.js";
import pipelineWorkerUrl from "../src/pipeline-worker.js?worker&url";

const video = /** @type {HTMLVideoElement} */ (document.getElementById("video"));
const maskCanvas = /** @type {HTMLCanvasElement} */ (document.getElementById("mask"));
const statsEl = /** @type {HTMLElement} */ (document.getElementById("stats"));
const startBtn = /** @type {HTMLButtonElement} */ (document.getElementById("start"));
const torchBtn = /** @type {HTMLButtonElement} */ (document.getElementById("torch"));
const lensSel = /** @type {HTMLSelectElement} */ (document.getElementById("lens"));

const ctx = /** @type {CanvasRenderingContext2D} */ (maskCanvas.getContext("2d"));
if (!ctx) throw new Error("2D canvas context unavailable.");

/** @typedef {import("../src/scanner.js").DetectedCode} DetectedCode */
/** @typedef {import("../src/gpu/binarizer.js").BinarizedFrame} BinarizedFrame */

/**
 * Draw the 1bpp mask for visual verification of the GPU pipeline.
 * @param {BinarizedFrame} frame
 */
function drawMask(frame) {
  maskCanvas.width = frame.width;
  maskCanvas.height = frame.height;
  const img = ctx.createImageData(frame.width, frame.height);
  const px = img.data;
  for (let y = 0; y < frame.height; y++) {
    let off = y * frame.bytesPerRow;
    let di = y * frame.width * 4;
    for (let x = 0; x < frame.width; x += 8) {
      const byte = frame.data[off++] ?? 0;
      for (let b = 0; b < 8 && x + b < frame.width; b++) {
        const v = ((byte >> b) & 1) === 1 ? 0 : 255; // ink -> black
        px[di] = px[di + 1] = px[di + 2] = v;
        px[di + 3] = 255;
        di += 4;
      }
    }
  }
  ctx.putImageData(img, 0, 0);
}

/**
 * Outline detected codes in green on the mask canvas.
 * @param {DetectedCode[]} codes
 */
function drawQuads(codes) {
  ctx.strokeStyle = "#00ff66";
  ctx.lineWidth = 2;
  for (const c of codes) {
    ctx.beginPath();
    c.position.forEach((p, i) => {
      if (i === 0) ctx.moveTo(p.x, p.y);
      else ctx.lineTo(p.x, p.y);
    });
    ctx.closePath();
    ctx.stroke();
  }
}

/** @type {DetectedCode[]} */
let codes = [];
/** @type {import("../src/gpu/binarizer.js").BinarizedFrame | null} */
let lastFrame = null;

/**
 * Debug accessor for the remote toolkit: import this page module and call
 * `debugState()` — no window globals involved.
 */
export function debugState() {
  return { frame: lastFrame, results: codes, scanner };
}

const scanner = new Scanner({
  video,
  pipelineWorker: pipelineWorkerUrl,
  lens: /** @type {import("../src/camera.js").LensPreference} */ (lensSel.value),
  binarizer: { windowDivisor: 20, threshold: 0.92, maxSize: 1920 },
  onFrame: (frame) => {
    drawMask(frame);
    lastFrame = frame;
  },
  onResults: (r) => {
    codes = r;
    if (r.length) drawQuads(r);
    console.log("DETECTED", JSON.stringify(r));
  },
  onStats: (s) => {
    statsEl.textContent =
      `${s.fps.toFixed(1)} fps | pipeline ${s.frameMs.toFixed(1)} ms | ` +
      `mask ${s.frameW}x${s.frameH} | zoom ${s.zoom?.toFixed(2) ?? "n/a"}\n` +
      (codes.length
        ? `FOUND: ${codes.map((c) => `${c.symbology} "${c.text}"`).join(", ")}`
        : "");
  },
});

async function start() {
  if (startBtn.disabled) return;
  startBtn.disabled = true;
  try {
    await scanner.start();
    torchBtn.disabled = !scanner.hasTorch();
  } catch (err) {
    statsEl.textContent = `ERROR: ${/** @type {Error} */ (err).message}`;
    startBtn.disabled = false; // allow retry (e.g. camera held by another app)
  }
}

startBtn.addEventListener("click", () => void start());
torchBtn.addEventListener("click", () => void scanner.setTorch(true));
lensSel.addEventListener("change", () => {
  void scanner.setLens(/** @type {import("../src/camera.js").LensPreference} */ (lensSel.value));
});

startBtn.disabled = false;
startBtn.textContent = "start";
