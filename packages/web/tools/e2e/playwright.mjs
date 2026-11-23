#!/usr/bin/env node
/**
 * Cross-engine regression driver — the CI-friendly companion to the CDP
 * tools (run-tests.mjs / fakecam.mjs / field.mjs), which remain the
 * authority for real phones over adb.
 *
 *   node tools/e2e/playwright.mjs                 # chromium + webkit + iphone
 *   node tools/e2e/playwright.mjs --browser webkit
 *   node tools/e2e/playwright.mjs --ci            # headless chromium + webkit
 *   node tools/e2e/playwright.mjs --fakecam       # only the live-path suite
 *   node tools/e2e/playwright.mjs --field         # local real-photo corpus
 *   node tools/e2e/playwright.mjs --only qrcode   # one fake-camera case
 *
 * It starts Vite itself (source pages: /@fs/tests/test.html, contract-test,
 * /scan.html) plus a static server for the built /dist payload
 * (/tests/artifact-test.html), so no external Chrome/CDP setup is needed.
 *
 * Projects:
 *   chromium  desktop Chromium (Blink)
 *   webkit    desktop WebKit/Safari engine
 *   iphone    WebKit with the iPhone 15 Pro descriptor — the automated
 *             proxy for iOS Safari when no device is attached. A REAL
 *             iPhone can be driven manually with `pnpm serve:iphone`
 *             (HTTPS on the LAN, camera needs a secure origin).
 *
 * The fake-camera mock is the same one fakecam.mjs uses (fakecam-spec.mjs),
 * installed BEFORE any page script runs.
 */
import { chromium, webkit, devices } from "playwright";
import { createServer as createViteServer } from "vite";
import { createServer as createHttpServer } from "node:http";
import { appendFileSync, existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { CASES, XFAIL, MOCK } from "./fakecam-spec.mjs";

const pkg = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const repo = resolve(pkg, "../..");
if (!existsSync(resolve(pkg, "dist/main.js"))) {
  console.error("missing dist/ — build it first: npm run build");
  process.exit(2);
}
const argv = process.argv.slice(2);
const has = (name) => argv.includes(`--${name}`);
const flag = (name, fallback) => {
  // `name` already includes the leading dashes (`--browser`, `--only`, …).
  const i = argv.indexOf(name);
  return i >= 0 ? (argv[i + 1] ?? fallback) : fallback;
};

const CI = has("--ci");
const ONLY_FAKECAM = has("--fakecam");
const FIELD = has("--field") || (!CI && !ONLY_FAKECAM && argv.length === 0);
const BROWSER = flag("--browser", CI ? "ci" : "all");
const CASE_TIMEOUT = Number(flag("--case-timeout", "20000"));
const ONLY = flag("--only", "");
const selected = ONLY ? CASES.filter(([s]) => s === ONLY) : CASES;
const OUT = flag("--jsonl", process.env.E2E_LOG ?? "/tmp/opencode/playwright-e2e.jsonl");
mkdirSync(dirname(OUT), { recursive: true });

/** @typedef {{ name: string, engine: "chromium" | "webkit", context: object }} Project */
/** @type {Project[]} */
const PROJECTS = [];
if (BROWSER === "all" || BROWSER === "ci" || BROWSER === "chromium")
  PROJECTS.push({ name: "chromium", engine: "chromium", context: {} });
if (BROWSER === "all" || BROWSER === "ci" || BROWSER === "webkit")
  PROJECTS.push({ name: "webkit", engine: "webkit", context: {} });
if (BROWSER === "all" || BROWSER === "iphone")
  PROJECTS.push({ name: "iphone", engine: "webkit", context: devices["iPhone 15 Pro"] });
if (!PROJECTS.length) {
  console.error(`unknown --browser ${BROWSER} (chromium | webkit | iphone | all | ci)`);
  process.exit(2);
}

// --- servers --------------------------------------------------------------
const vite = await createViteServer({ logLevel: "error", server: { port: 5173 } });
await vite.listen();
const ORIGIN = vite.resolvedUrls.local[0].replace(/\/$/, "");
// Internal harness pages live outside the dev root; Vite serves them via @fs.
const TESTS_FS = `${ORIGIN}/@fs${pkg}/tests`;

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
};
const staticServer = createHttpServer((req, res) => {
  try {
    const url = new URL(req.url ?? "/", "http://localhost");
    const path = normalize(join(repo, decodeURIComponent(url.pathname)));
    if (!path.startsWith(repo) || !statSync(path).isFile()) {
      res.writeHead(404).end("not found");
      return;
    }
    res.writeHead(200, { "content-type": MIME[extname(path)] ?? "application/octet-stream" });
    res.end(readFileSync(path));
  } catch {
    res.writeHead(404).end("not found");
  }
});
await new Promise((r) => staticServer.listen(0, "127.0.0.1", r));
const STATIC = `http://127.0.0.1:${staticServer.address().port}`;

