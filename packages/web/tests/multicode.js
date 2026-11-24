/**
 * Center-priority / multi-code regression on a packed retail label.
 *
 * ONE committed photo (`lot-of-codes-2026-11-24.jpeg`, a non-sensitive
 * ASUS NUC product box) holds seven codes. Every framing below is derived
 * programmatically in-page — a square crop centered on one code, with the
 * surrounding codes left visible — in BOTH still mode (`scan({ image })`)
 * and live mode (`still: false` via the bound worker, camera-independent so
 * it runs on Chromium AND WebKit), plus a lone-peripheral-code guard.
 *
 * Strict invariant:
 *   - Every exact match with `score >= CENTERED_SCORE` (0.85) passes.
 *   - A wrong/corrupted payload OR a surrounding code winning is ALWAYS a
 *     hard FAIL — never masked as XFAIL.
 *   - Only an honest empty no-read on a documented low-pitch open framing
 *     (`c128_serial@860`, ~2.1 px/module wide) may XFAIL.
 */

import binding, { options as transfer } from "@webreflection/bindings/message-port";
import barcode from "../api/main.js";
import workerUrl from "../api/worker.js?worker&url";
import { CENTERED_SCORE, PERIPHERAL_CAP } from "../src/rank.js";

const FIXTURE = new URL(
  "../test-codes/regression/lot-of-codes-2026-11-24.jpeg",
  import.meta.url,
);

