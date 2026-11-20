/**
 * Field-corpus regression (roadmap item 9). Replays REAL photos through
 * the SHIPPED pipeline — `main.js` -> `worker.js` -> `wasm.wasm` — in
 * still mode (the camera-roll path) or live mode (a canvas fake camera
 * through the same `scan({ video })` the default UI uses).
 *
 * Ground truth lives in a caller-supplied manifest, so this page knows
 * nothing personal: photos and `corpus.json` stay under the gitignored
 * `test-codes/field/`. Format (see tools/e2e/corpus.example.json):
 *
 *   { "cases": [ {
 *       "id": "receipt-112238",
 *       "image": "phone-2026-11-17/P_20261117_112238.jpg",
 *       "symbology": "code128",
 *       "text": "108717112609540270746300",
 *       "modes": ["still", "live"],
 *       "latencyMs": 5000,
 *       "budgetMs": 5000,
 *       "formats": ["code128"]
 *   } ] }
 *
 * `image` resolves against the manifest URL. `modes` defaults to
 * `["still"]`; `formats` defaults to `[symbology]` (`?formats=all` scans
 * everything, like the demo's "Everything" default). `xfail` marks a
 * known, tracked failure: it stays visible but does not redden the gate.
 * Title contract: TESTS PASS / TESTS FAILED / TESTS SKIP.
 */

/** @typedef {{ id: string, image: string, symbology: string, text: string, modes?: string[], latencyMs?: number, budgetMs?: number, formats?: string[] | null, xfail?: string }} FieldCase */
/** @typedef {{ version?: number, cases?: FieldCase[] }} Corpus */
/** @typedef {{ id: string, mode: string, expectedSym: string, expectedText: string, gotSym: string, gotText: string, ms: number, pass: boolean, xfail: boolean, note: string }} Row */

const q = new URLSearchParams(location.search);
const corpusUrl = q.get("corpus");
const mode = q.get("mode") === "live" ? "live" : "still";
const only = q.get("id")?.split(",").map((s) => s.trim()).filter(Boolean);
/** Shipped-bundle URL supplied by tools/e2e/field.mjs (a /@fs path). */
const apiParam = q.get("api");
const scanAll = q.get("formats") === "all";
/** Whole-case guard: live `scan` has no total deadline of its own. */
const caseTimeout = Number(q.get("timeout") ?? (mode === "live" ? 20_000 : 30_000));
/** Published live default is a 500ms frame budget; photos are heavier. */
const LIVE_FRAME_BUDGET = 1200;
const STILL_DEFAULT_LATENCY = 5000;
const STILL_DEFAULT_BUDGET = 5000;
const LIVE_DEFAULT_BUDGET = 10_000;

const rows = /** @type {HTMLElement} */ (document.getElementById("rows"));
const summaryEl = /** @type {HTMLElement} */ (document.getElementById("summary"));
const video = /** @type {HTMLVideoElement} */ (document.getElementById("video"));
const norm = (/** @type {string | null | undefined} */ s) =>
  String(s ?? "").toLowerCase().replace(/[^a-z0-9]/g, "");

/** @type {HTMLCanvasElement | null} */
let liveCanvas = null;
let liveTimer = 0;
/** @type {HTMLImageElement | null} */
let liveImage = null;

/** @param {Row} row */
function renderRow(row) {
  const tr = document.createElement("tr");
  tr.className = row.pass ? "pass" : "fail";
  for (const text of [
    row.id,
    row.mode,
    row.expectedSym,
    row.expectedText,
    row.gotText,
    `${row.ms}ms`,
    row.pass ? "PASS" : row.xfail ? "XFAIL" : "FAIL",
    row.note,
  ]) {
    const td = document.createElement("td");
    td.textContent = text;
    tr.append(td);
  }
  rows.append(tr);
}

/** @param {string} why */
function skip(why) {
  document.title = "TESTS SKIP";
  summaryEl.textContent = `== SKIP (${why})`;
}

if (!corpusUrl) {
  skip("no ?corpus= — the field corpus is personal and local-only");
} else {
  try {
    await run(corpusUrl);
  } catch (err) {
    summaryEl.textContent = `== FAIL (${err instanceof Error ? err.message : err})`;
    document.title = "TESTS FAILED";
  }
}