// --- reporting ------------------------------------------------------------
/** @type {{ project: string, suite: string, name: string, ok: boolean, xfail?: boolean, ms: number, note?: string }[]} */
const rows = [];
const record = (row) => {
  rows.push(row);
  const xfail = row.xfail ? "XFAIL" : row.ok ? "PASS" : "FAIL";
  console.log(`${xfail.padEnd(5)} ${row.project.padEnd(8)} ${row.suite.padEnd(9)} ${row.name.padEnd(24)} ${String(row.ms).padStart(6)}ms ${row.note ?? ""}`);
  if (OUT) appendFileSync(OUT, JSON.stringify(row) + "\n");
};

// --- suites ---------------------------------------------------------------
/** Wait for the page's title contract. @param {import('playwright').Page} page */
async function waitTitle(page, pass, fail, timeout) {
  await page.waitForFunction(
    ([p, f]) => document.title === p || document.title === f,
    [pass, fail],
    { timeout, polling: 250 },
  );
  return page.title();
}

/** The synthetic round-trip harness. */
async function suiteSelftest(page, project) {
  /** @type {[string, string, string][]} */
  const cases = [["gpu-or-cpu", `${TESTS_FS}/test.html`, "TESTS PASS"]];
  if (process.env.CI && project !== "chromium") {
    // The software binarizer's 31-case suite takes minutes on WebKitGTK;
    // Chromium covers that path explicitly, WebKit still covers the
    // default binarizer (and every other gate).
    console.log(`SKIP  ${project.padEnd(8)} selftest  cpu (CI: Chromium covers the explicit CPU path)`);
  } else {
    cases.push(["cpu", `${TESTS_FS}/test.html?bin=cpu`, "TESTS PASS"]);
  }
  for (const [name, url, pass] of cases) {
    const t0 = Date.now();
    await page.goto(url, { waitUntil: "load" });
    const title = await waitTitle(page, pass, "TESTS FAILED", 240_000);
    const summary = await page.locator("#summary").textContent();
    record({ project, suite: "selftest", name, ok: title === pass, ms: Date.now() - t0, note: summary ?? "" });
  }
}

/** Worker transports / render-raw / duplicate binding. */
async function suiteContract(page, project) {
  const t0 = Date.now();
  await page.goto(`${TESTS_FS}/contract-test.html`, { waitUntil: "load" });
  const title = await waitTitle(page, "TESTS PASS", "TESTS FAILED", 120_000);
  const checks = await page.evaluate(() =>
    [...document.querySelectorAll("table tr")].map((tr) => [...tr.children].map((td) => td.textContent)),
  );
  let fails = 0;
  for (const [name, pass, note] of checks) {
    if (pass !== "PASS") fails++;
    record({ project, suite: "contract", name: String(name), ok: pass === "PASS", ms: 0, note: note ?? "" });
  }
  record({ project, suite: "contract", name: "total", ok: title === "TESTS PASS" && fails === 0, ms: Date.now() - t0, note: `${checks.length - fails}/${checks.length}` });
}

/** Deterministic random/corrupted masks through the real wasm boundary. */
async function suiteFuzz(page, project) {
  const t0 = Date.now();
  await page.goto(`${TESTS_FS}/fuzz.html`, { waitUntil: "load" });
  const title = await waitTitle(page, "FUZZ PASS", "FUZZ FAILED", 240_000);
  const note = await page.locator("#summary").textContent();
  record({ project, suite: "fuzz", name: "mutation", ok: title === "FUZZ PASS", ms: Date.now() - t0, note: note ?? "" });
}

/** The built dist/ payload, plain static hosting (no bundler). */
async function suiteArtifact(browser, project) {
  const context = await browser.newContext(project.context);
  const page = await context.newPage();
  const t0 = Date.now();
  await page.goto(`${STATIC}/packages/web/tests/artifact-test.html`, { waitUntil: "load" });
  const title = await waitTitle(page, "ARTIFACT PASS", "ARTIFACT FAIL", 60_000);
  const result = await page.evaluate(() => globalThis.__RESULT);
  record({ project: project.name, suite: "artifact", name: "dist/ static", ok: title === "ARTIFACT PASS", ms: Date.now() - t0, note: result ?? "" });
  await context.close();
}

