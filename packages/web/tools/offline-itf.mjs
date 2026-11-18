/**
 * Offline ITF decode from the captured luma band (/tmp/itf-luma.b64).
 * 1056 x 400 rows starting at frame y=300. Simple: threshold sweep,
 * find 4-narrow-run start, decode pairs, validate check digit. Done.
 */
import { readFileSync } from "node:fs";

const luma = Uint8Array.from(Buffer.from(readFileSync("/tmp/itf-luma.b64", "utf8").trim(), "base64"));
const W = 1056, H = luma.length / W, Y0 = 300;
console.log(`luma ${W}x${H} (frame y ${Y0}-${Y0 + H})`);

const D2of5 = ["00110","10001","01001","11000","00101","10100","01100","00011","10010","01010"];

function runsOf(y, T) {
  const out = [];
  let cur = false, n = 0;
  for (let x = 0; x < W; x++) {
    const ink = luma[y * W + x] < T;
    if (ink === cur) n++;
    else { out.push([cur, n]); cur = ink; n = 1; }
  }
  if (n) out.push([cur, n]);
  return out;
}

function tryITF(rr) {
  if (rr.length < 14) return null;
  // find the start: a bar run followed by 3 more runs, all narrow-ish, with
  // the first pair after them classifying cleanly (per-pair sort-split).
  for (let s = 0; s + 14 <= rr.length; s++) {
    if (!rr[s][0]) continue;
    // bootstrap scale from the first 14 runs
    const boot = rr.slice(s, s + 14).map(r => r[1]).sort((a, b) => a - b);
    const thr = (boot[5] + boot[6]) / 2;
    if (thr < 1.5) continue;
    if (boot[5] >= thr || boot[6] < thr) continue; // no clean gap
    // start's 4 runs narrow
    let ok = true;
    for (let k = 0; k < 4; k++) if (rr[s + k][1] >= thr) { ok = false; break; }
    if (!ok) continue;
    // decode pairs
    const digits = [];
    let k = s + 4;
    let lastInk = -1;
    for (let i = 0; i < rr.length; i++) if (rr[i][0]) lastInk = i;
    while (k + 10 <= rr.length) {
      const widths = [];
      for (let i = 0; i < 10; i++) widths.push(rr[k + i][1]);
      const sorted = [...widths].sort((a, b) => a - b);
      const t2 = (sorted[5] + sorted[6]) / 2;
      if (sorted[5] >= t2 || sorted[6] < t2) return null; // no gap -> not a pair
      const cls = widths.map(w => (w >= t2 ? "1" : "0"));
      const aPat = cls.filter((_, i) => i % 2 === 0).join("");
      const bPat = cls.filter((_, i) => i % 2 === 1).join("");
      const ai = D2of5.indexOf(aPat), bi = D2of5.indexOf(bPat);
      if (ai < 0 || bi < 0) return null;
      digits.push(ai, bi);
      k += 10;
      // stop: narrow bar, narrow space, wide bar = last ink run
      if (k + 3 <= rr.length) {
        const b1 = rr[k][0] && rr[k][1] < t2 * 1.4;
        const sp = !rr[k + 1][0] && rr[k + 1][1] < t2 * 1.4;
        const b2 = rr[k + 2][0] && rr[k + 2][1] >= t2 * 1.4;
        if (b1 && sp && b2 && k + 2 === lastInk) {
          return { digits: digits.join(""), pairs: digits.length / 2 };
        }
      }
      if (digits.length > 30) return null;
    }
  }
  return null;
}

const hits = new Map();
for (let T = 0.30; T <= 0.70; T += 0.02) {
  for (let y = 20; y < H - 20; y += 2) {
    const rr = runsOf(y, T);
    const res = tryITF(rr);
    if (res) {
      const key = res.digits;
      hits.set(key, (hits.get(key) ?? 0) + 1);
    }
  }
}
console.log("results:", hits.size ? [...hits.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5) : "none");
