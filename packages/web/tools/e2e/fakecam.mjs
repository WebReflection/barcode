#!/usr/bin/env node
/**
 * Fake-camera live-path e2e: the integration tier without physical
 * aiming. A headless Chrome tab gets a getUserMedia mock (installed via
 * Page.addScriptToEvaluateOnNewDocument, BEFORE scan.js runs) that
 * builds a specimen with our own render pipeline — worker, wasm, the
 * real encoders — rasterizes it onto a canvas, and returns
 * canvas.captureStream() as the "camera". tests/scan-fakecam.html then runs its real
 * UI flow end to end: camera open, frames, decode tier, confirmation
 * law, result card.
 *
 *   node tools/e2e/fakecam.mjs code39 ABC-123        # single case
 *   node tools/e2e/fakecam.mjs --all                 # the built-in set
 *   node tools/e2e/fakecam.mjs --all --port 9222     # a connected phone
 *                                                    # (adb forward 9226 too)
 *
 * Records JSONL to $E2E_LOG (default /tmp/opencode/fakecam-e2e.jsonl).
 * The physical phone rig (run.mjs) stays the authority for camera
 * hardware questions (focus, glare, fps); this tier pins the software
 * flow so regressions in it are caught on every run.
 */
import { writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const rawArgs = process.argv.slice(2);
/** Pull a `--flag value` pair out of the arguments. */
const takeOption = (/** @type {string[]} */ list, /** @type {string} */ flag) => {
  const i = list.indexOf(flag);
  if (i < 0) return null;
  const value = list[i + 1] ?? null;
  list.splice(i, 2);
  return value;
};
const argList = rawArgs.slice();
const PORT = Number(takeOption(argList, "--port") ?? 9226);
// --shot FILE: save a phone/desktop screenshot of the result card on PASS.
const SHOT = takeOption(argList, "--shot");
const args = argList;
const OUT = process.env.E2E_LOG ?? "/tmp/opencode/fakecam-e2e.jsonl";
const TIMEOUT_MS = 20_000;
// Tracked known failures: printed XFAIL (and kept in the JSONL) instead of
// reddening the suite. Keep the reason specific and re-check when touching
import { CASES, XFAIL, MOCK } from "./fakecam-spec.mjs";

const ORIGIN = process.env.E2E_ORIGIN ?? "http://localhost:5173";

class Cdp {
  constructor(port) { this.port = port; this.id = 0; this.pending = new Map(); }
  async connect(urlSubstr) {
    const targets = await (await fetch(`http://localhost:${this.port}/json`)).json();
    this.target = targets.find((t) => t.type === "page" && t.url.includes(urlSubstr));
    if (!this.target) throw new Error(`no page matching ${urlSubstr} on :${this.port} — have: ${targets.map((t) => t.url).join(", ")}`);
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

// --- browser management ---------------------------------------------------
const version = await fetch(`http://localhost:${PORT}/json/version`).then((r) => r.json()).catch(() => null);
if (!version) {
  console.error(`no headless chrome on :${PORT} — launch:
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new \\
    --remote-debugging-port=${PORT} --user-data-dir=/tmp/opencode/chrome-fakecam \\
    --no-first-run --autoplay-policy=no-user-gesture-required about:blank`);
  process.exit(2);
}

/** Desktop: an isolated tab we close at the end. Android Chrome refuses
 * `/json/new`, so fall back to navigating an existing page target. */
let tab = null;
let ownTab = false;
try {
  const created = await (await fetch(`http://localhost:${PORT}/json/new?about:blank`, { method: "PUT" })).json();
  if (created?.id) {
    tab = created;
    ownTab = true;
  }
} catch {
  // Android (or an older Chrome): reuse an existing tab below.
}
if (!tab) {
  const targets = await (await fetch(`http://localhost:${PORT}/json`)).json();
  tab = targets.find((t) => t.type === "page");
  if (!tab) {
    console.error(`no page target on :${PORT} — open a tab first`);
    process.exit(2);
  }
}
const page = new Cdp(PORT);
{
  const targets = await (await fetch(`http://localhost:${PORT}/json`)).json();
  const t = targets.find((x) => x.id === tab.id);
  if (!t) throw new Error("created tab vanished");
  page.target = t;
  page.ws = new WebSocket(t.webSocketDebuggerUrl);
  page.ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && page.pending.has(msg.id)) {
      const { resolve, reject } = page.pending.get(msg.id);
      page.pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    }
  };
  await new Promise((res, rej) => { page.ws.onopen = res; page.ws.onerror = rej; });
}
await page.send("Page.enable");
await page.send("Page.addScriptToEvaluateOnNewDocument", { source: MOCK });

const cases = args[0] === "--all"
  ? CASES
  : (() => {
      if (args.length < 2) { console.error("usage: fakecam.mjs <symbology> <text> | --all"); process.exit(2); }
      return [[args[0], args[1]]];
    })();

for (const [s, t] of cases) {
  // inverted-/rotated-/diagonal-/tinyNN-<sym> are fixture variants, not
  // decoder formats
  const format = s
    .replace(/^inverted-/, "")
    .replace(/^island-/, "")
    .replace(/^rotated-/, "")
    .replace(/^diagonal-/, "")
    .replace(/^tiny\d+-/, "");
  const max = s.startsWith("tiny") ? "&max=1056" : "";
  const TESTS = fileURLToPath(new URL("../../tests/", import.meta.url));
  const url = `${ORIGIN}/@fs${TESTS}scan-fakecam.html?formats=${encodeURIComponent(format)}&e2e=${encodeURIComponent(`${s}:${t}`)}${max}&_=${Date.now()}`;
  await page.send("Page.navigate", { url });
  await sleep(1500);
  const started = await page.eval(`(function () {
    const b = document.getElementById("start");
    if (!b) return "no start button";
    b.click();
    return true;
  })()`);
  if (started !== true) {
    appendFileSync(OUT, JSON.stringify({ s, t, ok: false, err: started }) + "\n");
    console.log("SKIP", s, started);
    continue;
  }
  const t0 = Date.now();
  // A tracked known failure is expected to time out; do not spend the
  // full 20s on it every run (the inverted-Aztec XFAIL was 20.3s of a
  // ~45s `--all` gate). If a fix lands it reads in <1s like its
  // upright/diagonal siblings, so 6s still catches a real improvement.
  const caseTimeout = XFAIL.has(s) ? 6_000 : TIMEOUT_MS;
  let got = null, sym = null, matched = false, lastStatus = "";
  while (Date.now() - t0 < caseTimeout) {
    const st = await page.eval(`(function () {
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
    if (st?.visible && st.value) {
      const digitsOnly = st.value.replace(/^\(\d+\)/, "").replace(/\D/g, "");
      if (st.value === t || (/^\d+$/.test(t) && digitsOnly.startsWith(t))) {
        got = st.value; sym = st.sym; matched = true; break;
      }
    }
    await sleep(400);
  }
  const rec = { s, t, got, sym, ok: matched, ms: Date.now() - t0, status: lastStatus };
  const xfail = matched ? undefined : XFAIL.get(s);
  appendFileSync(OUT, JSON.stringify({ ...rec, xfail }) + "\n");
  console.log(rec.ok ? "PASS" : xfail ? "XFAIL" : "FAIL", JSON.stringify({ ...rec, xfail }));
  if (matched && SHOT) {
    await sleep(700); // let the result card lay out with its SVG
    const shot = await page.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(SHOT, Buffer.from(shot.data, "base64"));
    console.log("SHOT", SHOT);
  }
  if (!matched && process.env.FAKECAM_DUMP) {
    // Debug: the last binarized frame, its ink fraction, and both
    // polarities through the wasm API (what the scanner saw).
    const diag = await page.eval(`(async () => {
      try {
        const base = '/@fs' + ${JSON.stringify(process.cwd())};
        const scanMod = await import('/scan.js');
        if (!scanMod.debugState) {
          return {
            status: document.getElementById('status')?.textContent,
            frame: null,
            note: 'scan-fakecam.html is the internal harness; open demo/index.html for the public playground',
          };
        }
        const { debugState } = scanMod;
        const wasm = await import(base + '/src/wasm/pkg/barcode_wasm.js');
        const wasmUrl = (await import(base + '/src/wasm/pkg/barcode_wasm_bg.wasm?url')).default;
        await wasm.default(wasmUrl);
        const st = debugState();
        if (!st.frame) return { status: document.getElementById('status')?.textContent, frame: null };
        const { width: W, height: H, bytesPerRow: BPR, data } = st.frame;
        let ones = 0; for (const b of data) ones += b.toString(2).split('1').length - 1;
        const inv = new Uint8Array(data.length);
        for (let i = 0; i < data.length; i++) inv[i] = ~data[i] & 0xff;
        const sc = new wasm.BarcodeScanner();
        const arr = Uint8Array.from(data);
        const fmt = ['code128', 'qrcode', 'datamatrix', 'aztec', 'pdf417'];
        return {
          status: document.getElementById('status')?.textContent,
          W, H, BPR, ink: ones/(W*H),
          results: st.results,
          stillRaw: sc.decode_filtered_still(W, H, arr, fmt, 3000).map(r => r.text),
          stillInv: sc.decode_filtered_still(W, H, inv, fmt, 3000).map(r => r.text),
          pngRaw: (() => { const c = document.createElement('canvas'); c.width = W; c.height = H;
            const ctx = c.getContext('2d'); ctx.fillStyle = '#fff'; ctx.fillRect(0,0,W,H); ctx.fillStyle = '#000';
            for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) if ((data[y*BPR + (x>>3)] >> (x&7)) & 1) ctx.fillRect(x,y,1,1);
            return c.toDataURL('image/png'); })(),
        };
      } catch (e) { return { error: String(e) }; }
    })()`);
    if (diag?.pngRaw) {
      const file = `/tmp/opencode/fail-${s}.png`;
      writeFileSync(file, Buffer.from(diag.pngRaw.split(',')[1], 'base64'));
      delete diag.pngRaw;
      diag.file = file;
    }
    console.error("DIAG", JSON.stringify(diag));
  }
}
// Leave no tab behind: a full sweep opens one case tab per run and
// headless Chrome keeps every one of them alive (workers included),
// which eventually wedges the CDP target list.
if (ownTab) await fetch(`http://localhost:${PORT}/json/close/${tab.id}`).catch(() => {});
process.exit(0);
