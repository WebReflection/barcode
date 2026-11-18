/**
 * One-shot: re-zoom, verify the mask is alive, run both decoders.
 * Usage: node tools/session.mjs
 */

const ORIGIN = "http://localhost:5173";

const TABLES_JSON = JSON.stringify({
  A: { "*": "146", "0": "346", "1": "038", "2": "238", "3": "023", "4": "348", "5": "034", "6": "234", "7": "368", "8": "036", "9": "236", A: "078", B: "278", C: "027", D: "478", E: "047", F: "247", G: "678", H: "067", I: "267", J: "467", K: "058", L: "258", M: "025", N: "458", O: "045", P: "245", Q: "568", R: "056", S: "256", T: "456", U: "018", V: "128", W: "012", X: "148", Y: "014", Z: "124", "-": "168", ".": "016", " ": "126", $: "135", "/": "137", "+": "157", "%": "357" },
  B: { "*": "168", "0": "346", "1": "038", "2": "238", "3": "023", "4": "348", "5": "034", "6": "234", "7": "368", "8": "036", "9": "236", A: "078", B: "278", C: "027", D: "478", E: "047", F: "247", G: "678", H: "067", I: "267", J: "467", K: "058", L: "258", M: "025", N: "458", O: "045", P: "245", Q: "568", R: "056", S: "256", T: "456", U: "018", V: "128", W: "012", X: "148", Y: "014", Z: "124", "-": "168", ".": "016", " ": "126", $: "135", "/": "137", "+": "157", "%": "357" },
});

