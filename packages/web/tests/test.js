/**
 * Self-test harness: renders synthetic specimens of every supported
 * symbology (via the Rust fixture encoders exported from WASM), pushes
 * them through the exact same GPU-binarizer -> WASM pipeline the camera
 * video uses, and asserts the decoded text. No camera required — this is
 * the automated regression page.
 *
 * Usage: open /@fs…/tests/test.html. Results render as a table and are exposed
 * read programmatically from the rendered table + document.title.
 */

import { getWasmScanner } from "../src/decoder.js";
import barcode from "../api/main.js";
import workerUrl from "../api/worker.js?worker&url";
// The self-test now drives decodes through the PUBLIC api (scan/render);
// the pipeline-worker transfer protocol keeps its own ?worker=1 mode.
const api = barcode({ worker: workerUrl });

/** Hard still-scan budget per specimen (the API's default still latency). */
const STILL_BUDGET_MS = 5000;
import { createSVG } from "../src/svg.js";
import { createBinarizer } from "../src/binarizer-factory.js";
import { CpuBinarizer } from "../src/cpu-binarizer.js";
import { encode_fixture_opts, encode_fixture_raw_opts } from "../src/wasm/pkg/barcode_wasm.js";
const encodeFixture = (/** @type {string} */ sym, /** @type {string} */ text, /** @type {number} */ mod, /** @type {number} */ h) =>
  encode_fixture_opts(sym, text, mod, h, -1, -1, 0, -1, 0, 0, 0);
const encodeFixtureRaw = (/** @type {string} */ sym, /** @type {Uint8Array} */ raw, /** @type {number} */ mod, /** @type {number} */ h) =>
  encode_fixture_raw_opts(sym, raw, mod, h, -1, -1, 0, -1, 0, 0, 0);
import rawCases from "../tools/e2e/cases.json";

/**
 * ONE source of truth for specimens: tools/e2e/cases.json. The phone e2e
 * driver (tools/e2e/run.mjs) points the camera at the very same payloads,
 * so a case passing here but failing there isolates the physical layer
 * (exposure, pitch, keystone) instead of the decode pipeline. `m` (fixture
 * module px) and `scale` (render scale) are optional per-case overrides.
 *
 * @type {Array<[string, string, number, number]>}
 */
/** Hex string -> bytes. */
const hexBytes = (/** @type {string} */ hex) =>
  Uint8Array.from((hex.match(/../g) ?? []).map((h) => parseInt(h, 16)));
/** Byte-equality over (Uint8Array | undefined). */
const bufEq = (/** @type {Uint8Array | undefined} */ a, /** @type {Uint8Array} */ b) =>
  a instanceof Uint8Array && a.length === b.length && a.every((v, i) => v === b[i]);

/** @typedef {{ s: string, t: string, hex?: string | null, bytes?: Uint8Array | null, m: number, scale: number }} TestCase */
/** The wasm serde boundary is untyped; this is its actual shape. */
/** @typedef {{ width: number, height: number, bytesPerRow: number, data: Uint8Array | number[] }} Fixture */

/** @type {TestCase[]} */
const CASES = rawCases.map((/** @type {{ s: string, t?: string, hex?: string, m?: number, scale?: number }} */ c) => {
  const bytes = c.hex ? hexBytes(c.hex) : null;
  return {
    s: c.s,
    // binary payloads compare as their Latin-1-style text mapping AND
    // as exact bytes via Code.raw
    t: bytes ? Array.from(bytes, (b) => String.fromCharCode(b)).join("") : String(c.t ?? ""),
    hex: c.hex ?? null,
    bytes,
    m: c.m ?? 4,
    scale: c.scale ?? 1.0,
  };
});

/** @param {{width: number, height: number, bytesPerRow: number, data: Uint8Array}} enc */
/** @param {Fixture} enc */
function maskToCanvas(enc) {
  const img = new ImageData(enc.width, enc.height);
  const px = img.data;
  for (let y = 0; y < enc.height; y++) {
    const row = y * enc.bytesPerRow;
    let o = y * enc.width * 4;
    for (let x = 0; x < enc.width; x++, o += 4) {
      // Realistic print luma (not pure 0/255): like a phone photo of paper.
      const v = ((enc.data[row + (x >> 3)] ?? 0) >> (x & 7)) & 1 ? 25 : 235;
      px[o] = v;
      px[o + 1] = v;
      px[o + 2] = v;
      px[o + 3] = 255;
    }
  }
  const c = document.createElement("canvas");
  c.width = enc.width;
  c.height = enc.height;
  /** @type {CanvasRenderingContext2D} */ (c.getContext("2d")).putImageData(img, 0, 0);
  return c;
}

