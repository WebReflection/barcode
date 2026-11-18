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
 * It starts Vite itself (source pages: /test.html, /contract-test.html,
 * /scan.html) plus a static server for the built /dist payload
 * (/demo/artifact-test.html), so no external Chrome/CDP setup is needed.
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
  const i = argv.indexOf(`--${name}`);
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
  for (const [name, url, pass] of [
    ["gpu-or-cpu", `${ORIGIN}/test.html`, "TESTS PASS"],
    ["cpu", `${ORIGIN}/test.html?bin=cpu`, "TESTS PASS"],
  ]) {
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
  await page.goto(`${ORIGIN}/contract-test.html`, { waitUntil: "load" });
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

/** The built dist/ payload, plain static hosting (no bundler). */
async function suiteArtifact(browser, project) {
  const context = await browser.newContext(project.context);
  const page = await context.newPage();
  const t0 = Date.now();
  await page.goto(`${STATIC}/packages/web/demo/artifact-test.html`, { waitUntil: "load" });
  const title = await waitTitle(page, "ARTIFACT PASS", "ARTIFACT FAIL", 60_000);
  const result = await page.evaluate(() => globalThis.__RESULT);
  record({ project, suite: "artifact", name: "dist/ static", ok: title === "ARTIFACT PASS", ms: Date.now() - t0, note: result ?? "" });
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
  if (!canCapture) {
    console.log(`SKIP  ${project.padEnd(8)} fakecam     canvas.captureStream unavailable in this engine`);
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
    const url = `${ORIGIN}/scan.html?formats=${encodeURIComponent(format)}&e2e=${encodeURIComponent(`${s}:${t}`)}${max}&_=${Date.now()}`;
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
    if (links < 6) throw new Error(`only ${links} demo cards`);
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

  if (project === "chromium") {
    await check("bundled self-test", async () => {
      await page.goto(`${STATIC}/docs/test.html`, { waitUntil: "load" });
      const title = await waitTitle(page, "TESTS PASS", "TESTS FAILED", 240_000);
      if (title !== "TESTS PASS") {
        const summary = await page.locator("#summary").textContent();
        throw new Error(summary ?? "self-test failed");
      }
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
  const url = `${ORIGIN}/field.html?corpus=/@fs${corpus}&mode=still`;
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
        await context.close();
        await suiteDocs(browser, project.name);
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
