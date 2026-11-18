/**
 * Field-fixture harness: pushes packages/web/test-codes/field/
 * viva-dm-luma.json (1080x1920 grayscale photo of a DMRE envelope) through
 * the exact production pipeline — ImageBitmap -> pipeline worker -> GPU
 * binarizer -> WASM tiers — and asserts the decoded payload. No camera.
 *
 * Two rows: the all-tiers path (2D-first: QR scores nothing, DM decodes)
 * and the formats:["datamatrix"] narrowed path.
 *
 * Driven by tools/run-tests.mjs <cdpPort> /fixture.html (document.title
 * becomes TESTS PASS / TESTS FAILED).
 */

const EXPECTED = "00385F00010004L51QZWWY0003294720";

async function frameToCanvas(/** @type {{w: number, h: number, luma: number[]}} */ f) {
  const img = new ImageData(f.w, f.h);
  const px = img.data;
  for (let i = 0; i < f.w * f.h; i++) {
    const v = f.luma[i] ?? 0;
    const o = i * 4;
    px[o] = v;
    px[o + 1] = v;
    px[o + 2] = v;
    px[o + 3] = 255;
  }
  const c = document.createElement("canvas");
  c.width = f.w;
  c.height = f.h;
  /** @type {CanvasRenderingContext2D} */ (c.getContext("2d")).putImageData(img, 0, 0);
  return c;
}

/** Same serial worker protocol as the production Scanner (src/scanner.js). */
/** @typedef {import("../src/pipeline-worker.js").FrameResultMsg} FrameResultMsg */
/** @typedef {import("../src/decoder.js").DecodedCode} DecodedCode */

function connectWorker() {
  const worker = new Worker(new URL("../src/pipeline-worker.js", import.meta.url), { type: "module" });
  const ready = new Promise((resolve, reject) => {
    worker.addEventListener("message", function onm(/** @type {MessageEvent} */ ev) {
      const m = /** @type {{ type: "ready", kind?: string } | { type: "error", message: string }} */ (ev.data);
      if (m.type === "ready") { worker.removeEventListener("message", onm); resolve(m.kind); }
      if (m.type === "error") { worker.removeEventListener("message", onm); reject(new Error(m.message)); }
    });
    worker.postMessage({ type: "init", opts: { maxSize: 1920 } });
    setTimeout(() => reject(new Error("worker init timeout")), 15000);
  });
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
  return {
    kind: ready,
    /** @param {HTMLCanvasElement} canvas @param {import("../src/pipeline-worker.js").FrameOpts} [opts] */
    async decode(canvas, opts) {
      if (pending) throw new Error("frame already in flight");
      const { promise, resolve, reject } =
        /** @type {PromiseWithResolvers<FrameResultMsg>} */ (Promise.withResolvers());
      pending = { resolve, reject, timer: 0 };
      pending.timer = setTimeout(() => {
        pending = null;
        reject(new Error("worker decode timeout"));
      }, 30000);
      const bitmap = await createImageBitmap(canvas);
      worker.postMessage({ type: "frame", bitmap, opts }, [bitmap]);
      return promise;
    },
  };
}

/** @param {{ path: string, pass: boolean, got: string, binarizer: string, mask: string, ms: number }} row */
function renderRow(row) {
  const tr = document.createElement("tr");
  for (const v of [row.path, EXPECTED, row.pass ? "PASS" : "FAIL", row.got, row.binarizer, row.mask, String(row.ms)]) {
    const td = document.createElement("td");
    td.textContent = v;
    tr.appendChild(td);
  }
  document.getElementById("rows")?.appendChild(tr);
}

async function run() {
  const t0 = performance.now();
  // The field fixture is personal data and lives locally only.
  let f;
  try {
    f = /** @type {{ w: number, h: number, luma: number[] }} */ (await (await import("../test-codes/field/viva-dm-luma.json")).default);
  } catch {
    document.getElementById("status")?.append("field fixture not present locally — skipped");
    return;
  }
  const loadMs = Math.round(performance.now() - t0);
  const canvas = await frameToCanvas(f);
  const w = connectWorker();
  const kind = await w.kind;

  /** @type {any[]} */
  const rows = [];
  // 1. all tiers (production default): QR runs first and must score
  //    nothing; the DM tier then decodes.
  let dmResults = null;
  {
    const t1 = performance.now();
    const msg = await w.decode(canvas, { maxSize: 1920 });
    const hit = msg.results.find((/** @type {DecodedCode} */ r) => r.text === EXPECTED);
    rows.push({
      path: "all-tiers",
      pass: !!hit,
      got: hit ? hit.text : (msg.results[0]?.text ?? "(none)"),
      binarizer: "worker:" + msg.kind,
      mask: `${msg.frameW}x${msg.frameH}`,
      ms: Math.round(performance.now() - t1),
    });
  }
  // 2. formats:["datamatrix"]: only the DM tier runs.
  {
    const t2 = performance.now();
    const msg = await w.decode(canvas, { maxSize: 1920, formats: ["datamatrix"] });
    dmResults = msg.results;
    const hit = msg.results.find((/** @type {DecodedCode} */ r) => r.text === EXPECTED);
    rows.push({
      path: "datamatrix-only",
      pass: !!hit,
      got: hit ? hit.text : (msg.results[0]?.text ?? "(none)"),
      binarizer: "worker:" + msg.kind,
      mask: `${msg.frameW}x${msg.frameH}`,
      ms: Math.round(performance.now() - t2),
    });
  }
  const detail = document.getElementById("detail");
  if (detail)
    detail.textContent =
      `fixture ${f.w}x${f.h}, luma load ${loadMs}ms, binarizer ${kind}\n` +
      JSON.stringify(dmResults, null, 1);

  for (const row of rows) renderRow(row);
  const allPass = rows.every((r) => r.pass);
  const summary = /** @type {HTMLElement} */ (document.getElementById("summary"));
  summary.textContent = allPass
    ? `ALL ${rows.length} PASS (field fixture, ${kind})`
    : `${rows.filter((r) => !r.pass).length}/${rows.length} FAILED (field fixture, ${kind})`;
  summary.className = allPass ? "pass" : "fail";
  document.title = allPass ? "TESTS PASS" : "TESTS FAILED";
}

run().catch((err) => {
  const summary = /** @type {HTMLElement} */ (document.getElementById("summary"));
  summary.textContent = "FIXTURE FAILED: " + err;
  summary.className = "fail";
  document.title = "TESTS FAILED";
});