/** @param {HTMLCanvasElement} canvas */
function scaled(canvas, /** @type {number} */ scale) {
  // The binarizer snaps the mask down to 32px multiples; pad the specimen
  // to a 32-multiple with white margin so nothing is clipped, and apply
  // the render scale on top (simulates non-integer module sizes).
  const w0 = Math.round(canvas.width * scale);
  const h0 = Math.round(canvas.height * scale);
  const w = Math.ceil(w0 / 32) * 32;
  const h = Math.ceil(h0 / 32) * 32;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext("2d"));
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, w, h);
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(canvas, Math.floor((w - w0) / 2), Math.floor((h - h0) / 2), w0, h0);
  return c;
}

/**
 * Worker mode (?worker=1): routes the same specimens through the
 * pipeline worker (transferred ImageBitmaps -> GPU/CPU binarizer -> WASM),
 * proving the transfer protocol end to end.
 */
async function runWorker() {
  await getWasmScanner(); // init wasm on the main thread for encode_fixture
  const worker = new Worker(new URL("../src/pipeline-worker.js", import.meta.url), { type: "module" });
  const ready = new Promise((resolve, reject) => {
    worker.addEventListener("message", function onm(/** @type {MessageEvent} */ ev) {
      const m = /** @type {{ type: "ready", kind?: string } | { type: "error", message: string }} */ (ev.data);
      if (m.type === "ready") { worker.removeEventListener("message", onm); resolve(m.kind); }
      if (m.type === "error") { worker.removeEventListener("message", onm); reject(new Error(m.message)); }
    });
    worker.postMessage({ type: "init", opts: { maxSize: 1920 } });
    setTimeout(() => reject(new Error("worker init timeout")), 10000);
  });
  const kind = await ready;
  // Same serial protocol as the production Scanner: one frame in flight,
  // replies matched by arrival (no ids). See src/scanner.js #decodeInWorker.
  /** @typedef {import("../src/pipeline-worker.js").FrameResultMsg} FrameResultMsg */
  /** @typedef {import("../src/decoder.js").DecodedCode} DecodedCode */
  /** @type {{ resolve: (m: FrameResultMsg) => void, reject: (e: Error) => void, timer: number } | null} */
  let pending = null;
  worker.addEventListener("message", (/** @type {MessageEvent} */ ev) => {
    const m = /** @type {FrameResultMsg} */ (ev.data);
    if (m?.type === "frameResult" && pending) {
      clearTimeout(pending.timer);
      const p = pending;
      pending = null;
      m.error ? p.reject(new Error(m.error)) : p.resolve(m);
    }
  });
  /** @param {HTMLCanvasElement} canvas */
  const decodeInWorker = async (canvas) => {
    if (pending) throw new Error("frame already in flight");
    const { promise, resolve, reject } =
      /** @type {PromiseWithResolvers<FrameResultMsg>} */ (Promise.withResolvers());
    pending = { resolve, reject, timer: 0 };
    pending.timer = setTimeout(() => {
      pending = null;
      reject(new Error("worker decode timeout"));
    }, 10000);
    const bitmap = await createImageBitmap(canvas);
    worker.postMessage({ type: "frame", bitmap, opts: { maxSize: 1920 } }, [bitmap]);
    return promise;
  };

  const rows = [];
  for (const { s: sym, t: text, hex, bytes, m: module, scale } of CASES) {
    const t0 = performance.now();
    let row;
    try {
      const enc = /** @type {Fixture} */ (await (bytes ? encodeFixtureRaw(sym, bytes, module, 320) : encodeFixture(sym, text, module, 320)));
      const source = scaled(maskToCanvas(enc), scale);
      const msg = await decodeInWorker(source);
      const hit = msg.results.find((/** @type {DecodedCode} */ r) => r.text === text && (!bytes || bufEq(r.raw, bytes)));
      row = {
        sym, text, hex, bytes, module, scale,
        pass: !!hit,
        got: hit ? hit.text : (msg.results[0]?.text ?? "(none)"),
        binarizer: "worker:" + msg.kind,
        mask: `${msg.frameW}x${msg.frameH}`,
        ms: msg.decodeMs,
      };
    } catch (err) {
      row = { sym, text, hex, bytes, module, scale, pass: false, got: `ERROR: ${/** @type {Error} */ (err)}`, binarizer: "worker:?", mask: "-", ms: 0 };
    }
    rows.push(row);
    renderRow(row);
  }
  worker.terminate();
  return rows;
}