/** Ground-truth boxes measured on the source photo (1152 × 2048). */
/** @typedef {{ id: string, cx: number, cy: number, sym: string, text: string, openSides?: number[] }} Code */
/** @type {Code[]} */
const CODES = [
  { id: "qr_topright", cx: 872.5, cy: 633.5, sym: "qrcode", text: "W6ARAC0042936RB,WX0X" },
  { id: "c128_sales", cx: 487.5, cy: 655.0, sym: "code128", text: "RNUC14MNK1500002" },
  { id: "ean_right", cx: 811.5, cy: 761.0, sym: "ean13", text: "4711387783078" },
  { id: "c128_pn", cx: 433.0, cy: 856.0, sym: "code128", text: "90AR00M2-M000F0" },
  { id: "upc_right", cx: 804.0, cy: 887.0, sym: "upca", text: "197105783072" },
  { id: "c128_serial", cx: 420.5, cy: 1077.0, sym: "code128", text: "W6ARAC0042936RB", openSides: [860] },
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

/**
 * Framed around `upc_right` at 860px, with the 6 barcodes painted white so
 * ONLY the corner QR remains: pins that peripheral codes are demoted, never
 * dropped.
 * @param {HTMLImageElement} img
 */
const loneCornerQr = (img) => {
  const upc = /** @type {Code} */ (CODES.find((c) => c.id === "upc_right"));
  const canvas = framed(img, upc, 860);
  const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
  // Mask everything below the top-right QR (in 860px coords centered on upc_right:
  // QR sits around x=448..548, y=126..227; barcodes sit left of x=420 or below y=250).
  ctx.fillStyle = "#f4f2ea";
  ctx.fillRect(0, 250, 860, 610);
  ctx.fillRect(0, 0, 420, 260);
  return canvas;
};

/** @param {Code} code @param {number} side @param {any} got @param {{ minScore?: number }} [opts] */
const judge = (code, side, got, opts = {}) => {
  const minScore = opts.minScore ?? CENTERED_SCORE;
  const value = got?.text ?? "";
  if (value === code.text && got?.symbology === code.sym && (got?.score ?? 0) >= minScore) {
    return { ok: true, xfail: false };
  }
  // A corrupted string or a surrounding code winning is ALWAYS a hard fail.
  if (value !== "") return { ok: false, xfail: false };
  // An honest empty no-read is allowed only on documented open framings.
  const open = code.openSides?.includes(side) === true;
  return { ok: open, xfail: open };
};

const run = async () => {
  const tbody = /** @type {HTMLElement} */ (document.querySelector("#rows"));
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

  /** @type {any[]} */
  const rows = [];
  let pass = 0;
  let open = 0;
  let total = 0;

  /**
   * @param {string} id
   * @param {string} framing
   * @param {string} expected
   * @param {any} got
   * @param {number} ms
   * @param {{ ok: boolean, xfail: boolean }} verdict
   */
  const pushRow = (id, framing, expected, got, ms, verdict) => {
    total += 1;
    if (verdict.ok) pass += 1;
    if (verdict.xfail) open += 1;
    rows.push({ code: { id }, side: framing, got, ms, ...verdict });
    const tr = document.createElement("tr");
    if (!verdict.ok) tr.className = "fail";
    else if (verdict.xfail) tr.className = "xfail";
    for (const value of [
      id,
      framing,
      expected,
      got ? `${got.symbology}:${got.text}` : "(no read)",
      got?.score != null ? Number(got.score).toFixed(3) : "-",
      String(ms),
    ]) {
      tr.appendChild(document.createElement("td")).textContent = value;
    }
    tbody.append(tr);
  };

  // 1. STILL mode across all 7 codes × 2 framings (860px wide, 640px ~1.4x zoom).
  for (const code of CODES) {
    for (const side of FRAMINGS) {
      const t0 = performance.now();
      let got = null;
      try {
        got = await api.scan({ image: framed(img, code, side) });
      } catch (err) {
        console.error("multicode still:", code.id, side, err);
      }
      const ms = Math.round(performance.now() - t0);
      pushRow(code.id, `still-${side}`, `${code.sym}:${code.text}`, got, ms, judge(code, side, got));
    }
  }

  // 2. LIVE mode (`still: false` via bound worker — the exact iPhone camera path).
  // Verify every code at 640px AND the key QR-hijack cases (ean_right, upc_right)
  // at 860px, including checking that the corner QR stays in Tier C (<= 0.42).
  const liveWorker = new Worker(workerUrl, { type: "module" });
  const { port1, port2 } = new MessageChannel();
  const utils = /** @type {any} */ (binding(port1, {}));
  liveWorker.postMessage(null, { transfer: [port2] });

  const liveCases = [
    ...CODES.map((code) => ({ code, side: 640 })),
    { code: /** @type {Code} */ (CODES.find((c) => c.id === "ean_right")), side: 860, checkCornerQr: true },
    { code: /** @type {Code} */ (CODES.find((c) => c.id === "upc_right")), side: 860, checkCornerQr: true },
  ];
  try {
    for (const { code, side, checkCornerQr } of liveCases) {
      // Respect the worker's 600ms peripheral live ROI cooldown between independent scenes.
      await new Promise((r) => setTimeout(r, 620));
      const bmp = await createImageBitmap(framed(img, code, side));
      utils[transfer]({ transfer: [bmp] });
      const t0 = performance.now();
      const res = await utils.decode({ bitmap: bmp, still: false, accuracy: "auto" });
      const ms = Math.round(performance.now() - t0);
      const codes = /** @type {any[]} */ (res?.codes ?? []);
      const got = codes.at(0) ?? null;
      let verdict = judge(code, side, got);
      if (verdict.ok && checkCornerQr) {
        const qr = codes.find((c) => c.symbology === "qrcode");
        if (qr && (qr.score ?? 1) > PERIPHERAL_CAP + 1e-6) {
          verdict = { ok: false, xfail: false };
        }
      }
      pushRow(code.id, `live-${side}`, `${code.sym}:${code.text}`, got, ms, verdict);
    }

    // 3. Lone peripheral code guard (still + live): corner QR must still decode
    // when nothing else is in view, and its score must stay peripheral (<= 0.42).
    const qrCode = /** @type {Code} */ (CODES.find((c) => c.id === "qr_topright"));
    {
      const t0 = performance.now();
      const got = await api.scan({ image: loneCornerQr(img) });
      const ms = Math.round(performance.now() - t0);
      const ok =
        got?.symbology === "qrcode" &&
        got?.text === qrCode.text &&
        (got?.score ?? 1) <= PERIPHERAL_CAP + 1e-6;
      pushRow("lone_corner_qr", "still-860", `qrcode:${qrCode.text} (<=0.42)`, got, ms, { ok, xfail: false });
    }
    {
      await new Promise((r) => setTimeout(r, 260));
      const bmp = await createImageBitmap(loneCornerQr(img));
      utils[transfer]({ transfer: [bmp] });
      const t0 = performance.now();
      const res = await utils.decode({ bitmap: bmp, still: false, accuracy: "auto" });
      const ms = Math.round(performance.now() - t0);
      const got = res?.codes?.at(0) ?? null;
      const ok =
        got?.symbology === "qrcode" &&
        got?.text === qrCode.text &&
        (got?.score ?? 1) <= PERIPHERAL_CAP + 1e-6;
      pushRow("lone_corner_qr", "live-860", `qrcode:${qrCode.text} (<=0.42)`, got, ms, { ok, xfail: false });
    }
  } finally {
    liveWorker.terminate();
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
