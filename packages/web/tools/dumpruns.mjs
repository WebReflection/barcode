/**
 * Dump run sequences from rows with the most transitions (the barcode band),
 * for local structural analysis.
 * Usage: node tools/dumpruns.mjs [count]
 */

const ORIGIN = "http://localhost:5173";
const PORT = process.env.CDP_PORT ?? "9222";

const PROBE = `(async () => {
  ${GET_STATE}
  if (!f) return JSON.stringify({ error: "no frame" });
  function runsOf(y) {
    const out = [];
    let cur = false, n = 0;
    for (let x = 0; x < f.width; x++) {
      const ink = (f.data[y * f.bytesPerRow + (x >> 3)] >> (x & 7)) & 1;
      if (ink === cur) n++;
      else { out.push([cur, n]); cur = ink; n = 1; }
    }
    if (n) out.push([cur, n]);
    return out;
  }
  const cands = [];
  for (let y = 0; y < f.height; y++) {
    const rr = runsOf(y);
    cands.push([y, rr.length, rr]);
  }
  cands.sort((a, b) => b[1] - a[1]);
  const n = ${process.argv[2] ?? 3};
  return JSON.stringify({ w: f.width, h: f.height, top: cands.slice(0, n).map(([y, len, rr]) => ({ y, len, runs: rr })) });
})()`;

(async () => {
  const targets = await fetch(`http://localhost:${PORT}/json`).then((r) => r.json());
  const page = targets.find((t) => t.type === "page" && t.url.startsWith(ORIGIN));
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
  const r = await send("Runtime.evaluate", { expression: PROBE, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) console.log("EXC:", JSON.stringify(r.exceptionDetails).slice(0, 300));
  else console.log(r.result?.value);
  process.exit(0);
})().catch((e) => { console.error("ERR:", e.message); process.exit(1); });