/** @param {string} url */
async function run(url) {
  // `?corpus=` may be a path (/@fs/… on the dev server): resolve it
  // against the page before using it as an image base.
  const corpusHref = new URL(url, location.href).href;
  /** @type {Corpus} */
  let corpus;
  try {
    corpus = /** @type {Corpus} */ (await (await fetch(corpusHref)).json());
  } catch (err) {
    skip(`corpus unreachable (${url}): ${err instanceof Error ? err.message : err}`);
    return;
  }
  /** @type {FieldCase[]} */
  const cases = (corpus.cases ?? []).filter(
    (c) => (!only || only.includes(c.id)) && (!c.modes || c.modes.includes(mode)),
  );
  if (!cases.length) {
    skip(corpus.cases?.length ? `no case matches ?id= / mode=${mode}` : "corpus has no cases");
    return;
  }

  // `?api=` (from tools/e2e/field.mjs) points at the SHIPPED bundle; the
  // default source import is rewritten by Vite for manual/dev use. The
  // published bundle is minified and deliberately outside tsconfig.
  const mod = /** @type {{ default: (opts?: object) => { scan: (opts: object) => Promise<{ symbology: string, text: string } | undefined> } }} */ (
    apiParam ? await import(/* @vite-ignore */ apiParam) : await import("../api/main.js")
  );
  const session = mod.default({});

  /** @type {Row[]} */
  const out = [];
  for (const c of cases) {
    out.push(await runCase(c, session, corpusHref));
  }

  // `xfail` cases are known, tracked failures: visible in the table and
  // the summary, but they do not turn the gate red (a corpus that grows
  // from real failures must not block every other case).
  const failed = out.filter((r) => !r.pass && !r.xfail);
  const xfail = out.filter((r) => r.xfail);
  const passed = out.length - failed.length - xfail.length;
  const summary = failed.length
    ? `== ${passed}/${out.length} PASS (field corpus, ${mode}) — FAIL: ${failed.map((r) => r.id).join(", ")}`
    : `== ${passed}/${out.length} PASS, ${xfail.length} XFAIL (field corpus, ${mode})`;
  summaryEl.textContent = summary;
  // eslint-disable-next-line no-console
  console.log(summary);
  document.title = failed.length ? "TESTS FAILED" : "TESTS PASS";
}

/**
 * @param {FieldCase} c
 * @param {{ scan: (opts: object) => Promise<{ symbology: string, text: string } | undefined> }} session
 * @param {string} corpusUrl
 * @returns {Promise<Row>}
 */
async function runCase(c, session, corpusUrl) {
  const wantSym = norm(c.symbology);
  const wantText = c.text;
  const budget = c.budgetMs ?? (mode === "live" ? LIVE_DEFAULT_BUDGET : STILL_DEFAULT_BUDGET);
  const type = scanAll ? undefined : (c.formats ?? [c.symbology]);
  const row = /** @type {Row} */ ({
    id: c.id, mode, expectedSym: c.symbology, expectedText: wantText,
    gotSym: "", gotText: "", ms: 0, pass: false, xfail: false, note: "",
  });
  try {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.src = new URL(c.image, corpusUrl).href;
    await img.decode();
    const ctrl = new AbortController();
    const guard = setTimeout(() => ctrl.abort(), caseTimeout);
    const t0 = performance.now();
    let code;
    try {
      if (mode === "live") {
        armLiveCamera(img);
        code = await session.scan({
          video,
          latency: c.latencyMs ?? LIVE_FRAME_BUDGET,
          crop: "none",
          signal: ctrl.signal,
          ...(type ? { type } : {}),
        });
      } else {
        code = await session.scan({
          image: img,
          latency: c.latencyMs ?? STILL_DEFAULT_LATENCY,
          signal: ctrl.signal,
          ...(type ? { type } : {}),
        });
      }
    } finally {
      clearTimeout(guard);
    }
    row.ms = Math.round(performance.now() - t0);
    row.gotSym = code?.symbology ?? "";
    row.gotText = code?.text ?? "";
    const symOk = norm(row.gotSym) === wantSym;
    const textOk = row.gotText === wantText;
    row.pass = symOk && textOk && row.ms <= budget;
    if (!code) row.note = "no read";
    else if (!symOk) row.note = `symbology ${row.gotSym} != ${c.symbology}`;
    else if (!textOk) row.note = "text mismatch";
    else if (row.ms > budget) row.note = `over budget (${budget}ms)`;
    row.xfail = !row.pass && !!c.xfail;
    if (row.xfail) row.note = `XFAIL: ${c.xfail}${row.note ? ` (${row.note})` : ""}`;
  } catch (err) {
    row.note = err instanceof Error ? err.message : String(err);
    row.xfail = !!c.xfail;
    if (row.xfail) row.note = `XFAIL: ${c.xfail} (${row.note})`;
  }
  renderRow(row);
  return row;
}

/** @param {HTMLImageElement} img */
function armLiveCamera(img) {
  liveImage = img;
  const canvas = liveCanvas ?? document.createElement("canvas");
  liveCanvas = canvas;
  // A phone camera frame is ~1920 wide; keep the photo's aspect ratio.
  const scale = Math.min(1, 1920 / Math.max(img.naturalWidth, img.naturalHeight));
  canvas.width = Math.max(32, Math.round(img.naturalWidth * scale));
  canvas.height = Math.max(32, Math.round(img.naturalHeight * scale));
  drawLive();
  if (!liveTimer) {
    // A canvas stream only emits when the canvas changes; repaint the
    // (static) photo at ~15fps so the scanner's frame pump keeps running.
    liveTimer = setInterval(drawLive, 66);
  }
  navigator.mediaDevices.getUserMedia = /** @type {typeof navigator.mediaDevices.getUserMedia} */ (
    async () => canvas.captureStream(15)
  );
}

function drawLive() {
  const canvas = liveCanvas;
  const img = liveImage;
  if (!canvas || !img) return;
  const ctx = /** @type {CanvasRenderingContext2D | null} */ (canvas.getContext("2d"));
  if (!ctx) return;
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
}

window.addEventListener("pagehide", () => {
  if (liveTimer) clearInterval(liveTimer);
  liveTimer = 0;
});

// Top-level `await run(...)` has no static import/export: mark the file as
// a module for the type checker (the page loads it with type="module").
export {};
