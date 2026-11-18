#!/usr/bin/env node
/**
 * Field-corpus regression driver (roadmap item 9).
 *
 * Replays the personal field corpus (real photos + hand-confirmed ground
 * truth) through `demo/field.html`, which runs the SHIPPED artifacts
 * (main.js -> worker.js -> wasm.wasm) or the source API (`--dev`). This
 * is the gate that answers "do the codes people actually scan still
 * read, and within budget?".
 *
 * The corpus itself is local-only: `test-codes/field/corpus.json`
 * (gitignored, see tools/e2e/corpus.example.json for the format). When it
 * is absent the run SKIPS with exit 0 — `--require` turns a missing
 * corpus (or an empty/unknown `--id` selection) into exit 2, for a local
 * nightly gate that must not silently pass.
 *
 *   node tools/e2e/field.mjs                       # still, desktop :9225
 *   node tools/e2e/field.mjs --mode live           # fake camera -> live path
 *   node tools/e2e/field.mjs --port 9222           # same gate ON THE PHONE
 *   node tools/e2e/field.mjs --everything           # default UI (no formats)
 *   node tools/e2e/field.mjs --id invoice-dm-mask
 *
 * On-device: `adb forward tcp:9222 tcp:9222` + `adb reverse tcp:5173
 * tcp:5173`, open http://localhost:5173/ on the phone once, then run with
 * `--port 9222`. The corpus JSON is fetched from the Mac's vite dev
 * server, so photos stay on the Mac (and out of the repo).
 */
import { appendFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const argv = process.argv.slice(2);
/** @param {string} name @param {string | undefined} fallback */
const flag = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 ? (argv[i + 1] ?? fallback) : fallback;
};
const has = (name) => argv.includes(`--${name}`);

const PORT = flag("port", "9225");
const BASE = flag("base", "http://localhost:5173").replace(/\/$/, "");
const MODE = flag("mode", "still") === "live" ? "live" : "still";
const ONLY = flag("id");
const EVERYTHING = has("everything");
const DEV = has("dev");
const REQUIRED = has("require");
const TIMEOUT_MS = Number(flag("timeout", MODE === "live" ? "600000" : "300000"));
const OUT = flag("jsonl", process.env.E2E_LOG ?? "/tmp/opencode/field-corpus-e2e.jsonl");
mkdirSync(dirname(OUT), { recursive: true });

const localCorpus = fileURLToPath(new URL("../../test-codes/field/corpus.json", import.meta.url));
const corpusArg = flag("corpus");
const corpusUrl = corpusArg ?? `/@fs${localCorpus}`;
/** Published bundle by default; `--dev` boots api/main.js (no rebuild). */
const apiAbs = fileURLToPath(new URL(DEV ? "../../api/main.js" : "../../dist/main.js", import.meta.url));
const localCheck = corpusArg ? (corpusArg.startsWith("/@fs") ? corpusArg.slice(4) : null) : localCorpus;
if (localCheck && !existsSync(localCheck)) {
  const msg = `no local field corpus at ${localCheck} — personal fixtures are gitignored (see tools/e2e/corpus.example.json)`;
  if (REQUIRED) {
    console.error(`== FAIL (${msg})`);
    process.exit(2);
  }
  console.log(`== SKIP (${msg})`);
  process.exit(0);
}

const version = await fetch(`http://localhost:${PORT}/json/version`).then((r) => r.json()).catch(() => null);
if (!version) {
  console.error(`no Chrome DevTools endpoint on :${PORT} — a headless Chrome (desktop) or adb-forwarded phone is required`);
  process.exit(2);
}

/**
 * Desktop: create our own tab so the run is isolated and close it after.
 * Android Chrome refuses `/json/new` ("Could not create new page"), so
 * fall back to navigating an existing page target instead.
 */
