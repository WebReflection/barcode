/**
 * Native-resolution ROI gate (roadmap item 8).
 *
 * A 4096x3072 source is downscaled by the still binarizer (2112 cap).
 * The specimens here are deliberately at a size that the full-frame
 * mask cannot decode but the ORIGINAL bitmap can: the worker must find
 * their compact ink clusters, crop them at native pitch and decode.
 *
 * Same table + title contract as test.html / diagonal.html:
 *   node tools/run-tests.mjs 9225 "http://localhost:5173/roi.html"
 */

import barcode from "../api/main.js";

const { scan, render } = barcode();

/** @type {Array<[string, string, [number, number], [number, number], boolean?]>} */
const CASES = [
  // 130px on a 4096px frame: ~1.5px/module after the 2112 downscale.
  ["qrcode", "SMALL-QR-ROI", [800, 900], [130, 130]],
  // 340px wide, 24 digits: the full-frame 2112 mask merges the bars.
  ["code128", "108717112609540270746300", [800, 1800], [340, 55]],
  // White-on-black on a WHITE page: the code's black block is ~0.1% of
  // the frame, so neither the full-frame ink fraction nor the ROI crop's
  // (black block + light margin) can pick the inverted polarity from ink
  // alone. The crop flip retry must read it.
  ["qrcode", "INVERTED-QR-ROI", [900, 900], [150, 150], true],
];

/**
 * Render one specimen at an explicit pixel size on the big source canvas.
 * @param {string} markup SVG source (already recolored if inverted)
 * @param {number} w @param {number} h
 * @param {number} x @param {number} y
 */
async function paste(markup, w, h, x, y) {
  const canvas = document.createElement("canvas");
  canvas.width = 4096;
  canvas.height = 3072;
  const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  const img = new Image();
  img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(markup);
  await img.decode();
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(img, x, y, w, h);
  return canvas;
}

/** @param {{sym: string, text: string, got: string, pass: boolean, frame: string, ms: number}} row */
function renderRow(row) {
  const tr = document.createElement("tr");
  tr.className = row.pass ? "pass" : "fail";
  tr.innerHTML = `<td>${row.sym}</td><td>${row.text}</td><td>${row.got}</td>` +
    `<td>${row.pass ? "PASS" : "FAIL"}</td><td>${row.frame}</td><td>${row.ms}ms</td>`;
  document.querySelector("tbody")?.append(tr);
}

async function run() {
  /** @type {Array<{sym: string, text: string, got: string, pass: boolean, frame: string, ms: number}>} */
  const rows = [];
  for (const [sym, text, at, size, inverted] of CASES) {
    const t0 = performance.now();
    let row;
    try {
      const svg = await render({ symbology: sym, text });
      if (!svg) throw new Error(`${sym}: render returned nothing`);
      const markup = inverted
        ? svg.outerHTML
            .replaceAll("var(--barcode-ink, #000)", "#fff")
            .replaceAll("var(--barcode-paper, #fff)", "#000")
        : svg.outerHTML;
      const canvas = await paste(markup, size[0], size[1], at[0], at[1]);
      const code = await scan({ image: canvas, type: [sym], latency: 15000 });
      row = {
        sym,
        text,
        got: code?.text ?? "(none)",
        pass: code?.text === text,
        frame: `${canvas.width}x${canvas.height}`,
        ms: Math.round(performance.now() - t0),
      };
    } catch (err) {
      row = {
        sym,
        text,
        got: `ERROR: ${/** @type {Error} */ (err)}`,
        pass: false,
        frame: "-",
        ms: Math.round(performance.now() - t0),
      };
    }
    rows.push(row);
    renderRow(row);
  }
  const fails = rows.filter((r) => !r.pass);
  const summary = /** @type {HTMLElement} */ (document.getElementById("summary"));
  summary.textContent = fails.length
    ? `${fails.length}/${rows.length} FAILED — ${fails.map((f) => f.sym).join(", ")}`
    : `ALL ${rows.length} PASS (native-resolution ROI)`;
  summary.className = fails.length ? "fail" : "pass";
  document.title = fails.length ? "TESTS FAILED" : "TESTS PASS";
}

void run();
