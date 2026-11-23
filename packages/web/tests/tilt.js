/**
 * Tilt harness: users do not hold the phone perfectly perpendicular to the
 * code. Each symbology family's specimen is re-rendered ROTATED by a few
 * degrees (in-plane roll — what a wrist actually does) and pushed through
 * the same GPU-binarizer -> WASM pipeline, asserting the decode survives.
 *
 * The rotation is drawn with antialiasing on, so edges carry realistic
 * gray ramps exactly like a camera frame would.
 *
 * Same table + title contract as test.html: node tools/run-tests.mjs
 * <port> "http://localhost:5173/tilt.html" drives it headless.
 */

import { decodeFrame, getWasmScanner } from "../src/decoder.js";
import { createBinarizer } from "../src/binarizer-factory.js";
import { encode_fixture_opts } from "../src/wasm/pkg/barcode_wasm.js";

// One representative payload per family (full per-payload coverage lives
// in test.html; this page answers "does a tilted X still read?").
/** @type {Array<[string, string]>} */
const ALL_FAMILIES = [
  ["ean13", "4006381333931"],
  ["code128", "CODE128"],
  ["itf", "12345670"],
  ["codabar", "40156"],
  ["qrcode", "HELLO WORLD"],
  ["microqr", "HELLO"],
  ["datamatrix", "DM-PHYS-001"],
  ["aztec", "AZTEC-PHYS-001"],
  ["pdf417", "PDF417-PHYS-001"],
];

// ?families=datamatrix,aztec,pdf417 narrows the bank (the diagonal
// angles only make sense for the 2D families; 1D bars cannot read at
// 45° by construction).
const ONLY = (new URLSearchParams(location.search).get("families") ?? "")
  .split(",")
  .filter(Boolean);
const FAMILIES = ONLY.length ? ALL_FAMILIES.filter(([sym]) => ONLY.includes(sym)) : ALL_FAMILIES;

// In-plane roll angles. ±8° is already a careless wrist; beyond that the
// user is re-aiming and the reticle should guide them instead. ?angles=-12,-6
// overrides (envelope probing).
const ANGLES = (new URLSearchParams(location.search).get("angles") ?? "-8,-4,4,8")
  .split(",")
  .map(Number)
  .filter(Number.isFinite);

/** @param {Fixture} enc */
function maskToCanvas(enc) {
  const img = new ImageData(enc.width, enc.height);
  const px = img.data;
  for (let y = 0; y < enc.height; y++) {
    const row = y * enc.bytesPerRow;
    let o = y * enc.width * 4;
    for (let x = 0; x < enc.width; x++, o += 4) {
      const v = ((enc.data[row + (x >> 3)] ?? 0) >> (x & 7)) & 1 ? 25 : 235;
      px[o] = v; px[o + 1] = v; px[o + 2] = v; px[o + 3] = 255;
    }
  }
  const c = document.createElement("canvas");
  c.width = enc.width;
  c.height = enc.height;
  /** @type {CanvasRenderingContext2D} */ (c.getContext("2d")).putImageData(img, 0, 0);
  return c;
}

/**
 * Draw `canvas` rotated by `deg` about its center onto a white canvas
 * padded to hold the rotated bounding box, snapped to 32px multiples so
 * the binarizer clips nothing.
 */
function rotated(/** @type {HTMLCanvasElement} */ canvas, /** @type {number} */ deg) {
  const rad = (deg * Math.PI) / 180;
  const w0 = canvas.width;
  const h0 = canvas.height;
  const bw = Math.ceil(Math.abs(w0 * Math.cos(rad)) + Math.abs(h0 * Math.sin(rad)));
  const bh = Math.ceil(Math.abs(w0 * Math.sin(rad)) + Math.abs(h0 * Math.cos(rad)));
  const w = Math.ceil(bw / 32) * 32;
  const h = Math.ceil(bh / 32) * 32;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext("2d"));
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, w, h);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.translate(w / 2, h / 2);
  ctx.rotate(rad);
  ctx.drawImage(canvas, -w0 / 2, -h0 / 2);
  return c;
}

/**
 * @typedef {{ width: number, height: number, bytesPerRow: number, data: Uint8Array | number[] }} Fixture
 * @typedef {{ sym: string, text: string, render: string, pass: boolean, got: string, mask: string, ms: number }} TiltRow
 */

/** @param {TiltRow} row */
function renderRow(row) {
  const tr = document.createElement("tr");
  tr.className = row.pass ? "pass" : "fail";
  tr.innerHTML = `<td>${row.sym}</td><td>${row.text}</td><td>${row.render}</td>` +
    `<td>${row.pass ? "PASS" : "FAIL"}</td><td>${row.got}</td><td>${row.mask}</td><td>${row.ms}ms</td>`;
  document.querySelector("tbody")?.append(tr);
}

async function run() {
  const bin = await createBinarizer({ maxSize: 1920 });
  const scanner = await getWasmScanner();
  const rows = [];
  for (const [sym, text] of FAMILIES) {
    const enc = /** @type {Fixture} */ (await encode_fixture_opts(sym, text, 4, 320, -1, -1, 0, -1, 0, 0, 0));
    const flat = maskToCanvas(enc);
    for (const deg of ANGLES) {
      const t0 = performance.now();
      let row;
      try {
        const source = deg === 0 ? flat : rotated(flat, deg);
        const frame = await bin.process(source);
        const results = decodeFrame(scanner, frame);
        const hit = results.find((/** @type {import("../src/decoder.js").DecodedCode} */ r) => r.text === text);
        row = {
          sym, text, render: `${deg}°`,
          pass: !!hit,
          got: hit ? hit.text : (results[0]?.text ?? "(none)"),
          mask: `${frame.width}x${frame.height}`,
          ms: Math.round(performance.now() - t0),
        };
      } catch (err) {
        row = { sym, text, render: `${deg}°`, pass: false, got: `ERROR: ${/** @type {Error} */ (err)}`, mask: "-", ms: 0 };
      }
      rows.push(row);
      renderRow(row);
    }
  }
  const fails = rows.filter((r) => !r.pass);
  const summary = /** @type {HTMLElement} */ (document.getElementById("summary"));
  summary.textContent = fails.length
    ? `${fails.length}/${rows.length} FAILED — ${[...new Set(fails.map((f) => `${f.sym}@${f.render}`))].join(", ")}`
    : `ALL ${rows.length} PASS (±${Math.max(...ANGLES.map(Math.abs))}° roll)`;
  summary.className = fails.length ? "fail" : "pass";
  document.title = fails.length ? "TESTS FAILED" : "TESTS PASS";
}

void run();