async function run() {
  if (new URLSearchParams(location.search).get("worker") === "1") {
    try {
      const rows = await runWorker();
      const svg = await svgPass(rows);
      const opts = await optionsPass();
      const maxMs = Math.max(...rows.map((r) => r.ms));
      const allPass = rows.every((r) => r.pass) && svg.ok === svg.total && svg.total > 0
        && opts.ok === opts.total && maxMs <= STILL_BUDGET_MS;
      const summary = /** @type {HTMLElement} */ (document.getElementById("summary"));
      summary.textContent = allPass
        ? `ALL ${rows.length} PASS (pipeline worker) | max ${maxMs}ms | svg-roundtrip ${svg.ok}/${svg.total} | options ${opts.ok}/${opts.total}`
        : `${rows.filter((r) => !r.pass).length}/${rows.length} FAILED (pipeline worker) | max ${maxMs}ms | svg-roundtrip ${svg.ok}/${svg.total} | options ${opts.ok}/${opts.total}`;
      summary.className = allPass ? "pass" : "fail";
      document.title = allPass ? "TESTS PASS" : "TESTS FAILED";
    } catch (err) {
      const summary = /** @type {HTMLElement} */ (document.getElementById("summary"));
      summary.textContent = "WORKER FAILED: " + err;
      summary.className = "fail";
      document.title = "TESTS FAILED";
    }
    return;
  }
  const forceCpu = new URLSearchParams(location.search).get("bin") === "cpu";
  const bin = forceCpu
    ? { process: null, kind: "cpu" }
    : await createBinarizer({ maxSize: 1920 });
  let bin2 = bin;
  if (forceCpu) {
    const { CpuBinarizer } = await import("../src/cpu-binarizer.js");
    const cpu = new CpuBinarizer({ maxSize: 1920, windowDivisor: 20, threshold: 0.92 });
    bin2 = { process: (v) => cpu.process(v), destroy: () => {}, kind: "cpu", setThreshold: (t) => cpu.setThreshold(t) };
  }
  const scanner = await getWasmScanner();
  const rows = [];
  for (const { s: sym, t: text, hex, bytes, m: module, scale } of CASES) {
    const t0 = performance.now();
    let row;
    try {
      const enc = /** @type {Fixture} */ (await (bytes ? encodeFixtureRaw(sym, bytes, module, 320) : encodeFixture(sym, text, module, 320)));
      const source = scaled(maskToCanvas(enc), scale);
      const code = await api.scan({ image: source });
      const hit = code?.text === text && (!bytes || bufEq(code?.raw, /** @type {Uint8Array} */ (bytes))) ? code : null;
      row = {
        sym, text, hex, bytes, module, scale,
        pass: !!hit,
        got: hit ? hit.text : (code?.text ?? "(none)"),
        binarizer: "api",
        mask: "-",
        ms: Math.round(performance.now() - t0),
      };
    } catch (err) {
      row = { sym, text, hex, bytes, module, scale, pass: false, got: `ERROR: ${/** @type {Error} */ (err)}`, binarizer: bin2.kind, mask: "-", ms: 0 };
    }
    rows.push(row);
    renderRow(row);
  }
  const allPass = rows.every((r) => r.pass);
  const maxMs = Math.max(...rows.map((r) => r.ms));
  const perfOk = maxMs <= STILL_BUDGET_MS;
  const svg = await svgPass(rows);
  const svgOk = svg.ok === svg.total && svg.total > 0;
  const opts = await optionsPass();
  const summary = /** @type {HTMLElement} */ (document.getElementById("summary"));
  summary.textContent = allPass
    ? `ALL ${rows.length} PASS (api scan) | max ${maxMs}ms | svg-roundtrip ${svg.ok}/${svg.total} | options ${opts.ok}/${opts.total}`
    : `${rows.filter((r) => !r.pass).length}/${rows.length} FAILED (api scan) | max ${maxMs}ms | svg-roundtrip ${svg.ok}/${svg.total} | options ${opts.ok}/${opts.total}`;
  summary.className = allPass && svgOk && opts.ok === opts.total && perfOk ? "pass" : "fail";
  document.title = allPass && svgOk && opts.ok === opts.total && perfOk ? "TESTS PASS" : "TESTS FAILED";
}