/**
 * The live path with a fake camera. One page per project, init script
 * installed once; each case is a fresh navigation with `?e2e=`.
 */
async function suiteFakecam(browser, project) {
  const context = await browser.newContext(project.context);
  await context.addInitScript({ content: MOCK });
  const page = await context.newPage();
  // The mock camera is a canvas + captureStream(). WebKitGTK (CI) does
  // not always expose canvas capture; the page is still covered by the
  // synthetic, contract and docs suites there.
  const canCapture = await page.evaluate(
    () => typeof HTMLCanvasElement.prototype.captureStream === "function",
  );
  if (!canCapture || (process.env.CI && project !== "chromium")) {
    console.log(
      `SKIP  ${project.padEnd(8)} fakecam     ` +
      (canCapture
        ? "CI runs the fake camera on Chromium; WebKit keeps the other suites"
        : "canvas.captureStream unavailable in this engine"),
    );
    await context.close();
    return;
  }
  for (const [s, t] of selected) {
    const format = s
      .replace(/^inverted-/, "")
      .replace(/^island-/, "")
      .replace(/^rotated-/, "")
      .replace(/^diagonal-/, "")
      .replace(/^tiny\d+-/, "");
    const max = s.startsWith("tiny") ? "&max=1056" : "";
    const url = `${TESTS_FS}/scan-fakecam.html?formats=${encodeURIComponent(format)}&e2e=${encodeURIComponent(`${s}:${t}`)}${max}&_=${Date.now()}`;
    const t0 = Date.now();
    let got = null;
    let status = "";
    let note = "";
    try {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector("#start", { timeout: 30_000 });
      await page.click("#start");
      const deadline = Date.now() + (XFAIL.has(s) ? Math.min(6000, CASE_TIMEOUT) : CASE_TIMEOUT);
      while (Date.now() < deadline) {
        const st = await page.evaluate(() => {
          const card = document.getElementById("result");
          const visible = card && card.style.display !== "none";
          return {
            visible,
            value: visible ? document.getElementById("value")?.textContent : null,
            status: document.getElementById("status")?.textContent ?? "",
          };
        });
        status = st.status;
        if (st.visible && st.value) {
          const digitsOnly = st.value.replace(/^\(\d+\)/, "").replace(/\D/g, "");
          if (st.value === t || (/^\d+$/.test(t) && digitsOnly.startsWith(t))) {
            got = st.value;
            break;
          }
        }
        await page.waitForTimeout(300);
      }
    } catch (err) {
      note = String(err?.message ?? err).split("\n")[0];
    }
    const ok = got === t;
    const xfail = ok ? undefined : XFAIL.get(s);
    record({
      project, suite: "fakecam", name: s, ok,
      xfail: !!xfail,
      ms: Date.now() - t0,
      note: ok ? JSON.stringify(got) : xfail ?? (note || status),
    });
  }
  // Default UI gesture: a two-finger pinch must drive the CAMERA track's
  // zoom (the decoder reads stream pixels; a CSS scale would not), and
  // `touch-action: none` keeps the page at scale 1. The fake camera
  // advertises a zoom capability for this check (`?fakezoom=1`). The
  // fixture is a QR but only aztec is requested, so nothing confirms and
  // the dialog stays up for the measurement.
  {
    const t0 = Date.now();
    let ok = false;
    let note = "";
    try {
      const e2e = encodeURIComponent("qrcode:PINCH-ZOOM");
      await page.goto(
        `${TESTS_FS}/scan-fakecam.html?formats=aztec&fakezoom=1&e2e=${e2e}&_=${Date.now()}`,
        { waitUntil: "load" },
      );
      await page.waitForSelector("#start", { timeout: 30_000 });
      await page.click("#start");
      await page.waitForFunction(
        () => {
          const v = document.querySelector("dialog.barcode-ui video");
          return !!v && v.videoWidth > 0;
        },
        null,
        { timeout: 15_000 },
      );
      ok = await page.evaluate(() => {
        const track = document.querySelector("dialog.barcode-ui video")
          ?.srcObject?.getVideoTracks?.()[0];
        const stage = document.querySelector("dialog.barcode-ui .barcode-ui-stage");
        if (!track || !stage) return false;
        const before = track.getSettings().zoom;
        const r = stage.getBoundingClientRect();
        const cx = r.x + r.width / 2;
        const cy = r.y + r.height / 2;
        const fire = (/** @type {string} */ type, /** @type {number} */ pointerId, /** @type {number} */ x, /** @type {number} */ y) =>
          stage.dispatchEvent(new PointerEvent(type, {
            pointerId, pointerType: "touch", bubbles: true, cancelable: true,
            clientX: x, clientY: y, isPrimary: pointerId === 1,
          }));
        fire("pointerdown", 1, cx - 30, cy);
        fire("pointerdown", 2, cx + 30, cy);
        fire("pointermove", 1, cx - 75, cy);
        fire("pointermove", 2, cx + 75, cy);
        fire("pointerup", 1, cx - 75, cy);
        fire("pointerup", 2, cx + 75, cy);
        // setZoom is async: wait for the applied constraint.
        return new Promise((resolve) => {
          const deadline = performance.now() + 2000;
          const poll = () => {
            const zoom = track.getSettings().zoom;
            if (zoom > before) resolve(visualViewport.scale === 1);
            else if (performance.now() > deadline) resolve(false);
            else setTimeout(poll, 50);
          };
          poll();
        });
      });
      if (!ok) note = "pinch did not raise the track zoom (or the page zoomed)";
    } catch (err) {
      note = String(err?.message ?? err).split("\n")[0];
    }
    record({ project, suite: "fakecam", name: "pinch-zoom", ok, ms: Date.now() - t0, note: ok ? "" : note });
  }
  // Adaptive zoom on the published main.js path (no msg.mask): a small code
  // (< 40% of frame width) must raise the camera track's zoom once decoded.
  {
    const t0 = Date.now();
    let ok = false;
    let note = "";
    try {
      const e2e = encodeURIComponent("tiny132-qrcode:TINY-QR-ROI");
      await page.goto(
        `${TESTS_FS}/scan-fakecam.html?formats=qrcode&fakezoom=1&max=1056&e2e=${e2e}&_=${Date.now()}`,
        { waitUntil: "load" },
      );
      await page.waitForSelector("#start", { timeout: 30_000 });
      await page.evaluate(() => {
        const orig = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
        navigator.mediaDevices.getUserMedia = async (c) => {
          const stream = await orig(c);
          globalThis.__lastTrack = stream.getVideoTracks()[0];
          return stream;
        };
      });
      await page.click("#start");
      await page.waitForFunction(
        () => document.getElementById("value")?.textContent === "TINY-QR-ROI",
        null,
        { timeout: 15_000 },
      );
      const zoom = await page.evaluate(() => globalThis.__lastTrack?.getSettings?.().zoom ?? 1);
      ok = zoom > 1;
      if (!ok) note = `expected adaptiveZoom > 1, got ${zoom}`;
    } catch (err) {
      note = String(err?.message ?? err).split("\n")[0];
    }
    record({ project, suite: "fakecam", name: "adaptive-zoom", ok, ms: Date.now() - t0, note: ok ? "" : note });
  }
  // Injected decode forwarding frameW/frameH: verifies both 2D close-up hint
  // cropping (#updateHint/#mapFromHint) and digital-zoom coordinate mapping.
  {
    const t0 = Date.now();
    let ok = false;
    let note = "";
    try {
      const res = await page.evaluate(async (base) => {
        const { LiveScanner } = await import(`${base}/src/live-scanner.js`);
        const canvas = document.createElement("canvas");
        canvas.width = 600;
        canvas.height = 600;
        const cx = canvas.getContext("2d");
        cx.fillStyle = "#fff";
        cx.fillRect(0, 0, 600, 600);
        const origGum = navigator.mediaDevices.getUserMedia;
        const origEnum = navigator.mediaDevices.enumerateDevices;
        const makeStream = () => {
          const s = canvas.captureStream(30);
          const tick = setInterval(() => { cx.fillRect(0, 0, 1, 1); }, 33);
          s.getVideoTracks().forEach((t) => t.addEventListener("ended", () => clearInterval(tick)));
          return s;
        };
        navigator.mediaDevices.enumerateDevices = async () => [];
        navigator.mediaDevices.getUserMedia = async () => makeStream();
        try {
          // 1. 2D hint close-up + quad mapping back to full 600x600 video coords.
          const video1 = document.createElement("video");
          video1.muted = true;
          video1.playsInline = true;
          const calls1 = [];
          const hit1 = await new Promise((resolve, reject) => {
            const sc = new LiveScanner({
              video: video1,
              crop: "square",
              adaptiveZoom: false,
              decode: async (bmp, fmts) => {
                calls1.push({ w: bmp.width, h: bmp.height, fmts });
                const isHint = calls1.length > 1;
                const pos = isHint
                  ? [{ x: 25, y: 25 }, { x: 125, y: 25 }, { x: 125, y: 125 }, { x: 25, y: 125 }]
                  : [{ x: 200, y: 200 }, { x: 300, y: 200 }, { x: 300, y: 300 }, { x: 200, y: 300 }];
                return {
                  results: [{ symbology: "qrcode", text: "HINT-OK", raw: new Uint8Array([1]), position: pos }],
                  frameW: bmp.width,
                  frameH: bmp.height,
                };
              },
              onResults: (codes) => { void sc.stop(); resolve(codes[0]); },
              onError: reject,
            });
            sc.start().catch(reject);
          });
          const hintCropped = calls1.length >= 2 && calls1[0].w === 600 && calls1[1].w < 300 && calls1[1].fmts?.[0] === "qrcode";
          const hintMapped = hit1.position[0].x >= 180 && hit1.position[0].y >= 180;

          // 2. Digital zoom (2x center crop -> 300x300) maps quad back to full 600x600 space.
          const video2 = document.createElement("video");
          video2.muted = true;
          video2.playsInline = true;
          const calls2 = [];
          const hit2 = await new Promise((resolve, reject) => {
            const sc = new LiveScanner({
              video: video2,
              crop: "square",
              adaptiveZoom: false,
              decode: async (bmp) => {
                calls2.push({ w: bmp.width, h: bmp.height });
                if (bmp.width > 320) {
                  // Wait for zoomTo(2) to take effect before returning hits.
                  return { results: [], frameW: bmp.width, frameH: bmp.height };
                }
                return {
                  results: [{
                    symbology: "code128",
                    text: "DZOOM-OK",
                    raw: new Uint8Array([1]),
                    position: [{ x: 50, y: 120 }, { x: 250, y: 120 }, { x: 250, y: 180 }, { x: 50, y: 180 }],
                  }],
                  frameW: bmp.width,
                  frameH: bmp.height,
                };
              },
              onResults: (codes) => { void sc.stop(); resolve(codes[0]); },
              onError: reject,
            });
            sc.start().then(() => sc.zoomTo(2)).catch(reject);
          });
          const dzoomCropped = calls2.some((c) => c.w === 300 && c.h === 300);
          const dzoomMapped = Math.round(hit2.position[0].x) === 200 && Math.round(hit2.position[0].y) === 270;
          return { hintCropped, hintMapped, dzoomCropped, dzoomMapped, calls1, hit1Pos: hit1.position[0], hit2Pos: hit2.position[0] };
        } finally {
          navigator.mediaDevices.getUserMedia = origGum;
          navigator.mediaDevices.enumerateDevices = origEnum;
        }
      }, `${ORIGIN}/@fs${pkg}`);
      ok = res.hintCropped && res.hintMapped && res.dzoomCropped && res.dzoomMapped;
      if (!ok) note = JSON.stringify(res);
    } catch (err) {
      note = String(err?.message ?? err).split("\n")[0];
    }
    record({ project, suite: "fakecam", name: "hint-and-digital-zoom", ok, ms: Date.now() - t0, note: ok ? "" : note });
  }
  await context.close();
}

