/**
 * Symbology identification from a captured run sequence: tries EAN-13,
 * Code 128 and Code 39 structural fits across all windows and scales.
 * Usage: node tools/identify.mjs <runs.json>
 */

import { readFileSync } from "node:fs";
const runs = JSON.parse(readFileSync(process.argv[2] ?? "/tmp/row424.json", "utf8"));
console.log("runs:", runs.length, runs.map(r => (r[0] ? "B" : "S") + r[1]).join(" "));

const segOf = (s) => { let v = 0; for (let i = 0; i < s.length; i++) if (s[i] === "1") v |= 1 << i; return v; };

// --- EAN-13 ---
const L = ["0001101","0011001","0010011","0111101","0100011","0110001","0101111","0111011","0110111","0001011"];
const R = ["1110010","1100110","1101100","1000010","1011100","1001110","1010000","1000100","1001000","1110100"];
function tryEAN13(rr, s, N) {
  if (s + 59 > rr.length) return null;
  const mods = [];
  for (let k = 0; k < 59; k++) {
    const m = rr[s + k][1] / N;
    const r = Math.round(m);
    if (r < 1 || r > 4 || Math.abs(m - r) > 0.45) return null;
    mods.push(r);
  }
  const g = [0,1,2,27,28,29,30,31,56,57,58].every(i => mods[i] === 1);
  return { guardOk: g, mods: mods.join("") };
}

// --- Code 128: characters are 6 runs (3 bars, 3 spaces) totalling 11 modules;
// start A/B/C and stop have known leading patterns. We fit "chains of 11".
function tryCode128(rr, s, N) {
  // quantize a sliding chain of 6-run chars; report the first 8 char widths
  const chars = [];
  let k = s;
  while (k + 6 <= rr.length && chars.length < 10) {
    const widths = [];
    let total = 0;
    for (let i = 0; i < 6; i++) {
      const m = rr[k + i][1] / N;
      const r = Math.round(m);
      if (r < 1 || r > 4) return chars; // stop at first invalid
      widths.push(r); total += r;
    }
    chars.push(widths.join(",") + (total === 11 ? "" : ` (total ${total}!)`));
    k += 6;
  }
  return chars;
}

// --- Code 39: 9 elements + narrow gap per char; '*' = [1,2,1,1,2,1,2,1,1]
function tryCode39(rr, s, N) {
  const star = [1, 2, 1, 1, 2, 1, 2, 1, 1];
  let ok = true;
  for (let k = 0; k < 9; k++) {
    const m = rr[s + k][1] / N;
    if (Math.abs(m - star[k]) > 0.45) { ok = false; break; }
  }
  return ok;
}

for (let s = 0; s < runs.length - 10; s++) {
  if (!runs[s][0]) continue;
  for (let N = 2.0; N <= 7.0; N += 0.05) {
    // EAN-13
    const e = tryEAN13(runs, s, N);
    if (e && e.guardOk) console.log(`EAN-13 GUARD OK at s=${s} N=${N.toFixed(2)}`);
    // Code 39 star
    if (tryCode39(runs, s, N)) console.log(`Code39 '*' at s=${s} N=${N.toFixed(2)}`);
  }
  // Code 128 chain fit (N from the first char's total = 11)
  for (let N = 2.0; N <= 7.0; N += 0.05) {
    // first char: 6 runs summing to 11 modules
    let total = 0, ok = true;
    for (let i = 0; i < 6; i++) {
      const m = runs[s + i][1] / N;
      const r = Math.round(m);
      if (r < 1 || r > 4 || Math.abs(m - r) > 0.3) { ok = false; break; }
      total += r;
    }
    if (!ok || total !== 11) continue;
    const chars = tryCode128(runs, s, N);
    if (chars && chars.length >= 4) {
      console.log(`Code128 chain at s=${s} N=${N.toFixed(2)}: ${chars.join(" | ")}`);
    }
  }
}
