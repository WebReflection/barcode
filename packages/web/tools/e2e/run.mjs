#!/usr/bin/env node
/**
 * Device-to-device e2e sweep driver (the README's "integration tier").
 *
 *   Mac Chrome window (debug :9223)  displays /demo/show.html?s=X&t=Y
 *   Phone Chrome      (debug :9222)  runs   /demo/scan.html, camera at screen
 *
 * Per case: navigate the show window -> poll the phone's #result card until
 * it decodes (or timeout) -> record. Wrong/no decode -> tap "Scan again"
 * and keep polling. Results append to a JSONL corpus (input for the
 * decode-hardening milestone).
 *
 * Usage:
 *   node tools/e2e/run.mjs ean13 4006381333931        # single smoke case
 *   node tools/e2e/run.mjs --all tools/e2e/cases.json # full sweep
 * Setup (see notes/STATUS.md): adb forward/reverse, vite on :5173,
 * camera granted (cam_grant.mjs), scanner started once.
 */
import { readFileSync, appendFileSync } from "node:fs";

const SHOW_PORT = 9223; // Mac display window
const PHONE_PORT = 9222; // phone via adb forward
const OUT = process.env.E2E_LOG ?? "/tmp/opencode/e2e-corpus.jsonl";
const TIMEOUT_MS = 25_000;

const args = process.argv.slice(2);
let cases;
if (args[0] === "--all") {
  cases = JSON.parse(readFileSync(args[1] ?? new URL("./cases.json", import.meta.url), "utf8"));
} else {
  if (args.length < 2) { console.error("usage: run.mjs <symbology> <text> | --all [cases.json]"); process.exit(2); }
  cases = [{ s: args[0], t: args[1] }];
}

class Cdp {
  constructor(port, urlSubstr) { this.port = port; this.urlSubstr = urlSubstr; this.id = 0; this.pending = new Map(); }
  async connect() {
    const targets = await (await fetch(`http://localhost:${this.port}/json`)).json();
    this.target = targets.find((t) => t.type === "page" && t.url.includes(this.urlSubstr));
    if (!this.target) throw new Error(`no page matching ${this.urlSubstr} on :${this.port} — have: ${targets.map((t) => t.url)}`);
    this.ws = new WebSocket(this.target.webSocketDebuggerUrl);
    this.ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
      }
    };
    await new Promise((res, rej) => { this.ws.onopen = res; this.ws.onerror = rej; });
  }
  send(method, params) {
    return new Promise((resolve, reject) => {
      const msgId = ++this.id;
      this.pending.set(msgId, { resolve, reject });
      this.ws.send(JSON.stringify({ id: msgId, method, params }));
    });
  }
  async eval(expr) {
    const r = await this.send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return r.result?.value;
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const show = new Cdp(SHOW_PORT, "/show.html");
const phone = new Cdp(PHONE_PORT, "/scan.html");
await show.connect();
await phone.connect();
console.log(`show: ${show.target.url}\nphone: ${phone.target.url}`);

for (const c of cases) {
  if (c.hex) {
    // binary specimens have no printable text to show/compare on the
    // phone bench; they are covered by the selftest pipeline instead
    console.log(`SKIP ${c.s} hex (binary fixture)`);
    continue;
  }
  // Put the code on the Mac screen first.
  const fill = process.env.E2E_FILL ? `&fill=${process.env.E2E_FILL}` : "";
  const mod = process.env.E2E_MODULE ? `&m=${process.env.E2E_MODULE}` : "";
  await show.send("Page.navigate", { url: `http://localhost:5173/show.html?s=${encodeURIComponent(c.s)}&t=${encodeURIComponent(c.t)}${fill}${mod}` });
  await sleep(1200);

  // Fresh scanner per case, filtered to the symbology under test (junk 2D
  // hits fire onFound instantly and would otherwise kill the sweep) —
  // "#again" only resets the UI, it does not restart the pipeline, so a
  // page reload per case is the reliable restart.
  const acc = process.env.E2E_ACCURACY ? `&accuracy=${process.env.E2E_ACCURACY}` : "";
  await phone.send("Page.navigate", { url: `http://localhost:5173/scan.html?formats=${encodeURIComponent(c.s)}${acc}` });
  await sleep(1500);
  const started = await phone.eval(`(function () {
    const b = document.getElementById("start");
    if (!b) return "no start button";
    b.click();
    return true;
  })()`);
  if (started !== true) {
    const skip = { s: c.s, t: c.t, ok: false, err: started };
    appendFileSync(OUT, JSON.stringify(skip) + "\n");
    console.log("SKIP", JSON.stringify(skip));
    continue;
  }

  const t0 = Date.now();
  let got = null;
  let sym = null;
  let matched = false;
  let lastStatus = "";
  let polls = 0;
  while (Date.now() - t0 < TIMEOUT_MS) {
    const st = await phone.eval(`(function () {
      const card = document.getElementById("result");
      const visible = card && card.style.display !== "none";
      return {
        visible,
        sym: visible ? document.getElementById("symbology").textContent : null,
        value: visible ? document.getElementById("value").textContent : null,
        status: document.getElementById("status")?.textContent ?? "",
      };
    })()`);
    lastStatus = st?.status ?? lastStatus;
    if (polls++ < 4) console.log("  poll:", JSON.stringify(st));
    if (st?.visible && st.value) {
      // Symbologies with AI formatting (databar returns "(01)...") compare
      // digit-wise: drop the AI prefix, then the expected digits must prefix
      // the remainder (body + check digit).
      const digitsOnly = st.value.replace(/^\(\d+\)/, "").replace(/\D/g, "");
      if (st.value === c.t || (/^\d+$/.test(c.t) && digitsOnly.startsWith(c.t))) {
        got = st.value; sym = st.sym; matched = true; break;
      }
    }
    await sleep(700);
  }
  const rec = { s: c.s, t: c.t, got, sym, ok: matched, ms: Date.now() - t0, status: lastStatus };
  appendFileSync(OUT, JSON.stringify(rec) + "\n");
  console.log(rec.ok ? "PASS" : "FAIL", JSON.stringify(rec));
}
process.exit(0);
