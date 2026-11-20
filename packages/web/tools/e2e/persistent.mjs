#!/usr/bin/env node
// Persistent-camera DM test: the phone's scanner opens ONCE and never
// restarts; only the show window flips fixtures. Measures flip->decode
// latency per case in the focused-camera regime (the real-world one).
//
//   node tools/e2e/persistent.mjs <case.json or implicit DM set>
import { readFileSync } from "node:fs";

const cases = [
  { s: "datamatrix", t: "DM-PHYS-001" },
  { s: "datamatrix", t: "DM-RECT-001" },
  { s: "datamatrix", t: "DM-BIG-001" },
  { s: "datamatrix", t: "DATA MATRIX LONGER PAYLOAD 42" },
];

class Cdp {
  constructor(port, urlSubstr) { this.port = port; this.urlSubstr = urlSubstr; this.id = 0; this.pending = new Map(); }
  async connect() {
    const targets = await (await fetch(`http://localhost:${this.port}/json`)).json();
    this.target = targets.find((t) => t.type === "page" && t.url.includes(this.urlSubstr));
    if (!this.target) throw new Error(`no page matching ${this.urlSubstr} on :${this.port}`);
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
const tests = fileURLToPath(new URL("../../tests/", import.meta.url));
const show = new Cdp(9223, `/@fs${tests}show.html`);
const phone = new Cdp(9222, "/scan.html");
await show.connect();
await phone.connect();

// Open the scanner ONCE.
await phone.send("Page.navigate", { url: "http://localhost:5173/scan.html?formats=datamatrix" });
await sleep(1500);
const started = await phone.eval(`(function () {
  const b = document.getElementById("start");
  if (!b) return "no start button";
  b.click(); return true;
})()`);
if (started !== true) { console.error("start failed:", started); process.exit(2); }
console.log("scanner open — holding camera for the whole run");

const TIMEOUT = 30_000;
let pass = 0;
for (const c of cases) {
  await show.send("Page.navigate", { url: `http://localhost:5173/@fs${tests}show.html?s=${c.s}&t=${encodeURIComponent(c.t)}&fill=0.6` });
  const t0 = Date.now();
  let got = null;
  while (Date.now() - t0 < TIMEOUT) {
    const st = await phone.eval(`(function () {
      const card = document.getElementById("result");
      const visible = card && card.style.display !== "none";
      return visible ? document.getElementById("value").textContent : null;
    })()`);
    if (st === c.t) { got = st; break; }
    await sleep(250);
  }
  const ms = Date.now() - t0;
  if (got !== null) { pass++; console.log(`PASS ${String(ms / 1000).padStart(6, " ")}s  ${c.t}`); }
  else console.log(`FAIL ${TIMEOUT / 1000}.0s  ${c.t}`);
  // clear the result card so the next case waits for a fresh decode
  await phone.eval(`(function () {
    const b = document.getElementById("again");
    if (b) b.click(); return true;
  })()`);
  await sleep(500);
}
console.log(`--- ${pass}/${cases.length} pass (persistent camera)`);
process.exit(0);