/**
 * Prove the render layout options end-to-end: explicit QR EC/version/mask,
 * PDF417 EC level/columns, and a RECTANGULAR Data Matrix. Each case must
 * encode, rasterize back and decode to the same text; the QR/DM cases also
 * assert the module geometry (a forced version and a 12x36 DMRE).
 */
async function optionsPass() {
  /** @type {Array<{ symbology: string, text: string, raw?: Uint8Array,
   *   opts: import("../src/render-options.js").RenderOptions,
   *   geometry?: (r: SVGRect) => boolean }>} */
  const cases = [
    {
      symbology: "qrcode", text: "OPTIONS-QR-H",
      opts: { ec: "H", version: 7, mask: 5 },
      // v7 = 45 modules + 6 modules quiet each side
      geometry: (/** @type {SVGRect} */ r) => r.width === (45 + 12) * 4 && r.height === (45 + 12) * 4,
    },
    { symbology: "qrcode", text: "OPTIONS-QR-L", opts: { ec: "L", mask: 0 } },
    { symbology: "pdf417", text: "OPTS", opts: { level: 6 } },
    { symbology: "pdf417", text: "HI", opts: { columns: 3 } },
    {
      // GS1-128: FNC1 decodes to GS (0x1D); re-encoding a scan result must
      // keep the separator instead of underflowing on the control byte.
      symbology: "code128", text: "0112345678901231\u001d10ABC", opts: {},
    },
    {
      // binary payload → encode_fixture_raw_opts: QR byte mode at v6/EC H
      symbology: "qrcode", text: "", raw: Uint8Array.from([0, 255, 1, 128, 2]),
      opts: { ec: "H", mask: 3, version: 6 },
      geometry: (/** @type {SVGRect} */ r) => r.width === (17 + 4 * 6 + 12) * 4,
    },
    {
      symbology: "datamatrix", text: "OPTIONS-DM",
      opts: { rows: 12, cols: 36 },
      // 36x12 modules + 4 modules quiet each side → visibly rectangular
      geometry: (/** @type {SVGRect} */ r) => r.width === (36 + 8) * 4 && r.height === (12 + 8) * 4,
    },
  ];
  let ok = 0;
  let total = 0;
  for (const c of cases) {
    total++;
    try {
      const input = c.raw
        ? { symbology: c.symbology, raw: c.raw }
        : { symbology: c.symbology, text: c.text };
      const node = /** @type {SVGSVGElement} */ (await api.render(input, c.opts));
      if (!node) throw new Error("render → null");
      const box = node.viewBox.baseVal;
      if (c.geometry && !c.geometry(box)) throw new Error(`geometry ${box.width}x${box.height}`);
      node.setAttribute("style", "max-height:32px;max-width:240px;width:auto;height:auto");
      const canvas = await rasterizeSVG(node, 2);
      const code = await api.scan({ image: canvas });
      const hit = c.raw
        ? code?.raw instanceof Uint8Array && bufEq(code.raw, c.raw)
        : code?.text === c.text;
      if (!hit) {
        throw new Error(c.raw
          ? `scan raw → ${code?.raw ? Array.from(code.raw).join(",") : null}`
          : `scan → ${JSON.stringify(code?.text ?? null)}`);
      }
      ok++;
    } catch (err) {
      console.error("options-pass:", c.symbology, c.opts, err);
    }
  }
  // A Data Matrix scan result carries its ECC200 size, so render(code)
  // keeps a rectangular DMRE rectangular; a payload that does not fit the
  // recorded size must fall back instead of failing.
  total++;
  try {
    let node = /** @type {SVGSVGElement} */ (
      await api.render({ symbology: "datamatrix", text: "OPTIONS-DM" }, { rows: 12, cols: 36 })
    );
    if (!node) throw new Error("DMRE render → null");
    node.setAttribute("style", "max-height:32px;max-width:240px;width:auto;height:auto");
    const code = await api.scan({ image: await rasterizeSVG(node, 2) });
    if (!code) throw new Error("DMRE scan → null");
    if (code.rows !== 12 || code.cols !== 36) throw new Error(`result size ${code.rows}x${code.cols}`);
    node = /** @type {SVGSVGElement} */ (await api.render(code));
    if (!node) throw new Error("inherited render → null");
    const box = node.viewBox.baseVal;
    if (box.width !== (36 + 8) * 4 || box.height !== (12 + 8) * 4) {
      throw new Error(`inherited geometry ${box.width}x${box.height}`);
    }
    // 300 lowercase chars need 72x72: the recorded 12x36 cannot hold them.
    node = /** @type {SVGSVGElement} */ (
      await api.render({ symbology: "datamatrix", text: "a".repeat(300), rows: 12, cols: 36 })
    );
    if (!node) throw new Error("fallback render → null");
    if (node.viewBox.baseVal.width <= (36 + 8) * 4) throw new Error("fallback kept the too-small size");
    ok++;
  } catch (err) {
    console.error("dm-inherit-pass:", err);
  }
  return { ok, total };
}

