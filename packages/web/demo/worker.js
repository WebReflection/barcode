/**
 * Worker API demo: the page entry, the explicit /worker entry spawned by
 * the page, and /worker imported statically at the top of a consumer
 * worker (the contract for a guaranteed MessagePort channel).
 */
import barcode from "../api/main.js";
import workerUrl from "./worker-url.js";

const logEl = /** @type {HTMLElement} */ (document.getElementById("log"));
/** @type {string[]} */
const lines = [];
/** @param {string} text @param {"" | "pass" | "fail" | "dim"} [kind] */
function log(text, kind = "dim") {
  lines.push(`${kind === "pass" ? "PASS" : kind === "fail" ? "FAIL" : "·"} ${text}`);
  logEl.textContent = lines.join("\n");
}
function clear() {
  lines.length = 0;
  logEl.textContent = "";
}

const $$ = (/** @type {string} */ id) =>
  /** @type {HTMLButtonElement} */ (document.getElementById(id));

/**
 * @param {SVGSVGElement} svg
 */
async function rasterize(svg) {
  const source =
    "data:image/svg+xml;charset=utf-8," +
    encodeURIComponent(new XMLSerializer().serializeToString(svg));
  const img = new Image();
  img.src = source;
  await img.decode();
  const scale = Math.min(3, 900 / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(img.naturalWidth * scale)) + 80;
  canvas.height = Math.max(1, Math.round(img.naturalHeight * scale)) + 80;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas 2d unavailable");
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(img, 40, 40, canvas.width - 80, canvas.height - 80);
  return canvas;
}

$$("auto-main").addEventListener("click", async () => {
  clear();
  try {
    const { default: entry } = await import("../api/main.js");
    if (typeof entry === "function") log("main-thread default is barcode()", "pass");
    else log(`main-thread default is ${typeof entry}`, "fail");
  } catch (err) {
    log(String(/** @type {Error} */ (err).message || err), "fail");
  }
});

$$("explicit").addEventListener("click", async () => {
  clear();
  const button = $$("explicit");
  button.disabled = true;
  try {
    // The explicit worker entry; no default worker is ever spawned.
    const worker = new Worker(workerUrl, { type: "module" });
    worker.addEventListener("error", (event) => log(`worker error: ${event.message}`, "fail"));
    const session = barcode({ worker });
    log(`barcode({ worker }) returned the caller's Worker: ${session.worker === worker}`, session.worker === worker ? "pass" : "fail");
    const t0 = performance.now();
    const svg = await session.render({ symbology: "qrcode", text: "EXPLICIT-WORKER" });
    if (!svg) throw new Error("render returned null");
    const code = await session.scan({ image: await rasterize(svg) });
    log(`render + scan → ${JSON.stringify(code?.text ?? null)} in ${Math.round(performance.now() - t0)}ms`, code?.text === "EXPLICIT-WORKER" ? "pass" : "fail");
  } catch (err) {
    log(String(/** @type {Error} */ (err).message || err), "fail");
  } finally {
    button.disabled = false;
  }
});

$$("auto-worker").addEventListener("click", async () => {
  clear();
  const button = $$("auto-worker");
  button.disabled = true;
  try {
    const worker = new Worker(new URL("./worker-entry.js", import.meta.url), {
      type: "module",
    });
    /** @type {any[]} */
    const messages = [];
    worker.addEventListener("message", (event) => {
      messages.push(event.data);
      if (event.data?.type === "entry") {
        log(
          `worker default: ${event.data.kind}${event.data.promise ? " (promise)" : ""}, ` +
          `decode=${event.data.decode}, encode=${event.data.encode}`,
          event.data.promise && event.data.decode && event.data.encode ? "pass" : "fail",
        );
      }
      if (event.data?.error) log(`worker error: ${event.data.error}`, "fail");
    });
    const session = barcode({ worker });
    const t0 = performance.now();
    const svg = await session.render({ symbology: "qrcode", text: "AUTO-WORKER" });
    if (!svg) throw new Error("render returned null");
    const code = await session.scan({ image: await rasterize(svg) });
    log(`render + scan → ${JSON.stringify(code?.text ?? null)} in ${Math.round(performance.now() - t0)}ms`, code?.text === "AUTO-WORKER" ? "pass" : "fail");
    await new Promise((resolve) => setTimeout(resolve, 100));
    if (!messages.some((m) => m?.type === "entry" && m.promise)) {
      log("worker never reported a promise default", "fail");
    }
  } catch (err) {
    log(String(/** @type {Error} */ (err).message || err), "fail");
  } finally {
    button.disabled = false;
  }
});