const PROBE_BODY = `
const __mod = await import(document.querySelector('script[type="module"]').src);
  if (!__mod.debugState) return JSON.stringify({ error: "page has no debugState()" });
  const __st = __mod.debugState();
  const f = __st.frame;
  if (!f) return JSON.stringify({ error: "no frame" });
  await new Promise((r) => setTimeout(r, 400));
  const tables = ${TABLES_JSON};
  const rev = {};
  for (const [tn, t] of Object.entries(tables)) {
    rev[tn] = {};
    for (const [ch, idx] of Object.entries(t)) rev[tn][idx] = ch;
  }
  function runsOf(y) {
    const out = [];
    let cur = 0, n = 0;
    for (let x = 0; x < f.width; x++) {
      const ink = (f.data[y * f.bytesPerRow + (x >> 3)] >> (x & 7)) & 1;
      if (ink === cur) n++;
      else { out.push([cur, n]); cur = ink; n = 1; }
    }
    if (n) out.push([cur, n]);
    return out;
  }
  function matchChar(elems, i, revT) {
    if (i + 9 > elems.length) return null;
    const lens = [];
    for (let k = 0; k < 9; k++) lens.push(elems[i + k][1]);
    const bars = [0, 2, 4, 6, 8].map((k) => lens[k]);
    const spaces = [1, 3, 5, 7].map((k) => lens[k]);
    const barLo = Math.min(...bars), barHi = Math.max(...bars);
    const spLo = Math.min(...spaces), spHi = Math.max(...spaces);
    if (spHi < spLo * 1.9 || spHi > spLo * 4.5) return null;
    const barMid = (barLo + barHi) / 2, spMid = (spLo + spHi) / 2;
    const idx = [];
    for (let k = 0; k < 9; k++) {
      const wide = k % 2 === 0 ? lens[k] > barMid : lens[k] > spMid;
      if (wide) idx.push(k);
    }
    const ch = revT[idx.join("")];
    return ch ? { ch, spLo } : null;
  }
  function tryFrom(elems, i, revT) {
    let j = i, text = "";
    let first = true;
    while (j + 9 <= elems.length) {
      const m = matchChar(elems, j, revT);
      if (!m) return null;
      if (first) { if (m.ch !== "*") return null; first = false; }
      else if (m.ch === "*") return text.length >= 4 ? text : null;
      text += m.ch;
      j += 9;
      if (j < elems.length && !elems[j][0] && elems[j][1] <= m.spLo * 1.6) j++;
    }
    return null;
  }
  function decode39(y, revT) {
    const elems = runsOf(y);
    for (const flip of [false, true]) {
      const e = flip ? [...elems].reverse() : elems;
      for (let i = 0; i + 9 <= e.length; i++) {
        if (!e[i][0]) continue;
        const t = tryFrom(e, i, revT);
        if (t) return flip ? [...t].reverse().join("") : t;
      }
    }
    return null;
  }
  function decodeEAN13(elems) {
    const L = ["0001101","0011001","0010011","0111101","0100011","0110001","0101111","0111011","0110111","0001011"];
    const G = ["0100111","0110011","0011011","0100001","0011101","0111001","0000101","0010001","0001001","0010111"];
    const R = ["1110010","1100110","1101100","1000010","1011100","1001110","1010000","1000100","1001000","1110100"];
    const PAR = ["LLLLLL","LLGLGG","LLGGLG","LLGGGL","LGLLGG","LGGLLG","LGGGLL","LGLGLG","LGLGGL","LGGLGL"];
    const n = elems.length;
    for (let i = 0; i + 59 <= n; i++) {
      if (!elems[i][0]) continue;
      let w = 0;
      for (let k = 0; k < 59; k++) w += elems[i + k][1];
      const N = w / 95;
      if (N < 0.8) continue;
      let ok = true;
      const mods = [];
      for (let k = 0; k < 59; k++) {
        const m = elems[i + k][1] / N;
        const r = Math.round(m);
        if (r < 1 || r > 4 || Math.abs(m - r) > 0.45) { ok = false; break; }
        mods.push(r);
      }
      if (!ok) continue;
      if (mods[0] !== 1 || mods[1] !== 1 || mods[2] !== 1) continue;
      if (mods[27] !== 1 || mods[28] !== 1 || mods[29] !== 1 || mods[30] !== 1 || mods[31] !== 1) continue;
      if (mods[56] !== 1 || mods[57] !== 1 || mods[58] !== 1) continue;
      let bits = "";
      for (let k = 0; k < 59; k++) bits += (elems[i + k][0] ? "1" : "0").repeat(mods[k]);
      if (bits.length !== 95) continue;
      let parity = "";
      const left = [];
      for (let d = 0; d < 6; d++) {
        const seg = bits.slice(3 + d * 7, 3 + d * 7 + 7);
        const li = L.indexOf(seg), gi = G.indexOf(seg);
        if (li >= 0) { left.push(li); parity += "L"; }
        else if (gi >= 0) { left.push(gi); parity += "G"; }
        else { left.length = 0; break; }
      }
      if (!left.length) continue;
      const first = PAR.indexOf(parity);
      if (first < 0) continue;
      const right = [];
      for (let d = 0; d < 6; d++) {
        const seg = bits.slice(50 + d * 7, 50 + d * 7 + 7);
        const ri = R.indexOf(seg);
        if (ri < 0) { right.length = 0; break; }
        right.push(ri);
      }
      if (!right.length) continue;
      return [first, ...left, ...right].join("");
    }
    return null;
  }
  const counts = { A: {}, B: {}, ean13: {} };
  let ink = 0, tot = 0;
  for (let i = 0; i < f.data.length; i++) {
    const b = f.data[i];
    for (let k = 0; k < 8; k++) { ink += (b >> k) & 1; tot++; }
  }
  for (let y = 0; y < f.height; y++) {
    const elems = runsOf(y);
    const e = decodeEAN13(elems);
    if (e) counts.ean13[e] = (counts.ean13[e] ?? 0) + 1;
    for (const tn of ["A", "B"]) {
      const t = decode39(y, rev[tn]);
      if (t) counts[tn][t] = (counts[tn][t] ?? 0) + 1;
    }
  }
  return JSON.stringify({ inkPct: (100 * ink / tot).toFixed(1), ...counts });
`;

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
  const ev = (expr) => send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true }).then((r) => r.result?.value);

  console.log("zoom:", await ev(`(async () => {
    const __mod = await import(document.querySelector('script[type="module"]').src);
    const s = __mod.debugState().scanner;
    return s ? await s.zoomTo(2.5) : "no scanner";
  })()`));
  await new Promise((r) => setTimeout(r, 1500));
  console.log(await ev(`(function(){ const f = window.__frame; if (!f) return "no frame";
    return "probe running on: " + new Date().toISOString(); })()`));
  console.log(await ev(`(async () => { ${PROBE_BODY} })()`));
  process.exit(0);
})().catch((e) => { console.error("ERR:", e.message); process.exit(1); });
