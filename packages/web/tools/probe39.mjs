/**
 * Ground-truth probe: decode Code 39 directly on the phone's GPU mask
 * (window.__frame) over all rows, to validate the ISO table + decoder
 * algorithm before porting to Rust. Tests both candidate '*' patterns.
 *
 * Usage: node tools/probe39.mjs
 */

const ORIGIN = "http://localhost:5173";

/** char -> wide element indices (0=first bar, alternating bar/space) */
const STAR_A = "146"; // ISO/IEC 16388 extraction: S1, B3, B4
const STAR_B = "168"; // USS-39 OCR: S1, B4, B5

/** @param {string} star */
function buildTable(star) {
  return {
    "0": "346", "1": "038", "2": "238", "3": "023", "4": "348",
    "5": "034", "6": "234", "7": "368", "8": "036", "9": "236",
    A: "078", B: "278", C: "027", D: "478", E: "047", F: "247",
    G: "678", H: "067", I: "267", J: "467", K: "058", L: "258",
    M: "025", N: "458", O: "045", P: "245", Q: "568", R: "056",
    S: "256", T: "456", U: "018", V: "128", W: "012", X: "148",
    Y: "014", Z: "124", "-": "168", ".": "016", " ": "126",
    $: "135", "/": "137", "+": "157", "%": "357", "*": star,
  };
}

/** char -> wide element indices */
const TABLES_JSON = JSON.stringify({
  A: buildTable(STAR_A),
  B: buildTable(STAR_B),
});

const probeSrc = `(async () => {
  const __mod = await import(document.querySelector('script[type="module"]').src);
  if (!__mod.debugState) return JSON.stringify({ error: "page has no debugState()" });
  const __st = __mod.debugState();
  const f = __st.frame;
  if (!f) return JSON.stringify({ error: "no frame" });

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
    // Binarization dilates ink, so bars run ~1px wide vs spaces at the same
    // nominal width. Classify each type against its own min/max midline.
    const bars = [0, 2, 4, 6, 8].map((k) => lens[k]);
    const spaces = [1, 3, 5, 7].map((k) => lens[k]);
    const barLo = Math.min(...bars), barHi = Math.max(...bars);
    const spLo = Math.min(...spaces), spHi = Math.max(...spaces);
    // every char has >= 1 wide space -> internal ratio must be plausible
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
      // intercharacter gap: a narrow background run
      if (j < elems.length && !elems[j][0] && elems[j][1] <= m.spLo * 1.6) j++;
    }
    return null;
  }

  function decodeRow(y, tn) {
    const elems = runsOf(y);
    const revT = rev[tn];
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

  const y0 = 0, y1 = f.height;

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
      // guards: start 1,1,1 | center 1,1,1,1,1 (runs 27..31) | end 1,1,1
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
  for (let y = y0; y < y1; y++) {
    const elems = runsOf(y);
    const e = decodeEAN13(elems);
    if (e) counts.ean13[e] = (counts.ean13[e] ?? 0) + 1;
    for (const tn of ["A", "B"]) {
      const t = decodeRow(y, tn);
      if (t) counts[tn][t] = (counts[tn][t] ?? 0) + 1;
    }
  }
  if (Object.values(counts).every((c) => !Object.keys(c).length)) {
    // diagnostics: row with the most ink runs in plausible Code 39 sizes
    let best = null;
    for (let y = y0; y < y1; y++) {
      const r = runsOf(y);
      if (!best || r.length > best.runs.length) best = { y, runs: r };
    }
    return JSON.stringify({ diag: { y: best.y, n: best.runs.length, runs: best.runs } });
  }
  return JSON.stringify(counts);
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
    const to = setTimeout(() => rej(new Error("timeout: " + m)), 20000);
    const h = (ev) => {
      const d = JSON.parse(ev.data);
      if (d.id === mid) { clearTimeout(to); ws.removeEventListener("message", h); d.error ? rej(new Error(JSON.stringify(d.error))) : res(d.result); }
    };
    ws.addEventListener("message", h);
    ws.send(JSON.stringify({ id: mid, method: m, params: p }));
  });
  const r = await send("Runtime.evaluate", { expression: probeSrc, returnByValue: true, awaitPromise: true });
  console.log(r.result?.value);
  process.exit(0);
})().catch((e) => { console.error("ERR:", e.message); process.exit(1); });
