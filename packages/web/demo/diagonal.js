/**
 * Diagonal gate (roadmap item 4): a 2D symbol presented at 45/135/225/315°
 * through the PUBLIC still-image path. The wasm mask-level diagonal pass
 * cannot restore Aztec's bullseye from an already-resampled mask; the
 * worker's source-rotation retry (rotate the bitmap with antialiasing,
 * binarize again, decode) must.
 *
 * Same table + title contract as test.html / tilt.html:
 *   node tools/run-tests.mjs 9225 "http://localhost:5173/diagonal.html"
 */

import barcode from "../api/main.js";

const { scan, render } = barcode();

/** One representative payload per diagonal-sensitive family. */
const FAMILIES = /** @type {Array<[string, string]>} */ ([
  ["datamatrix", "DM-DIAG-001"],
  ["aztec", "AZTEC-DIAG-001"],
  ["pdf417", "PDF417-DIAG-001"],
]);

// 225/315 exercise the -45° arm; 45/135 the +45° arm.
const ANGLES = [0, 45, 135, 225, 315];

/**
 * Draw the SVG rotated by `deg` about its centre on a white canvas padded
 * to the rotated bounding box, with the same high-quality antialiasing a
 * camera frame has.
 * @param {SVGSVGElement} svg
 * @param {number} deg
 */
function rotatedCanvas(svg, deg) {
  const vb = svg.viewBox.baseVal;
  const scale = Math.max(2, Math.floor(900 / Math.max(vb.width, vb.height)));
  const w0 = vb.width * scale;
  const h0 = vb.height * scale;
  const rad = (deg * Math.PI) / 180;
  const bw = Math.ceil(Math.abs(w0 * Math.cos(rad)) + Math.abs(h0 * Math.sin(rad)));
  const bh = Math.ceil(Math.abs(w0 * Math.sin(rad)) + Math.abs(h0 * Math.cos(rad)));
  const canvas = document.createElement("canvas");
  canvas.width = bw + 40;
  canvas.height = bh + 40;
  const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  const img = new Image();
  img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg.outerHTML);
  return img.decode().then(() => {
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.translate(canvas.width / 2, canvas.height / 2);
    ctx.rotate(rad);
    ctx.drawImage(img, -w0 / 2, -h0 / 2, w0, h0);
    return canvas;
  });
}

/** @param {{sym: string, text: string, deg: number, pass: boolean, got: string, frame: string, ms: number}} row */
function renderRow(row) {
  const tr = document.createElement("tr");
  tr.className = row.pass ? "pass" : "fail";
  tr.innerHTML = `<td>${row.sym}</td><td>${row.text}</td><td>${row.deg}°</td>` +
    `<td>${row.pass ? "PASS" : "FAIL"}</td><td>${row.got}</td><td>${row.frame}</td><td>${row.ms}ms</td>`;
  document.querySelector("tbody")?.append(tr);
}

async function run() {
  const rows = [];
  for (const [sym, text] of FAMILIES) {
    const svg = await render({ symbology: sym, text });
    if (!svg) throw new Error(`${sym}: render returned nothing`);
    for (const deg of ANGLES) {
      const t0 = performance.now();
      let row;
      try {
        const canvas = await rotatedCanvas(svg, deg);
        const code = await scan({ image: canvas, type: [sym], latency: 15000 });
        row = {
          sym, text, deg,
          pass: code?.text === text,
          got: code?.text ?? "(none)",
          frame: canvas.width + "x" + canvas.height,
          ms: Math.round(performance.now() - t0),
        };
      } catch (err) {
        row = {
          sym, text, deg, pass: false,
          got: `ERROR: ${/** @type {Error} */ (err)}`,
          frame: "-", ms: Math.round(performance.now() - t0),
        };
      }
      rows.push(row);
      renderRow(row);
    }
  }
  const fails = rows.filter((r) => !r.pass);
  const summary = /** @type {HTMLElement} */ (document.getElementById("summary"));
  summary.textContent = fails.length
    ? `${fails.length}/${rows.length} FAILED — ${[...new Set(fails.map((f) => `${f.sym}@${f.deg}°`))].join(", ")}`
    : `ALL ${rows.length} PASS (source-rotation diagonals)`;
  summary.className = fails.length ? "fail" : "pass";
  document.title = fails.length ? "TESTS FAILED" : "TESTS PASS";
}

void run();