/**
 * The committed docs/ site: landing, render round-trip, still sample,
 * worker page, and (once, on Chromium) the bundled self-test.
 */
async function suiteDocs(browser, project) {
  const context = await browser.newContext(project.context);
  const page = await context.newPage();
  const check = async (/** @type {string} */ name, /** @type {() => Promise<void>} */ fn) => {
    const t0 = Date.now();
    try {
      await fn();
      record({ project, suite: "docs", name, ok: true, ms: Date.now() - t0 });
    } catch (err) {
      record({
        project, suite: "docs", name, ok: false, ms: Date.now() - t0,
        note: String(/** @type {Error} */ (err)?.message ?? err).split("\n")[0],
      });
    }
  };

  await check("landing links", async () => {
    await page.goto(`${STATIC}/docs/index.html`, { waitUntil: "load" });
    const title = await page.title();
    if (!title.includes("@webreflection/barcode")) throw new Error(`title: ${title}`);
    const links = await page.locator("a.card").count();
    if (links < 5) throw new Error(`only ${links} playground cards`);
  });

  await check("render round-trip", async () => {
    await page.goto(`${STATIC}/docs/render.html`, { waitUntil: "load" });
    await page.waitForSelector("#preview svg", { timeout: 30_000 });
    await page.click("#scanback");
    await page.waitForFunction(
      () => document.getElementById("status")?.textContent?.includes("Round-trip OK"),
      null,
      { timeout: 30_000 },
    );
  });

  await check("still sample", async () => {
    await page.goto(`${STATIC}/docs/still.html`, { waitUntil: "load" });
    await page.click("#sample-c128");
    await page.waitForFunction(
      () => document.getElementById("value")?.textContent === "STILL-DEMO-001",
      null,
      { timeout: 30_000 },
    );
  });

  await check("worker entries", async () => {
    await page.goto(`${STATIC}/docs/worker.html`, { waitUntil: "load" });
    await page.click("#auto-main");
    await page.click("#explicit");
    await page.waitForFunction(
      () => document.getElementById("log")?.textContent?.includes("render + scan"),
      null,
      { timeout: 30_000 },
    );
    await page.click("#auto-worker");
    await page.waitForFunction(
      () => document.getElementById("log")?.textContent?.includes("AUTO-WORKER"),
      null,
      { timeout: 30_000 },
    );
  });


  await context.close();
}

