/**
 * Deep diagnostic: per-row EAN-13 window analysis of the live GPU mask.
 * Reports how far each stage gets (rows -> windows -> quantized -> guards)
 * and dumps the best window's module bits for local inspection.
 *
 * Usage: node tools/diag.mjs
 */

const ORIGIN = "http://localhost:5173";

const PROBE = `(async () => {
  const __mod = await import(document.querySelector('script[type="module"]').src);
  if (!__mod.debugState) return JSON.stringify({ error: "page has no debugState()" });
  const __st = __mod.debugState();
  const f = __st.frame;
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
  let best = null;
  const stats = { rows: 0, windows: 0, quantOk: 0, guardOk: 0 };
  for (let y = 0; y < f.height; y++) {
    const rr = runsOf(y);
    if (rr.length < 59) continue;
    stats.rows++;
    for (let s = 0; s + 59 <= rr.length; s++) {
      if (!rr[s][0]) continue;
      stats.windows++;
      let w = 0;
      for (let k = 0; k < 59; k++) w += rr[s + k][1];
      const N = w / 95;
      if (N < 0.8) continue;
      let err = 0, ok = true;
      for (let k = 0; k < 59; k++) {
        const m = rr[s + k][1] / N;
        if (Math.abs(m - Math.round(m)) > 0.45) { ok = false; break; }
        err += Math.abs(m - Math.round(m));
      }
      if (!ok) continue;
      stats.quantOk++;
      const mods = [];
      for (let k = 0; k < 59; k++) mods.push([rr[s + k][0], Math.round(rr[s + k][1] / N)]);
      const g = mods[0][1] === 1 && mods[1][1] === 1 && mods[2][1] === 1 &&
        mods[27][1] === 1 && mods[28][1] === 1 && mods[29][1] === 1 && mods[30][1] === 1 &&
        mods[31][1] === 1 && mods[56][1] === 1 && mods[57][1] === 1 && mods[58][1] === 1;
      if (g) stats.guardOk++;
      const avg = err / 59;
      if (!best || avg < best.avg) best = { y, avg, N, mods };
    }
  }
  if (!best) return JSON.stringify({ stats });
  let bits = "";
  for (const [ink, m] of best.mods) bits += (ink ? "1" : "0").repeat(m);
  return JSON.stringify({ stats, best: { y: best.y, avg: best.avg.toFixed(3), N: best.N.toFixed(2), bits } });
})()`;

(async () => {
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
  const r = await send("Runtime.evaluate", { expression: PROBE, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) console.log("EXC:", JSON.stringify(r.exceptionDetails).slice(0, 400));
  else console.log(r.result?.value);
  process.exit(0);
})().catch((e) => { console.error("ERR:", e.message); process.exit(1); });
