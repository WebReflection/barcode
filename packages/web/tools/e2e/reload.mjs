#!/usr/bin/env node
/**
 * Hard-reload a phone/devtools page, bypassing cache. STATUS.md quirk: the
 * phone once served a week-old cached scan.html; this clears it.
 * Usage: node tools/e2e/reload.mjs <url-substr> [port]
 */
const [urlSubstr, port = "9222"] = process.argv.slice(2);
const targets = await (await fetch(`http://localhost:${port}/json`)).json();
const target = targets.find((t) => t.type === "page" && t.url.includes(urlSubstr));
if (!target) { console.error("no page matching", urlSubstr); process.exit(1); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
const send = (m, p) => new Promise((res, rej) => {
  const mid = ++id;
  pending.set(mid, { res, rej });
  ws.send(JSON.stringify({ id: mid, method: m, params: p }));
});
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    const { res, rej } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? rej(new Error(JSON.stringify(msg.error))) : res(msg.result);
  }
};
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
await send("Page.enable");
await send("Page.reload", { ignoreCache: true });
console.log("hard-reloaded", target.url);
process.exit(0);
