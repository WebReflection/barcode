#!/usr/bin/env node
import { fileURLToPath } from "node:url";
/**
 * Drive tests/contract-test.html (worker transports) over CDP.
 * Usage: node tools/run-contract.mjs [cdpPort] [url]
 */
const port = process.argv[2] ?? "9225";
const tests = fileURLToPath(new URL("../tests/", import.meta.url));
const url = process.argv[3] ?? `http://localhost:5173/@fs${tests}contract-test.html`;

const targets = await fetch(`http://localhost:${port}/json`).then((r) => r.json());
let page = targets.find((t) => t.type === "page");
if (!page) {
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
let title = "";
for (let i = 0; i < 120; i++) {
  await new Promise((r) => setTimeout(r, 500));
  const t = await send("Runtime.evaluate", { expression: "document.title", returnByValue: true });
  title = t.result?.value ?? "";
  if (title === "TESTS PASS" || title === "TESTS FAILED") break;
}
if (title !== "TESTS PASS" && title !== "TESTS FAILED") {
  console.error("TIMEOUT: contract run did not finish");
  process.exit(1);
}
const checks = await send("Runtime.evaluate", { expression: `
  [...document.querySelectorAll('table tr')].map((tr) =>
    [...tr.children].map((td) => td.textContent))`, returnByValue: true });
let fails = 0;
for (const [name, pass, note] of checks.result?.value ?? []) {
  if (pass !== "PASS") fails++;
  console.log(`${pass === "PASS" ? "PASS" : "FAIL"}  ${String(name).padEnd(38)} ${note ?? ""}`);
}
process.exit(fails ? 1 : 0);
