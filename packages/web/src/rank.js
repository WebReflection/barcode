/**
 * Center-priority ranking for decode results.
 *
 * The default UI draws a horizontal line across the middle of the
 * viewfinder: that line — and the center point it crosses — is what the
 * user is aiming with, so the decoder must honour the same promise. A flat
 * "distance from the center" score is not enough: on a packed label a
 * corner QR 130px above the center still scored 0.77-0.82 and outranked
 * (and, through the worker's early exits, suppressed) the barcode under
 * the line.
 *
 * The score is therefore tiered by *reticle overlap*:
 *
 *   Tier A — the frame center point is inside the symbol's quad
 *            (`0.85 .. 1.00`): the code under the reticle.
 *   Tier B — the quad crosses the horizontal center scan line (or the
 *            vertical axis for a vertically oriented 1D strip), but not
 *            the center point (`0.45 .. 0.78`).
 *   Tier C — surrounding / peripheral (`0 .. 0.42`), steeply decaying.
 *
 * The floors do not overlap and `CENTERED_SCORE` sits at Tier A's floor
 * (`0.85`), so a non-centered Tier B symbol can never trigger centered
 * early exits or 2-frame centered confirmation. Tiering is a *ranking*,
 * never a filter: a peripheral code is still returned, it just loses the
 * race. The public contract (`score` 1 = dead center, 0 = frame corner;
 * bigger symbol breaks ties) is unchanged.
 */

/** @typedef {{ position: Array<{x: number, y: number}>, score?: number }} Rankable */

/** Tier A floor: only a quad containing the frame center is "centered". */
export const CENTERED_SCORE = 0.85;
/** Tier C ceiling: surrounding codes never reach a centered tier. */
export const PERIPHERAL_CAP = 0.42;
/** How much of the viewfinder edge the UI scan line spans (12% inset). */
const INSET = 0.12;
/** Tolerance (fraction of the shorter side) for a center point sitting on
 *  a thin 1D strip's edge: a 1D decoder that early-returns after 2 rows
 *  reports a 1-2px-tall sliver even when the physical bars span ~10% of the
 *  frame across the reticle. 5% (~32px on 640px) covers the bar half-height
 *  while staying well inside the ~15-20% gap to any neighbouring code. */
const TOL = 0.05;

/**
 * Attach `score` to every code and sort best-first. Coordinates are
 * mask-space — the same space the quads already are in — so the frame
 * center is the viewfinder/reticle center for square and full crops.
 * @template {Rankable} T
 * @param {T[]} codes @param {number} width @param {number} height
 * @returns {T[]}
 */
export const rankCodes = (codes, width, height) => {
  const w = width || 1;
  const h = height || 1;
  const cx = w / 2;
  const cy = h / 2;
  // Normalize by the HALF SHORTER side: a corner must be genuinely far,
  // and a tall frame must not dilute the horizontal distance.
  const radius = Math.min(w, h) / 2 || 1;
  const tol = TOL * Math.min(w, h);
  const lx0 = INSET * w;
  const lx1 = w - lx0;
  const ly0 = INSET * h;
  const ly1 = h - ly0;
  for (const code of codes) {
    const position = code.position ?? [];
    if (!position.length) {
      code.score = 0;
      continue;
    }
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    let sx = 0;
    let sy = 0;
    for (const { x, y } of position) {
      if (x < x0) x0 = x;
      if (x > x1) x1 = x;
      if (y < y0) y0 = y;
      if (y > y1) y1 = y;
      sx += x;
      sy += y;
    }
    const n = position.length;
    const inside =
      cx >= x0 - tol && cx <= x1 + tol && cy >= y0 - tol && cy <= y1 + tol;
    // The UI draws a horizontal scan line across the middle of the frame.
    // Only vertically oriented strips (height > 1.5 * width) also qualify on
    // the vertical center axis — otherwise codes stacked above or below the
    // reticle on a packed label (like a top-right QR above an EAN-13) would
    // get promoted from Tier C into Tier B.
    const verticalStrip = y1 - y0 > (x1 - x0) * 1.5;
    const onLine =
      (cy >= y0 - tol && cy <= y1 + tol && x1 >= lx0 && x0 <= lx1) ||
      (verticalStrip && cx >= x0 - tol && cx <= x1 + tol && y1 >= ly0 && y0 <= ly1);
    // Distance from the center point to the axis-aligned quad box (0 when
    // inside), blended with the centroid distance.
    const dBox = Math.hypot(
      Math.max(x0 - cx, 0, cx - x1),
      Math.max(y0 - cy, 0, cy - y1),
    );
    const dCentroid = Math.hypot(sx / n - cx, sy / n - cy);
    const dNorm = Math.min(1, (0.65 * dBox + 0.35 * dCentroid) / radius);
    const radial = Math.exp(-3.2 * dNorm * dNorm);
    let score;
    if (inside) score = 0.85 + 0.15 * (1 - Math.min(1, dCentroid / radius));
    else if (onLine) score = Math.min(0.78, 0.45 + 0.33 * radial);
    else score = Math.min(PERIPHERAL_CAP, PERIPHERAL_CAP * radial);
    code.score = Math.max(0, Math.min(1, score));
  }
  return codes.sort(
    (a, b) =>
      (b.score ?? 0) - (a.score ?? 0) ||
      quadArea(b.position ?? []) - quadArea(a.position ?? []),
  );
};

/** Unsigned quad area (shoelace), only the sort tiebreak. */
const quadArea = (/** @type {Array<{x: number, y: number}>} */ p) => {
  let a = 0;
  for (let i = 0; i < p.length; i++) {
    const cur = p[i] ?? { x: 0, y: 0 };
    const next = p[(i + 1) % p.length] ?? cur;
    a += cur.x * next.y - next.x * cur.y;
  }
  return Math.abs(a) / 2;
};

/** True when a code sits under the reticle: the one-code contract. */
export const isCentered = (/** @type {Rankable | undefined} */ code) =>
  (code?.score ?? 0) >= CENTERED_SCORE;

/** True when at least one candidate is centered. */
export const hasCentered = (/** @type {Rankable[]} */ codes) => codes.some(isCentered);
