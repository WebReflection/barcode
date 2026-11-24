/**
 * Pure-module unit checks (no camera, no worker, no wasm): the reticle
 * ranking contract and the ROI proposal geometry. Fast, and the same on
 * every engine, so a regression in the center-priority rules is caught
 * before any decode runs.
 */

import { CENTERED_SCORE, PERIPHERAL_CAP, hasCentered, isCentered, rankCodes } from "../src/rank.js";
import { centerRoiCandidates } from "../src/roi.js";

/** @type {[string, () => void][]} */
const CHECKS = [];
/** @param {string} name @param {() => void} fn */
const check = (name, fn) => CHECKS.push([name, fn]);

/** @param {number} x0 @param {number} y0 @param {number} x1 @param {number} y1 */
const quad = (x0, y0, x1, y1) => [
  { x: x0, y: y0 },
  { x: x1, y: y0 },
  { x: x1, y: y1 },
  { x: x0, y: y1 },
];
/** @param {number} x @param {number} y @param {number} side */
const square = (x, y, side) => quad(x, y, x + side, y + side);

check("a dead-center quad scores ~1 and is centered", () => {
  const [code] = rankCodes([{ position: square(400, 400, 200) }], 1000, 1000);
  if (!(code.score > 0.99)) throw new Error(`score ${code.score}`);
  if (!isCentered(code)) throw new Error("isCentered should be true");
});

check("a corner quad is peripheral and capped", () => {
  const codes = rankCodes(
    [
      { position: square(850, 20, 120), id: "corner" },
      { position: square(830, 830, 140), id: "other" },
    ],
    1000,
    1000,
  );
  for (const code of codes) {
    if (!(code.score <= PERIPHERAL_CAP + 1e-9)) throw new Error(`${code.id} score ${code.score}`);
    if (isCentered(code)) throw new Error(`${code.id} must not be centered`);
  }
});

check("tiers never overlap and Tier B never passes isCentered", () => {
  const centered = rankCodes([{ position: square(430, 430, 140) }], 1000, 1000)[0];
  // A bar strip on the horizontal center line but ending left of the center
  // column, so the frame center point is NOT inside it.
  const onLine = rankCodes([{ position: quad(60, 470, 380, 530) }], 1000, 1000)[0];
  const peripheral = rankCodes([{ position: square(20, 20, 90) }], 1000, 1000)[0];
  if (!(centered.score > onLine.score)) throw new Error(`centered ${centered.score} <= onLine ${onLine.score}`);
  if (!(onLine.score > peripheral.score)) throw new Error(`onLine ${onLine.score} <= peripheral ${peripheral.score}`);
  if (!(centered.score >= CENTERED_SCORE)) throw new Error("centered below the gate");
  if (!(onLine.score < CENTERED_SCORE)) throw new Error(`Tier B ${onLine.score} must stay below CENTERED_SCORE ${CENTERED_SCORE}`);
  if (!isCentered(centered)) throw new Error("centered must pass isCentered");
  if (isCentered(onLine)) throw new Error("Tier B must NOT pass isCentered");
});

check("a square/horizontal code stacked above the reticle stays Tier C", () => {
  // Directly above (500, 500) on x = 500 (like a top-right QR above an EAN
  // on a packed label): must NOT get a Tier B bonus just for sharing x = cx.
  const qrAbove = rankCodes([{ position: square(440, 160, 120) }], 1000, 1000)[0];
  const barBelow = rankCodes([{ position: quad(300, 680, 700, 740) }], 1000, 1000)[0];
  if (!(qrAbove.score <= PERIPHERAL_CAP + 1e-9)) throw new Error(`qrAbove ${qrAbove.score} leaked out of Tier C`);
  if (!(barBelow.score <= PERIPHERAL_CAP + 1e-9)) throw new Error(`barBelow ${barBelow.score} leaked out of Tier C`);
});

check("a vertical strip crossing the vertical center axis is Tier B", () => {
  // Tall vertical 1D strip (h > 1.5 * w) stopping above the center row.
  const code = rankCodes([{ position: quad(470, 40, 530, 380) }], 1000, 1000)[0];
  if (!(code.score > PERIPHERAL_CAP)) throw new Error(`score ${code.score}`);
  if (!(code.score < CENTERED_SCORE)) throw new Error(`score ${code.score} above Tier B`);
});

check("ranking is best-first and keeps every candidate", () => {
  const codes = rankCodes(
    [
      { position: square(20, 20, 80), id: "corner" },
      { position: square(430, 430, 140), id: "center" },
      { position: quad(60, 470, 380, 530), id: "line" },
    ],
    1000,
    1000,
  );
  if (codes.length !== 3) throw new Error(`length ${codes.length}`);
  if (codes.map((c) => c.id).join() !== "center,line,corner") {
    throw new Error(`order ${codes.map((c) => c.id).join()}`);
  }
  if (!hasCentered(codes)) throw new Error("hasCentered");
});

check("hasCentered is false for a lone peripheral read", () => {
  const codes = rankCodes([{ position: square(860, 20, 100) }], 1000, 1000);
  if (hasCentered(codes)) throw new Error("a corner read must not satisfy the reticle");
});

check("centerRoiCandidates covers the reticle and stays in bounds", () => {
  const boxes = centerRoiCandidates(1200, 900, { linear: true, square: true });
  if (boxes.length !== 3) throw new Error(`boxes ${boxes.length}`);
  const [tight, wide, sq] = boxes;
  if (!tight.linear || !wide.linear || sq.linear) throw new Error("linear flags");
  for (const b of boxes) {
    if (b.x < 0 || b.y < 0 || b.x + b.w > 1200 || b.y + b.h > 900) throw new Error(`out of bounds ${JSON.stringify(b)}`);
    // The frame center must be inside every reticle crop.
    if (600 < b.x || 600 > b.x + b.w || 450 < b.y || 450 > b.y + b.h) throw new Error(`not centered ${JSON.stringify(b)}`);
  }
  if (tight.w >= wide.w) throw new Error("the tight strip must be narrower");
  if (!(sq.w === sq.h)) throw new Error("the square crop must be square");
});

const run = () => {
  const rows = /** @type {HTMLTableSectionElement} */ (document.querySelector("#rows"));
  let pass = 0;
  for (const [name, fn] of CHECKS) {
    const tr = document.createElement("tr");
    let note = "PASS";
    try {
      fn();
      pass += 1;
    } catch (err) {
      note = /** @type {Error} */ (err).message;
      tr.className = "fail";
    }
    for (const value of [name, note]) tr.appendChild(document.createElement("td")).textContent = value;
    rows.append(tr);
  }
  const ok = pass === CHECKS.length;
  const summary = /** @type {HTMLElement} */ (document.querySelector("#summary"));
  summary.textContent = ok ? `UNIT PASS ${pass}/${CHECKS.length}` : `UNIT FAIL ${pass}/${CHECKS.length}`;
  summary.className = ok ? "pass" : "fail";
  document.title = ok ? "UNIT PASS" : "UNIT FAILED";
};

run();
