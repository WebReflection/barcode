/**
 * Remote-drive the demo running in Chrome on a USB-attached Android phone.
 *
 * Prereqs:
 *   adb reverse tcp:5173 tcp:5173
 *   adb forward tcp:9222 localabstract:chrome_devtools_remote
 *
 * Usage: node tools/drive.mjs [screenshot-out-path]
 * Every poll: (re)connect, grant camera, self-heal the start button,
 * log stats + console, save a screenshot. Survives Chrome restarts.
 * Kill with Ctrl-C.
 */

import { writeFileSync } from "node:fs";

const HOST = "http://localhost:9222";
const ORIGIN = "http://localhost:5173";
const OUT = process.argv[2] ?? "/tmp/barcode-phone.jpg";
const INTERVAL_MS = 5000;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function rpc(ws) {
  let id = 0;
  const pending = new Map();
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const [resolve, reject] = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    }
  });
  const failAll = () => {
    for (const [, [resolve, reject]] of pending) reject(new Error("connection closed"));
    pending.clear();
  };
  ws.addEventListener("close", failAll);
  ws.addEventListener("error", failAll);
  return (method, params = {}) =>
    new Promise((resolve, reject) => {
      if (ws.readyState !== 1) return reject(new Error("ws not open"));
      const mid = ++id;
      pending.set(mid, [resolve, reject]);
      ws.send(JSON.stringify({ id: mid, method, params }));
    });
}

const opened = (url) =>
  new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    ws.addEventListener("open", () => resolve(ws));
    ws.addEventListener("error", () => reject(new Error("ws connect failed")));
  });

async function poll(n) {
  const targets = await fetch(`${HOST}/json`).then((r) => r.json());
  const page = targets.find((t) => t.type === "page" && t.url.startsWith(ORIGIN));
  if (!page) throw new Error("demo tab not found");

  // Re-grant each poll: CDP permission grants don't survive Chrome restarts.
  try {
    const v = await fetch(`${HOST}/json/version`).then((r) => r.json());
    const browser = await opened(v.webSocketDebuggerUrl);
    await rpc(browser)("Browser.grantPermissions", {
      permissions: ["videoCapture"],
      origin: ORIGIN,
    });
    browser.close();
  } catch (e) {
    console.log("[drive] grant failed:", e.message);
  }

  const ws = await opened(page.webSocketDebuggerUrl);
  const send = rpc(ws);
  await send("Page.enable");
  await send("Runtime.enable");

  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.method === "Runtime.consoleAPICalled" && msg.params.type !== "debug") {
      const line = msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ");
      console.log(`[console.${msg.params.type}] ${line}`);
    } else if (msg.method === "Runtime.exceptionThrown") {
      console.log(`[exception] ${msg.params.exceptionDetails?.text}`);
    }
  });

  // Self-heal: retry start while the camera is unavailable or startup pending.
  const stats = await send("Runtime.evaluate", {
    expression: `(function () {
      const b = document.getElementById("start");
      const s = document.getElementById("stats")?.textContent ?? "";
      if (b && !b.disabled && (s.startsWith("ERROR") || s.includes("waiting for camera"))) b.click();
      return s;
    })()`,
    returnByValue: true,
  }).then((r) => r.result?.value ?? "");

  const shot = await send("Page.captureScreenshot", { format: "jpeg", quality: 70 });
  writeFileSync(OUT, Buffer.from(shot.data, "base64"));
  ws.close();
  return { stats };
}

for (let i = 0; ; i++) {
  try {
    const { stats } = await poll(i);
    console.log(`[stats] ${stats.replace(/\n/g, " | ")}`);
  } catch (e) {
    console.log(`[drive] poll #${i} failed: ${e.message}`);
  }
  await sleep(INTERVAL_MS);
}
