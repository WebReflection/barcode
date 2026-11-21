#!/usr/bin/env node
/**
 * Published-payload budget gate. The SDK ships four files; this script
 * makes their size a CI contract instead of a note.
 *
 * Run after `npm run build` (it reads dist/):
 *
 *   npm run test:budgets
 *
 * The budgets have ~5% headroom over the 0.2.6 payload. Raising one is a
 * deliberate size decision, not a test fix: update this file in the same
 * commit and say why.
 */
import { existsSync, readFileSync } from "node:fs";
import { brotliCompressSync, gzipSync } from "node:zlib";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = resolve(root, "dist");
const read = (name) => readFileSync(resolve(dist, name));

if (!existsSync(resolve(dist, "worker.js")) || !existsSync(resolve(dist, "wasm.wasm"))) {
  console.error("dist/ is missing — run `npm run build` first");
  process.exit(2);
}

const main = read("main.js");
const worker = read("worker.js");
const wasm = read("wasm.wasm");
const js = Buffer.concat([main, worker]);

const checks = [
  { name: "wasm.wasm raw", bytes: wasm.length, budget: 1_850_000 },
  { name: "wasm.wasm brotli", bytes: brotliCompressSync(wasm).length, budget: 370_000 },
  { name: "main+worker raw", bytes: js.length, budget: 68_000 },
  { name: "main+worker gzip", bytes: gzipSync(js, { level: 9 }).length, budget: 25_000 },
];

let failed = 0;
console.log("# published payload budgets");
for (const { name, bytes, budget } of checks) {
  const ok = bytes <= budget;
  if (!ok) failed++;
  const pct = ((bytes / budget) * 100).toFixed(1);
  console.log(
    `${ok ? "PASS" : "FAIL"}  ${name.padEnd(20)} ${String(bytes).padStart(8)} B / ${String(budget).padStart(8)} B  (${pct}%)`,
  );
}
console.log(`== ${checks.length - failed} PASS, ${failed} FAIL (${checks.length} budgets)`);
process.exit(failed ? 1 : 0);
