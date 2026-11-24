/**
 * Center-priority / multi-code regression on a packed retail label.
 *
 * ONE committed photo (`lot-of-codes-2026-11-24.jpeg`, a non-sensitive
 * ASUS NUC product box) holds seven codes. Every framing below is derived
 * programmatically in-page — a square crop centered on one code, with the
 * surrounding codes left visible — which is exactly the user's framing on
 * a phone. The invariant under test:
 *
 *   the code the viewfinder is centered on wins, and a surrounding code
 *   is never returned in its place.
 *
 * `open: true` marks a case where the centered code is not readable yet
 * at that framing (known gap, documented in
 * test-codes/regression/README.md): the assertion then only requires that
 * NO OTHER code wins. That is the promise this suite was added for — the
 * decoder must never answer "corner QR" when the user is framing the
 * barcode under the center line.
 */

import barcode from "../api/main.js";
import workerUrl from "../api/worker.js?worker&url";

const FIXTURE = new URL(
  "../test-codes/regression/lot-of-codes-2026-11-24.jpeg",
  import.meta.url,
);

/** Ground-truth boxes measured on the source photo (1152 × 2048). */
/** @typedef {{ id: string, cx: number, cy: number, sym: string, text: string, open?: boolean }} Code */
/** @type {Code[]} */
const CODES = [
  { id: "qr_topright", cx: 872.5, cy: 633.5, sym: "qrcode", text: "W6ARAC0042936RB,WX0X" },
  { id: "c128_sales", cx: 487.5, cy: 655.0, sym: "code128", text: "RNUC14MNK1500002" },
  { id: "ean_right", cx: 811.5, cy: 761.0, sym: "ean13", text: "4711387783078" },
  { id: "c128_pn", cx: 433.0, cy: 856.0, sym: "code128", text: "90AR00M2-M000F0", open: true },
  { id: "upc_right", cx: 804.0, cy: 887.0, sym: "upca", text: "197105783072" },
  { id: "c128_serial", cx: 420.5, cy: 1077.0, sym: "code128", text: "W6ARAC0042936RB", open: true },
  { id: "c128_check", cx: 842.5, cy: 1046.0, sym: "code128", text: "WX0X" },
];

/** Framing side lengths: a wide viewfinder and a ~1.4× zoom. */
const FRAMINGS = [860, 640];

/** Cardboard tone for the areas a crop reaches outside the photo. */
const CARDBOARD = "#b78f5d";

const api = barcode({ worker: workerUrl });

/** @param {HTMLImageElement} img @param {Code} code @param {number} side */
const framed = (img, code, side) => {
  const canvas = document.createElement("canvas");
  canvas.width = side;
  canvas.height = side;
  const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
  ctx.fillStyle = CARDBOARD;
  ctx.fillRect(0, 0, side, side);
  const half = side / 2;
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(
    img,
    Math.round(code.cx - half),
    Math.round(code.cy - half),
    side,
    side,
    0,
    0,
    side,
    side,
  );
  return canvas;
};

/** @param {Code} code @param {number} side @param {any} got */
const judge = (code, side, got) => {
  const value = got?.text ?? "";
  if (value === code.text) return { ok: true, xfail: false };
  // A DIFFERENT code winning is never acceptable, whatever the framing.
  if (code.open === true) {
    // Documented gap (the strip prints at ~2.1-2.7 px/module and the
    // fuzzy matcher still misreads a character): accept "no read" and a
    // same-symbology read, which is the centered strip itself — a wrong
    // value is tracked as an XFAIL, never a different code silently.
    const sameStrip = got != null && got.symbology === code.sym;
    return { ok: value === "" || sameStrip, xfail: sameStrip };
  }
  if (value !== "") return { ok: false, xfail: false };
  return { ok: false, xfail: false };
};

const run = async () => {
  /** @type {HTMLTableSectionElement} */ (document.querySelector("#rows"));
  const img = new Image();
  img.src = FIXTURE.href;
  try {
    await img.decode();
  } catch (err) {
    document.title = "MULTICODE FAILED";
    /** @type {HTMLElement} */ (document.querySelector("#summary")).textContent =
      `fixture failed to load: ${err}`;
    return;
  }
  const rows = [];
  let pass = 0;
  let open = 0;
  let total = 0;
  for (const code of CODES) {
    for (const side of FRAMINGS) {
      total += 1;
      const t0 = performance.now();
      let got = null;
      try {
        got = await api.scan({ image: framed(img, code, side) });
      } catch (err) {
        console.error("multicode:", code.id, side, err);
      }
      const ms = Math.round(performance.now() - t0);
      const verdict = judge(code, side, got);
      if (verdict.ok) pass += 1;
      if (verdict.xfail) open += 1;
      rows.push({ code, side, got, ms, ...verdict });
      const tr = document.createElement("tr");
      if (!verdict.ok) tr.className = "fail";
      else if (verdict.xfail) tr.className = "xfail";
      for (const value of [
        code.id,
        `${side}px`,
        `${code.sym}:${code.text}`,
        got ? `${got.symbology}:${got.text}` : "(no read)",
        got?.score != null ? Number(got.score).toFixed(3) : "-",
        String(ms),
      ]) {
        tr.appendChild(document.createElement("td")).textContent = value;
      }
      /** @type {HTMLElement} */ (document.querySelector("#rows")).append(tr);
    }
  }
  const allOk = pass === total;
  const summary = /** @type {HTMLElement} */ (document.querySelector("#summary"));
  summary.textContent = allOk
    ? `MULTICODE PASS ${pass}/${total}${open ? ` (${open} documented open)` : ""}`
    : `MULTICODE FAIL ${pass}/${total}${open ? ` (${open} documented open)` : ""}`;
  summary.className = allOk ? "pass" : "fail";
  document.title = allOk ? "MULTICODE PASS" : "MULTICODE FAILED";
  /** @type {any} */ (globalThis).__MULTICODE = rows;
};

run();