/** Committed, non-sensitive specimens from shipped bug reports. */
async function suiteRegression(browser, project) {
  const box = resolve(pkg, "test-codes/regression/gs1-datamatrix-box-2026-11-19.jpeg");
  const hard = resolve(pkg, "test-codes/regression/qr-hard-negative-2026-11-19.jpeg");
  const code32 = resolve(pkg, "test-codes/regression/code32-aic-2026-11-19.jpeg");
  const code128UpsideDown = resolve(pkg, "test-codes/regression/code128-upside-down-label-2026-11-23.png");
  const code128SmallQuietZone = resolve(pkg, "test-codes/regression/code128-small-quiet-zone-2026-11-23.jpeg");
  const pharma = resolve(pkg, "test-codes/regression/pharma-real-code-2026-11-20.jpeg");
  const pharmaClose = resolve(pkg, "test-codes/regression/pharma-itf-close-2026-11-20.jpeg");
  const blurTilt = resolve(pkg, "test-codes/regression/ean13-blur-tilt-mask-2026-11-22.png");
  const code39Tilt = resolve(pkg, "test-codes/regression/code39-blur-tilt-mask-2026-11-22.png");
  const databarTilt = resolve(pkg, "test-codes/regression/databar-blur-tilt-mask-2026-11-22.png");
  const ean8Noise = resolve(pkg, "test-codes/regression/ean8-blur-noise-mask-2026-11-22.png");
  const ean13Dim = resolve(pkg, "test-codes/regression/ean13-dim-tilt-2026-11-22.png");
  if (!existsSync(code32)) {
    console.log(`SKIP  ${project.padEnd(8)} regression no fixture at ${code32}`);
    return;
  }
  const context = await browser.newContext(project.context);
  /**
   * One still scan of a committed fixture through the demo page.
   * @param {string} fixture
   * @param {string} [query]
   * @returns {Promise<{ ms: number, value: string, status: string }>}
   */
  const scanFixture = async (fixture, query = "") => {
    const page = await context.newPage();
    const t0 = Date.now();
    try {
      await page.goto(`${ORIGIN}/still.html${query}`, { waitUntil: "load" });
      await page.setInputFiles("#file", fixture);
      await page.waitForFunction(
        () => {
          const status = document.getElementById("status")?.textContent ?? "";
          return status.startsWith("Read ") || status.startsWith("No code");
        },
        null,
        { timeout: 30_000 },
      );
      return {
        ms: Date.now() - t0,
        value: await page.locator("#value").textContent() ?? "",
        status: await page.locator("#status").textContent() ?? "",
      };
    } finally {
      await page.close();
    }
  };
  /** @param {string} name @param {() => Promise<void>} run */
  const check = async (name, run) => {
    const t0 = Date.now();
    try {
      await run();
      record({ project, suite: "regression", name, ok: true, ms: Date.now() - t0 });
    } catch (err) {
      record({
        project, suite: "regression", name, ok: false, ms: Date.now() - t0,
        note: String(/** @type {Error} */ (err)?.message ?? err).split("\n")[0],
      });
    }
  };

  await check("code32-aic", async () => {
    const { value } = await scanFixture(code32);
    if (value !== "A938261563") throw new Error(`code32: got ${JSON.stringify(value)}`);
  });

  if (existsSync(code128UpsideDown)) {
    // Real user photo (public by permission): a Code 128 label held upside
    // down, ~2.6 px/module. Every straight pass misses; the tight dense-row
    // band at 0.88, re-binarized from a mirrored SOURCE, reads it. Pins the
    // tight-band + source-mirror lever in `api/worker.js` / `src/roi.js`.
    await check("code128-upside-down-label", async () => {
      const { value } = await scanFixture(code128UpsideDown);
      if (value !== "GFIT26316112687249") {
        throw new Error(`code128 upside-down: got ${JSON.stringify(value)}`);
      }
    });
  }

  if (existsSync(code128SmallQuietZone)) {
    // Real user photo (public by permission) of the same label, a wider
    // crop that includes the camera frame: the printed quiet zone is ~6
    // modules where Code 128 mandates 10, and zxing reads it while the
    // row scanners rejected it. Pins the white-pad (`padDrawable`) lever
    // in `api/worker.js` / `src/roi.js`.
    await check("code128-small-quiet-zone", async () => {
      const { value } = await scanFixture(code128SmallQuietZone);
      if (value !== "GFIT26316112687249") {
        throw new Error(`code128 small quiet zone: got ${JSON.stringify(value)}`);
      }
    });
  }

  if (existsSync(blurTilt)) {
    // Pre-binarized mask of a synthetic 2 px/module EAN-13 with gaussian
    // blur + 6° roll: the whole-row 95-module quantizer rejected every row,
    // the per-digit variance fallback reads it. The fixture is the mask (not
    // the grayscale scene) so the gate is independent of the binarizer and
    // also runs on the CI's CPU-only Chromium; zxing misses this mask.
    await check("ean13-blur-tilt", async () => {
      const { value } = await scanFixture(blurTilt, "?type=ean13");
      if (value !== "4006381333931") throw new Error(`ean13-blur-tilt: got ${JSON.stringify(value)}`);
    });
  }

  if (existsSync(code39Tilt)) {
    // Mask of a synthetic 3 px/module Code 39 at 6° roll with blur. The
    // upright scanner misses it; zxing reads this very mask. Pins the
    // still-only small-roll source retry in api/worker.js.
    await check("code39-blur-tilt", async () => {
      const { value } = await scanFixture(code39Tilt, "?type=code39");
      if (value !== "ABC-1234") throw new Error(`code39-blur-tilt: got ${JSON.stringify(value)}`);
    });
  }

  if (existsSync(databarTilt)) {
    // Mask of a synthetic 2 px/module DataBar Omni at 6° roll with blur
    // (the GPU binarizer's 0.75 opinion). Every straight still pass misses
    // it; the ±3 source rotation in api/worker.js reads it. The fixture is
    // the mask, so the gate runs on the CI's CPU-only Chromium too.
    await check("databar-blur-tilt", async () => {
      const { value } = await scanFixture(databarTilt, "?type=databar");
      if (value !== "(01)40063813339314") throw new Error(`databar-blur-tilt: got ${JSON.stringify(value)}`);
    });
  }

  if (existsSync(ean8Noise)) {
    // Mask of a synthetic 2 px/module EAN-8 at 6° roll, blur 1.2 and luma
    // noise ±10. Its rows break the global 67-module quantizer but each
    // digit stays within the local variance; pins the per-digit fallback in
    // barcode-ean8 (`try_variance`). Verified load-bearing: this mask does
    // NOT decode without the fallback (0.82/0.92).
    await check("ean8-blur-noise", async () => {
      const { value } = await scanFixture(ean8Noise, "?type=ean8");
      if (value !== "96385074") throw new Error(`ean8-blur-noise: got ${JSON.stringify(value)}`);
    });
  }

  if (existsSync(ean13Dim)) {
    // Synthetic 2 px/module EAN-13 at 6° roll with blur, captured at
    // light 0.4 (label paper ~0.37, print ~0.04) — the dim class where
    // the GPU binarizer's auto-levels stretch costs module edges. The
    // still path re-draws the source brighter and reads it. CPU reads it
    // straight through its relative floor, so this check passes on both
    // paths while the lift is only load-bearing on the GPU one.
    await check("ean13-dim-tilt", async () => {
      const { value } = await scanFixture(ean13Dim, "?type=ean13");
      if (value !== "4006381333931") throw new Error(`ean13-dim-tilt: got ${JSON.stringify(value)}`);
    });
  }

  if (existsSync(box)) {
    const GS1 = "901050883\u001d100006316163\u001d";
    // A CI CPU runner needs more than the 5s still default for the
    // low-contrast opinion ladder (local: 3.4s, GitHub: 5.8s). These are
    // correctness checks; hard-negative-deadline pins the budget.
    await check("gs1-datamatrix", async () => {
      const { value } = await scanFixture(box, "?type=datamatrix&latency=20000");
      if (value !== GS1) throw new Error(`datamatrix: got ${JSON.stringify(value)}`);
    });
    await check("gs1-two-codes", async () => {
      // Default "everything": the DataMatrix is more centered than the
      // EAN-13, so the linear fast-path must not hide the 2D tier.
      const { value } = await scanFixture(box, "?latency=20000");
      if (value !== GS1) throw new Error(`two-codes: got ${JSON.stringify(value)}`);
    });
  }

  if (existsSync(pharma)) {
    // The user's real-world requirement: default "everything", no format
    // filter. The tiny ITF must also outrank the same-bars Code 128 false
    // positive (`0BJVcB`) that used to confirm instead. ~4.4s locally.
    await check("itf-pharma-all", async () => {
      const { value } = await scanFixture(pharma, "?latency=20000");
      if (value !== "0122508903") throw new Error(`itf: got ${JSON.stringify(value)}`);
    });
  }

  if (existsSync(pharmaClose)) {
    // Same stop-rule bug at native pitch: the label border shares the
    // barcode's rows, so the old "stop must be the last ink" rule missed
    // every frame.
    await check("itf-pharma-close", async () => {
      const { value } = await scanFixture(pharmaClose, "?type=itf&latency=10000");
      if (value !== "0122508903") throw new Error(`itf close: got ${JSON.stringify(value)}`);
    });
  }

  if (existsSync(hard)) {
    await check("hard-negative-deadline", async () => {
      // No decoder reads this QR; the point is that the still scan stops
      // at its budget instead of sweeping for ~23s.
      const { status, ms } = await scanFixture(hard, "?latency=1500");
      if (!status.startsWith("No code")) throw new Error(`unexpected read: ${status}`);
      if (ms > 12_000) throw new Error(`still scan took ${ms}ms with a 1500ms budget`);
    });
  }

  await context.close();
}