let target = null;
let ownTab = false;
try {
  const created = await fetch(`http://localhost:${PORT}/json/new?about:blank`, { method: "PUT" }).then((r) => r.json());
  if (created?.webSocketDebuggerUrl) {
    target = created;
    ownTab = true;
  }
} catch {
  // Android (or an older Chrome): use an existing tab below.
}
if (!target) {
  const targets = await fetch(`http://localhost:${PORT}/json`).then((r) => r.json());
  target = targets.find((t) => t.type === "page");
  if (!target) {
    console.error(
      `no page target on :${PORT} — open a tab first ` +
      `(phone: adb shell am start -a android.intent.action.VIEW -d http://localhost:5173/demo.html)`,
    );
    process.exit(2);
  }
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.addEventListener("open", res);
  ws.addEventListener("error", rej);
});
let id = 0;
/** @type {Map<number, { resolve: (v: any) => void, reject: (e: Error) => void }>} */
const pending = new Map();
ws.addEventListener("message", (ev) => {
  const d = JSON.parse(ev.data);
  if (d.id && pending.has(d.id)) {
    const { resolve, reject } = pending.get(d.id);
    pending.delete(d.id);
    d.error ? reject(new Error(JSON.stringify(d.error))) : resolve(d.result);
  }
});
/** @param {string} method @param {object} [params] */
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const mid = ++id;
    pending.set(mid, { resolve, reject });
    ws.send(JSON.stringify({ id: mid, method, params }));
  });

const params = new URLSearchParams({ corpus: corpusUrl, mode: MODE, api: `/@fs${apiAbs}` });
if (ONLY) params.set("id", ONLY);
if (EVERYTHING) params.set("formats", "all");
const pageUrl = `${BASE}/field.html?${params}`;

let exitCode = 0;
try {
  await send("Page.enable");
  await send("Runtime.enable");
  await send("Page.navigate", { url: pageUrl });
  console.log(`corpus: ${corpusUrl}\nmode:   ${MODE}${EVERYTHING ? " (all formats)" : ""}${DEV ? " (dev source)" : " (published bundle)"}\npage:   ${pageUrl}`);

  const t0 = Date.now();
  let title = "";
  let lastSummary = "";
  while (Date.now() - t0 < TIMEOUT_MS) {
    await new Promise((r) => setTimeout(r, 500));
    const t = await send("Runtime.evaluate", { expression: "document.title", returnByValue: true });
    title = t.result?.value ?? "";
    const s = await send("Runtime.evaluate", {
      expression: "document.getElementById('summary')?.textContent ?? ''",
      returnByValue: true,
    });
    lastSummary = s.result?.value ?? "";
    if (lastSummary && lastSummary !== "running…" && (title === "TESTS PASS" || title === "TESTS FAILED" || title === "TESTS SKIP")) break;
  }
  if (title !== "TESTS PASS" && title !== "TESTS FAILED" && title !== "TESTS SKIP") {
    console.error(`TIMEOUT after ${Math.round((Date.now() - t0) / 1000)}s — last summary: ${lastSummary}`);
    exitCode = 1;
  } else {
    const rows = await send("Runtime.evaluate", {
      expression: "[...document.querySelectorAll('tbody tr')].map((tr) => [...tr.children].map((td) => td.textContent))",
      returnByValue: true,
    });
    /** @type {string[][]} */
    const table = rows.result?.value ?? [];
    let fails = 0;
    for (const r of table) {
      const [caseId = "", mode = "", sym = "", want = "", got = "", ms = "", result = "", note = ""] = r;
      const pass = result === "PASS";
      const xfail = result === "XFAIL";
      if (!pass && !xfail) fails++;
      console.log(
        `${pass ? "PASS" : xfail ? "XFAIL" : "FAIL"}  ${caseId.padEnd(30)} ${mode.padEnd(5)} ${sym.padEnd(11)} ` +
        `${JSON.stringify(want).padEnd(44)} got=${JSON.stringify(got)} ${ms.padStart(6)} ${note}`,
      );
      appendFileSync(OUT, JSON.stringify({ id: caseId, mode, sym, want, got, ms: Number.parseInt(ms, 10), pass, xfail, note }) + "\n");
    }
    console.log(lastSummary);
    // Trust the page: a page-level failure with zero rows (import crash,
    // corpus parse) must still fail the run.
    exitCode = title === "TESTS SKIP" ? (REQUIRED ? 2 : 0) : (title === "TESTS FAILED" || fails ? 1 : 0);
  }
} finally {
  // `process.exit` would skip this: always close OUR tab so repeated
  // runs cannot wedge the target (the fakecam lesson).
  ws.close();
  if (ownTab) await fetch(`http://localhost:${PORT}/json/close/${target.id}`).catch(() => {});
}
process.exit(exitCode);
