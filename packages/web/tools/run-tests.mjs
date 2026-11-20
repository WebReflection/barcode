#!/usr/bin/env node
import { fileURLToPath } from "node:url";
/**
 * Drive the self-test harness page over CDP.
 * Usage: node tools/run-tests.mjs [cdpPort] [url]
 */
const port = process.argv[2] ?? "9333";
const tests = fileURLToPath(new URL("../tests/", import.meta.url));
const url = process.argv[3] ?? `http://localhost:5173/@fs${tests}test.html`;

const targets = await fetch(`http://localhost:${port}/json`).then((r) => r.json());
let page = targets.find((t) => t.type === "page");
if (!page) {
  // create one via the browser endpoint
  const browser = targets; // fallback: PUT /json/new
  const created = await fetch(`http://localhost:${port}/json/new?about:blank`, { method: "PUT" }).then((r) => r.json());
  page = created;
}
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.addEventListener("open", res); ws.addEventListener("error", rej); });
let id = 0;
const pending = new Map();
ws.addEventListener("message", (ev) => {
  const d = JSON.parse(ev.data);
  if (d.id && pending.has(d.id)) {
    const { resolve, reject } = pending.get(d.id);
    pending.delete(d.id);
    d.error ? reject(new Error(JSON.stringify(d.error))) : resolve(d.result);
  }
});
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const mid = ++id;
  pending.set(mid, { resolve, reject });
  ws.send(JSON.stringify({ id: mid, method, params }));
});

await send("Page.enable");
await send("Runtime.enable");
await send("Page.navigate", { url });
// wait for the run to finish (the page sets the title; max 60s)
let title = "";
for (let i = 0; i < 120; i++) {
  await new Promise((r) => setTimeout(r, 500));
  const t = await send("Runtime.evaluate", { expression: "document.title", returnByValue: true });
  title = t.result?.value ?? "";
  if (title === "TESTS PASS" || title === "TESTS FAILED") break;
}
if (title !== "TESTS PASS" && title !== "TESTS FAILED") {
  console.error("TIMEOUT: run did not finish");
  process.exit(1);
}
// results are the rendered table: sym | text | render | result | got | mask | ms
const rows = await send("Runtime.evaluate", { expression: `
  [...document.querySelectorAll('tbody tr')].map((tr) =>
    [...tr.children].map((td) => td.textContent))`, returnByValue: true });
let fails = 0;
for (const c of rows.result?.value ?? []) {
  const [sym, text, render, result, got, mask, ms] = c;
  const pass = result === "PASS";
  if (!pass) fails++;
  console.log(`${pass ? "PASS" : "FAIL"}  ${sym.padEnd(8)} ${JSON.stringify(text).padEnd(26)} ${render.padEnd(12)} got=${JSON.stringify(got)} ${mask} ${ms}`);
}
const summary = await send("Runtime.evaluate", { expression: "document.getElementById('summary').textContent", returnByValue: true });
console.log("==", summary.result.value);
process.exit(fails ? 1 : 0);
