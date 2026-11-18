#!/usr/bin/env node
/**
 * Synthetic hardening sweep runner: spawns HEADLESS Chrome (no window — the
 * physical rig is retired), loads /harden.html through vite, polls for the
 * result, and writes JSON + a summary to stdout.
 *
 * Prereq: vite serving packages/web (npx vite --host --port 5173).
 * Usage: node tools/harden.mjs [--out FILE] [--quick]
 * Chrome binary: env CHROME_BIN overrides the stable path.
 */
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const outArg = args.indexOf("--out");
const OUT = outArg >= 0 ? args[outArg + 1] : "/tmp/opencode/harden-results.json";
const QUICK = args.includes("--quick") ? "?quick=1" : "";
const PAGE = `http://localhost:5173/harden.html${QUICK}`;
const DEBUG_PORT = 9225;
const CHROME =
  process.env.CHROME_BIN ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

// 1. vite must be up — the page imports src modules.
try {
  const r = await fetch(PAGE.replace(OUT, ""), { method: "HEAD" });
  if (!r.ok) throw new Error(String(r.status));
} catch {
  console.error(`vite is not serving ${PAGE} — run: npx vite --host --port 5173 (from packages/web)`);
  process.exit(1);
}

// 2. Headless Chrome (its own profile dir; never a visible window).
const chrome = spawn(CHROME, [
  "--headless=new",
  `--remote-debugging-port=${DEBUG_PORT}`,
  "--user-data-dir=/tmp/opencode/harden-chrome",
  "--no-first-run",
  "--no-default-browser-check",
  "--window-size=1000,800",
  "about:blank",
], { stdio: "ignore" });
const killChrome = () => { try { chrome.kill("SIGKILL"); } catch { /* gone */ } };
process.on("exit", killChrome);

let targets;
for (let i = 0; i < 40; i++) {
  await new Promise((r) => setTimeout(r, 250));
  try {
    targets = await (await fetch(`http://localhost:${DEBUG_PORT}/json`)).json();
    if (targets) break;
  } catch { /* not up yet */ }
}
if (!targets) { console.error("headless chrome did not come up"); process.exit(1); }

// 3. New tab on the sweep page (PUT required since Chrome 111).
const tab = await (
  await fetch(`http://localhost:${DEBUG_PORT}/json/new?${encodeURIComponent(PAGE)}`, { method: "PUT" })
).json();

const ws = new WebSocket(tab.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
const send = (m, p) => new Promise((res, rej) => {
  const mid = ++id;
  pending.set(mid, { res, rej });
  ws.send(JSON.stringify({ id: mid, method: m, params: p }));
});
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    const { res, rej } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? rej(new Error(JSON.stringify(msg.error))) : res(msg.result);
  }
};
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
await send("Runtime.enable");

// 4. Poll until the sweep finishes (page sets __HARDEN_DONE).
const deadline = Date.now() + 10 * 60_000;
let result = null, error = null;
while (Date.now() < deadline) {
  await new Promise((r) => setTimeout(r, 1500));
  const { result: v } = await send("Runtime.evaluate", {
    expression: "({done: !!window.__HARDEN_DONE, res: window.__HARDEN ?? null, err: window.__HARDEN_ERROR ?? null})",
    returnByValue: true,
  });
  const val = v.value;
  if (val.err) { error = val.err; break; }
  if (val.done) { result = val.res; break; }
  process.stdout.write(".");
}
if (error) { console.error("\nsweep failed:", error); process.exit(1); }
if (!result) { console.error("\ntimed out waiting for the sweep"); process.exit(1); }

writeFileSync(OUT, JSON.stringify(result, null, 1));
console.log(`\nwrote ${OUT}\n`);
console.log("symbology  single  any-of-3");
for (const c of result.cases) {
  const s = (100 * c.single.hits / c.single.of).toFixed(0).padStart(5);
  const a = (100 * c.any.hits / c.any.of).toFixed(0).padStart(5);
  console.log(`${c.s.padEnd(10)} ${s}%  ${a}%   (${c.s === "qrcode" ? c.t.slice(0, 20) + "…" : c.t})`);
}
process.exit(0);
