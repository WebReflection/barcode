#!/usr/bin/env node
/**
 * Browser-level camera permission grant for the phone scan page.
 * STATUS.md: page-level CDP grant rejects video_capture; the BROWSER
 * endpoint accepts it. Usage: node tools/e2e/cam_grant.mjs [origin]
 * Requires: adb forward tcp:9222 localabstract:chrome_devtools_remote
 */
const origin = process.argv[2] ?? "http://localhost:5173";

const ver = await (await fetch("http://localhost:9222/json/version")).json();
const ws = new WebSocket(ver.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
const send = (method, params) => new Promise((resolve, reject) => {
  const msgId = ++id;
  pending.set(msgId, { resolve, reject });
  ws.send(JSON.stringify({ id: msgId, method, params }));
});
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
  }
};
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });

await send("Browser.grantPermissions", {
  origin,
  permissions: ["videoCapture", "audioCapture"],
});
console.log("granted", origin);
process.exit(0);
