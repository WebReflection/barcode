/**
 * scan({ video, signal }) and scan({ into }) in one page.
 */
import barcode from "../api/main.js";
import workerUrl from "../api/worker.js?worker&url";

const { scan, render } = barcode({ worker: workerUrl });

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));
const video = /** @type {HTMLVideoElement} */ (document.getElementById("video"));
const startBtn = /** @type {HTMLButtonElement} */ (document.getElementById("start"));
const stopBtn = /** @type {HTMLButtonElement} */ (document.getElementById("stop"));
const statusEl = $("status");
const resultEl = $("result");
const intoBtn = /** @type {HTMLButtonElement} */ (document.getElementById("into"));
const intoStatus = $("into-status");
const intoResult = $("into-result");

/** @type {AbortController | null} */
let controller = null;

/** @param {HTMLElement} el @param {string} message @param {"" | "error" | "pass"} [kind] */
function status(el, message, kind = "") {
  el.textContent = message;
  el.className = `status ${kind}`;
}

/** @param {import("../main.d.ts").Code} code @param {HTMLElement} target */
async function show(code, target) {
  target.replaceChildren();
  const value = document.createElement("div");
  value.textContent = `${code.symbology}: ${code.text}`;
  target.append(value);
  try {
    const svg = await render(code);
    if (svg) target.append(svg);
  } catch { /* preview is optional */ }
}

startBtn.addEventListener("click", async () => {
  if (controller) return;
  controller = new AbortController();
  startBtn.disabled = true;
  stopBtn.disabled = false;
  resultEl.replaceChildren();
  status(statusEl, "Starting the camera…");
  try {
    const code = await scan({ video, signal: controller.signal });
    if (!code) {
      status(statusEl, "Scan stopped.", "");
    } else {
      status(statusEl, `Read ${code.symbology}.`, "pass");
      await show(code, resultEl);
    }
  } catch (err) {
    status(statusEl, /** @type {Error} */ (err).message || String(err), "error");
  } finally {
    controller = null;
    startBtn.disabled = false;
    stopBtn.disabled = true;
  }
});

stopBtn.addEventListener("click", () => {
  controller?.abort();
  status(statusEl, "Stopping…");
});

intoBtn.addEventListener("click", async () => {
  intoBtn.disabled = true;
  intoResult.replaceChildren();
  status(intoStatus, "Opening the dialog in the card…");
  try {
    const code = await scan({ into: "#mount" });
    if (!code) {
      status(intoStatus, "Scan stopped.", "");
    } else {
      status(intoStatus, `Read ${code.symbology}.`, "pass");
      await show(code, intoResult);
    }
  } catch (err) {
    status(intoStatus, /** @type {Error} */ (err).message || String(err), "error");
  } finally {
    intoBtn.disabled = false;
  }
});
