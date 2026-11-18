/**
 * Watch for a successful decode: sets zoom, logs focus state, screenshots
 * on success. Usage: node tools/watch.mjs [zoom] [maxMinutes]
 */

const ORIGIN = "http://localhost:5173";
const ZOOM = parseFloat(process.argv[2] ?? "2.5");
const MAX_MIN = parseFloat(process.argv[3] ?? "3");

const targets = await fetch("http://localhost:9222/json").then((r) => r.json());
const page = targets.find((t) => t.type === "page" && t.url.startsWith(ORIGIN + "/index.html"))
  ?? targets.find((t) => t.type === "page" && t.url.startsWith(ORIGIN));
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.addEventListener("open", r); ws.addEventListener("error", j); });
let id = 0;
const send = (m, p) => new Promise((res, rej) => {
  const mid = ++id;
  const to = setTimeout(() => rej(new Error("timeout: " + m)), 30000);
  const h = (ev) => { const d = JSON.parse(ev.data); if (d.id === mid) { clearTimeout(to); ws.removeEventListener("message", h); d.error ? rej(new Error(JSON.stringify(d.error))) : res(d.result); } };
  ws.addEventListener("message", h);
  ws.send(JSON.stringify({ id: mid, method: m, params: p }));
});
await send("Runtime.enable");
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) return "EXC: " + JSON.stringify(r.exceptionDetails).slice(0, 200);
  return r.result?.value;
};

for (let i = 0; i < 20; i++) {
  if ((await evaluate(`(async () => { const m = await import(document.querySelector('script[type="module"]').src); return typeof m.debugState().scanner; })()`)) === "object") break;
  await new Promise((r) => setTimeout(r, 1000));
}
console.log("zoom:", await evaluate(`(async () => { const m = await import(document.querySelector('script[type="module']").src); const s = m.debugState().scanner; return s ? await s.zoomTo(${ZOOM}) : "no scanner"; })()`));
console.log("focus:", await evaluate(`(async () => { const m = await import(document.querySelector('script[type="module']").src); const s = m.debugState().scanner; return s ? JSON.stringify(await s.focusInfo()) : "{}"; })()`));

for (let i = 0; i < (MAX_MIN * 30); i++) {
  await new Promise((r) => setTimeout(r, 2000));
  const v = await evaluate(`(async () => {
    const __mod = await import(document.querySelector('script[type="module"]').src);
  if (!__mod.debugState) return JSON.stringify({ error: "page has no debugState()" });
  const __st = __mod.debugState();
  const f = __st.frame;
  const d = null; // decode lives in the worker now
    if (!f || !d) return JSON.stringify({ ready: false });
    const raw = d.decode(f.width, f.height, f.data);
    return JSON.stringify({ raw });
  })()`);
  let parsed;
  try { parsed = JSON.parse(v ?? "{}"); } catch { parsed = {}; }
  if (parsed.raw && parsed.raw.length) {
    console.log("DECODED:", JSON.stringify(parsed.raw));
    const shot = await send("Page.captureScreenshot", { format: "jpeg", quality: 70 });
    const { writeFileSync } = await import("node:fs");
    writeFileSync("/tmp/barcode-found.jpg", Buffer.from(shot.data, "base64"));
    process.exit(0);
  }
  if (i % 15 === 0) {
    const fi = await evaluate(`(async () => { const m = await import(document.querySelector('script[type="module']").src); const s = m.debugState().scanner; return s ? JSON.stringify(await s.focusInfo()) : "{}"; })()`);
    console.log("waiting... focus:", fi);
  }
}
console.log("no decode in", MAX_MIN, "min");
process.exit(1);
