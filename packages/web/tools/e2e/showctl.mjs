#!/usr/bin/env node
/**
 * Control the Mac show window: `maximize` (default) or `restore`.
 * Usage: node tools/e2e/showctl.mjs [maximize|fullscreen|normal] [port]
 */
const [state = "maximize", port = "9223"] = process.argv.slice(2);
const targets = await (await fetch(`http://localhost:${port}/json`)).json();
const target = targets.find((t) => t.type === "page" && t.url.includes("/tests/show.html"));
if (!target) { console.error("no show.html page on :" + port); process.exit(1); }
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
const { windowId } = await send("Browser.getWindowForTarget");
await send("Browser.setWindowBounds", { windowId, bounds: { windowState: state } });
console.log("window", windowId, "->", state);
process.exit(0);