/**
 * For every passing row: re-encode the found value as SVG, show it, then
 * prove the representation by rasterizing it back and decoding it again.
 * @param {any[]} rows
 */
async function svgPass(rows) {
  const scanner = await getWasmScanner();
  const trs = /** @type {NodeListOf<HTMLTableRowElement>} */ (
    document.querySelectorAll("tbody tr")
  );
  let ok = 0;
  let total = 0;
  // strictly serial: the api worker answers one frame at a time
  for (const [i, row] of rows.entries()) {
    {
      if (!row.pass) continue;
      const cell = /** @type {HTMLElement | undefined} */ (trs[i]?.querySelector(".svg"));
      if (!cell) continue;
      try {
        // binary rows render from the exact bytes (render()'s raw input)
        const node = /** @type {SVGSVGElement} */ (await (row.hex
          ? api.render({ symbology: row.sym, raw: row.bytes })
          : api.render({ symbology: row.sym, text: row.text })));
        if (!node) throw new Error("render \u2192 null");
        node.setAttribute("style", "max-height:32px;max-width:240px;width:auto;height:auto;display:block");
        cell.append(node);
        const canvas = await rasterizeSVG(node, 2);
        const code = await api.scan({ image: canvas });
        const hit = row.hex
          ? bufEq(code?.raw, /** @type {Uint8Array} */ (row.bytes))
          : code?.text === row.text;
        total++;
        if (hit) ok++;
        else {
          cell.title = "SVG round-trip FAILED: " + JSON.stringify(code?.text ?? null);
          cell.className = "svg rtfail";
        }
      } catch (err) {
        cell.textContent = "svg: " + err;
      }
    }
  }
  return { ok, total };
}

/** Rasterize an SVG element into a canvas at `scale` x its viewBox size. */
async function rasterizeSVG(/** @type {SVGSVGElement} */ node, /** @type {number} */ scale) {
  const img = new Image();
  img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(node.outerHTML);
  await img.decode();
  const w = Math.round(node.viewBox.baseVal.width * scale);
  const h = Math.round(node.viewBox.baseVal.height * scale);
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const cx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
  cx.fillStyle = "#fff";
  cx.fillRect(0, 0, w, h);
  cx.drawImage(img, 0, 0, w, h);
  return canvas;
}

/** Threshold a black/white canvas into the BinarizedFrame mask contract. */
function canvasToBitImage(/** @type {HTMLCanvasElement} */ canvas) {
  const { width, height } = canvas;
  const d = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d")).getImageData(0, 0, width, height).data;
  const bytesPerRow = Math.ceil(width / 8 / 4) * 4;
  const data = new Uint8Array(bytesPerRow * height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (/** @type {number} */ (d[(y * width + x) * 4]) < 128) {
        const i = y * bytesPerRow + (x >> 3);
        data[i] = /** @type {number} */ (data[i]) | (1 << (x & 7));
      }
    }
  }
  return { width, height, bytesPerRow, data };
}

/** @param {{ sym: string, text: string, hex?: string | null, bytes?: Uint8Array | null, module: number, scale: number, pass: boolean, got: string, mask: string, ms?: number, [k: string]: unknown }} row */
function renderRow(row) {
  const tr = document.createElement("tr");
  tr.className = row.pass ? "pass" : "fail";
  tr.innerHTML = `<td>${row.sym}</td><td>${row.text}</td><td>${row.module}px @${row.scale}x</td>` +
    `<td>${row.pass ? "PASS" : "FAIL"}</td><td>${row.got}</td><td>${row.mask}</td><td>${row.ms}ms</td><td class="svg"></td>`;
  document.querySelector("tbody")?.append(tr);
}

const start = /** @type {HTMLButtonElement} */ (document.getElementById("run"));
start.addEventListener("click", () => { void run(); });
void run();