/** Real photos, local-only. Skips (does not fail) when the corpus is absent. */
async function suiteField(browser, project) {
  const corpus = resolve(pkg, "test-codes/field/corpus.json");
  if (!existsSync(corpus)) {
    console.log(`SKIP  ${project.padEnd(8)} field     no local corpus at ${corpus}`);
    return;
  }
  const context = await browser.newContext(project.context);
  const page = await context.newPage();
  const t0 = Date.now();
  const url = `${TESTS_FS}/field.html?corpus=/@fs${corpus}&mode=still`;
  await page.goto(url, { waitUntil: "load" });
  const title = await waitTitle(page, "TESTS PASS", "TESTS FAILED", 600_000);
  const table = await page.evaluate(() =>
    [...document.querySelectorAll("#rows tr")].map((tr) => [...tr.children].map((td) => td.textContent)),
  );
  let fails = 0;
  for (const [id, mode, sym, want, got, ms, result, note] of table) {
    const ok = result === "PASS";
    const xfail = result === "XFAIL";
    if (!ok && !xfail) fails++;
    record({ project, suite: "field", name: String(id), ok, xfail, ms: Number.parseInt(String(ms), 10) || 0, note: `${sym} ${JSON.stringify(got)}${note ? ` ${note}` : ""}` });
  }
  record({ project, suite: "field", name: "total", ok: title === "TESTS PASS" && fails === 0, ms: Date.now() - t0, note: "" });
  await context.close();
}

// --- run ------------------------------------------------------------------
let failed = 0;
try {
  for (const project of PROJECTS) {
    console.log(`\n# ${project.name} (${project.engine})`);
    const browserType = project.engine === "webkit" ? webkit : chromium;
    const browser = await browserType.launch({ headless: true });
    try {
      if (!ONLY_FAKECAM) {
        const context = await browser.newContext(project.context);
        const page = await context.newPage();
        await suiteArtifact(browser, project);
        await suiteSelftest(page, project.name);
        await suiteContract(page, project.name);
        await suiteFuzz(page, project.name);
        await context.close();
        await suiteDocs(browser, project.name);
        await suiteRegression(browser, project.name);
      }
      await suiteFakecam(browser, project.name);
      if (FIELD) await suiteField(browser, project.name);
    } finally {
      await browser.close();
    }
  }
} finally {
  await vite.close();
  staticServer.close();
}

failed = rows.filter((r) => !r.ok && !r.xfail).length;
const pass = rows.filter((r) => r.ok).length;
const xfail = rows.filter((r) => r.xfail).length;
console.log(`\n== ${pass} PASS, ${failed} FAIL, ${xfail} XFAIL (${rows.length} checks)`);
process.exit(failed ? 1 : 0);
