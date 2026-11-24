/**
 * Region-of-interest candidates for the native-resolution pass (roadmap
 * item 8).
 *
 * The binarizer downscales every camera/photo frame to `maxSize` (1056
 * live, 2112 still). A code that is small in the frame loses exactly the
 * module pitch it needs: at 4096px a 60px QR is 15px after the 1056
 * downscale, while a crop of the original source keeps all 60px. This
 * module proposes those crops from the mask the full-frame sweep already
 * produced — no extra pass over the source image.
 *
 * The detector is deliberately symbology-agnostic: coarse cell activity
 * → connected components → ink density. Barcodes and 2D symbols are
 * compact, high-density ink clusters, so they rank above text lines and
 * background texture without knowing which family they are. The actual
 * arbitration is still the decoder's (format/RS/checksum), so a wrong
 * candidate only costs one crop.
 */

import { LINEAR } from "./live-schedule.js";

/** @typedef {import("./gpu/binarizer.js").BinarizedFrame} BinarizedFrame */

/**
 * A candidate box in the frame's SOURCE pixel coordinates (not mask
 * coordinates). `linear` marks a bar-strip proposal (1D decoders only).
 * @typedef {{ x: number, y: number, w: number, h: number, score: number, linear?: boolean }} RoiBox
 */

/** Coarse cell side in mask pixels. */
const CELL = 24;
/** Ink samples are taken on a stride inside a cell (cost, not precision). */
const SAMPLE_STRIDE = 3;
/** A cell needs this ink fraction to count as activity. */
const CELL_INK = 0.045;
/** Candidates below this many mask pixels on either side are noise. */
const MIN_SIDE = 8;
/** Never propose a crop that is almost the whole frame. */
const MAX_AREA_FRACTION = 0.72;

/**
 * Reticle weight for a proposal: the decoder's crop budget and deadline
 * are finite, so the box under the UI's center reticle must be tried
 * before surrounding boxes. Returns 3 when the box contains the frame
 * center, 1.8 when it crosses either center axis (the UI scan line), and
 * decays to 0.5 at the corners.
 * @param {number} x @param {number} y @param {number} w @param {number} h
 * @param {number} sourceW @param {number} sourceH
 */
const centerWeight = (x, y, w, h, sourceW, sourceH) => {
  const cx = sourceW / 2;
  const cy = sourceH / 2;
  if (cx >= x && cx <= x + w && cy >= y && cy <= y + h) return 3;
  const onAxis = (cy >= y && cy <= y + h) || (cx >= x && cx <= x + w);
  const d =
    Math.hypot(
      Math.max(x - cx, 0, cx - (x + w)),
      Math.max(y - cy, 0, cy - (y + h)),
    ) / (Math.min(sourceW, sourceH) / 2 || 1);
  return (onAxis ? 1.8 : 1) * Math.max(0.5, 1 - 0.6 * Math.min(1, d));
};

/**
 * The reticle itself as crops, best first: the code the user is aiming
 * at, at native resolution, isolated from every surrounding code.
 *
 * Connected components merge a packed label's stacked barcodes + text
 * into one blob, and the density ranking then favours whichever code in
 * the frame has the most bars — not the one under the line. These
 * deterministic boxes encode the UI contract directly:
 *
 *   - wide horizontal strip centered on the frame's mid line (the
 *     viewfinder's scan line) for a 1D symbol under it;
 *   - tight strip on the same line for a short centered symbol beside a
 *     longer neighbor on the same band;
 *   - center square for a centered 2D symbol.
 *
 * @param {number} sourceW @param {number} sourceH
 * @param {{ linear?: boolean, square?: boolean }} [opts]
 * @returns {RoiBox[]}
 */
export function centerRoiCandidates(sourceW, sourceH, opts = {}) {
  const { linear = true, square = true } = opts;
  if (!(sourceW > 0 && sourceH > 0)) return [];
  const cx = sourceW / 2;
  const cy = sourceH / 2;
  const minSide = Math.min(sourceW, sourceH);
  /** @type {RoiBox[]} */
  const boxes = [];
  if (linear) {
    const wideH = Math.min(sourceH, Math.max(96, Math.round(sourceH * 0.24)));
    const tightH = Math.min(sourceH, Math.max(64, Math.round(sourceH * 0.14)));
    const wideW = Math.max(MIN_SIDE, Math.round(sourceW * 0.92));
    const tightW = Math.max(MIN_SIDE, Math.round(sourceW * 0.65));
    const push = (/** @type {number} */ w, /** @type {number} */ h, /** @type {number} */ score) => {
      const x = Math.max(0, Math.round(cx - w / 2));
      const y = Math.max(0, Math.round(cy - h / 2));
      const bw = Math.min(sourceW - x, Math.round(w));
      const bh = Math.min(sourceH - y, Math.round(h));
      if (bw < MIN_SIDE || bh < MIN_SIDE) return;
      boxes.push({ x, y, w: bw, h: bh, score, linear: true });
    };
    push(wideW, wideH, 30);
    push(tightW, tightH, 26);
  }
  if (square) {
    const side = Math.max(MIN_SIDE, Math.round(minSide * 0.5));
    const x = Math.max(0, Math.round(cx - side / 2));
    const y = Math.max(0, Math.round(cy - side / 2));
    const s = Math.min(side, sourceW - x, sourceH - y);
    if (s >= MIN_SIDE) boxes.push({ x, y, w: s, h: s, score: 28, linear: false });
  }
  return boxes;
}

/** @param {BinarizedFrame} frame @param {number} x @param {number} y */
const inkAt = (frame, x, y) =>
  (frame.data[y * frame.bytesPerRow + (x >> 3)] ?? 0) >> (x & 7) & 1;

/**
 * Connected components of active cells, 8-connected. Returns cell lists.
 * @param {Uint8Array} active @param {number} cols @param {number} rows
 * @returns {number[][]}
 */
const components = (active, cols, rows) => {
  const seen = new Uint8Array(active.length);
  /** @type {number[][]} */
  const out = [];
  for (let start = 0; start < active.length; start++) {
    if (!active[start] || seen[start]) continue;
    /** @type {number[]} */
    const cells = [];
    /** @type {number[]} */
    const stack = [start];
    seen[start] = 1;
    while (stack.length) {
      const i = /** @type {number} */ (stack.pop());
      cells.push(i);
      const cx = i % cols;
      const cy = (i / cols) | 0;
      for (let dy = -1; dy <= 1; dy++) {
        for (let dx = -1; dx <= 1; dx++) {
          if (!dx && !dy) continue;
          const nx = cx + dx;
          const ny = cy + dy;
          if (nx < 0 || ny < 0 || nx >= cols || ny >= rows) continue;
          const n = ny * cols + nx;
          if (active[n] && !seen[n]) {
            seen[n] = 1;
            stack.push(n);
          }
        }
      }
    }
    out.push(cells);
  }
  return out;
};

/**
 * Ink fraction over a box, sampled on a stride. Cheap: only used for the
 * handful of candidate components, never the whole frame.
 * @param {BinarizedFrame} frame
 * @param {number} x0 @param {number} y0 @param {number} x1 @param {number} y1
 */
const sampleDensity = (frame, x0, y0, x1, y1) => {
  const step = Math.max(2, Math.round(Math.min(x1 - x0, y1 - y0) / 24));
  let ink = 0;
  let total = 0;
  for (let y = y0; y < y1; y += step) {
    for (let x = x0; x < x1; x += step) {
      total++;
      ink += inkAt(frame, x, y);
    }
  }
  return total ? ink / total : 0;
};

/**
 * Propose source-space ROI boxes for the native-resolution pass, best
 * first. `frame` is the binarized full-frame mask; `sourceW/sourceH` are
 * the original bitmap dimensions that mask came from.
 *
 * @param {BinarizedFrame} frame
 * @param {number} sourceW
 * @param {number} sourceH
 * @param {number} [limit]
 * @returns {RoiBox[]}
 */
export function roiCandidates(frame, sourceW, sourceH, limit = 4) {
  if (!(frame.width > 0 && frame.height > 0 && sourceW > 0 && sourceH > 0)) return [];
  const cols = Math.ceil(frame.width / CELL);
  const rows = Math.ceil(frame.height / CELL);
  const active = new Uint8Array(cols * rows);
  for (let cy = 0; cy < rows; cy++) {
    const y0 = cy * CELL;
    const y1 = Math.min(frame.height, y0 + CELL);
    for (let cx = 0; cx < cols; cx++) {
      const x0 = cx * CELL;
      const x1 = Math.min(frame.width, x0 + CELL);
      let ink = 0;
      let total = 0;
      for (let y = y0; y < y1; y += SAMPLE_STRIDE) {
        for (let x = x0; x < x1; x += SAMPLE_STRIDE) {
          total++;
          ink += inkAt(frame, x, y);
        }
      }
      if (total && ink / total >= CELL_INK) active[cy * cols + cx] = 1;
    }
  }

  // Mask scale: the binarizer pads sources that already fit 1:1 and only
  // downscales sources larger than maxSize. A padded mask is wider than
  // its source, so the source<->mask scale is 1 in that case.
  const sx = sourceW <= frame.width ? 1 : sourceW / frame.width;
  const sy = sourceH <= frame.height ? 1 : sourceH / frame.height;

  /** @type {RoiBox[]} */
  const boxes = [];
  const frameArea = frame.width * frame.height;
  for (const cells of components(active, cols, rows)) {
    let minX = cols;
    let minY = rows;
    let maxX = 0;
    let maxY = 0;
    for (const i of cells) {
      const cx = i % cols;
      const cy = (i / cols) | 0;
      minX = Math.min(minX, cx);
      minY = Math.min(minY, cy);
      maxX = Math.max(maxX, cx);
      maxY = Math.max(maxY, cy);
    }
    const mx0 = minX * CELL;
    const my0 = minY * CELL;
    const mx1 = Math.min(frame.width, (maxX + 1) * CELL);
    const my1 = Math.min(frame.height, (maxY + 1) * CELL);
    const bw = mx1 - mx0;
    const bh = my1 - my0;
    if (bw < MIN_SIDE || bh < MIN_SIDE) continue;
    if (bw * bh > MAX_AREA_FRACTION * frameArea) continue;
    const density = sampleDensity(frame, mx0, my0, mx1, my1);
    if (density < CELL_INK) continue;
    // The cell component overstates the symbol by up to a cell on every
    // side. A 1D bar code is only ~2 modules tall at these scales, so a
    // 24px cell boundary can double the crop height — and the binarizer's
    // 32px snap then pads a 70px crop to 96px, a mask on which the row
    // decoder stops finding a clean multi-row consensus. Shrink to the
    // actual ink extents first; the margin below is the quiet zone.
    let ix0 = mx1;
    let iy0 = my1;
    let ix1 = mx0;
    let iy1 = my0;
    for (let y = my0; y < my1; y++) {
      for (let x = mx0; x < mx1; x++) {
        if (!inkAt(frame, x, y)) continue;
        if (x < ix0) ix0 = x;
        if (x > ix1) ix1 = x;
        if (y < iy0) iy0 = y;
        if (y > iy1) iy1 = y;
      }
    }
    if (ix1 < ix0 || iy1 < iy0) continue;
    // Compactness: how much of the cell bounding box is active. A solid
    // code scores ~1; a sparse text block or a page rule scores lower.
    const coverage = cells.length / ((maxX - minX + 1) * (maxY - minY + 1));
    // Prefer dense, compact clusters; area only breaks near-ties so a
    // 200px QR is not buried by a 24px speck of print noise. The reticle
    // weight then puts the cluster under the UI's center reticle ahead of
    // an equally dense surrounding one on a packed label.
    const score =
      density *
      (0.55 + 0.45 * coverage) *
      (1 + Math.log10(1 + bw * bh) / 8) *
      centerWeight(mx0, my0, bw, bh, frame.width, frame.height);
    // Ink bounds stop at the first/last dark module — the quiet zone is
    // white and invisible. 12% of the box side is ~4 QR modules and ~10
    // Code128 modules at typical module counts.
    const sw = (ix1 - ix0 + 1) * sx;
    const sh = (iy1 - iy0 + 1) * sy;
    const padX = Math.max(8, Math.round(sw * 0.12));
    const padY = Math.max(8, Math.round(sh * 0.12));
    const x = Math.max(0, Math.round(ix0 * sx) - padX);
    const y = Math.max(0, Math.round(iy0 * sy) - padY);
    const w = Math.min(sourceW - x, Math.round(sw) + 2 * padX);
    const h = Math.min(sourceH - y, Math.round(sh) + 2 * padY);
    if (w < MIN_SIDE || h < MIN_SIDE) continue;
    boxes.push({ x, y, w, h, score });
  }

  // Best score first. Overlapping proposals add nothing: the same crop
  // would be binarized and decoded twice.
  boxes.sort((a, b) => b.score - a.score);
  /** @type {RoiBox[]} */
  const picked = [];
  for (const b of boxes) {
    const overlap = picked.some((p) => {
      const ix = Math.max(0, Math.min(b.x + b.w, p.x + p.w) - Math.max(b.x, p.x));
      const iy = Math.max(0, Math.min(b.y + b.h, p.y + p.h) - Math.max(b.y, p.y));
      return ix * iy > 0.6 * Math.min(b.w * b.h, p.w * p.h);
    });
    if (!overlap) picked.push(b);
    if (picked.length >= limit) break;
  }
  return picked;
}

/** Internal bit-transition count per byte (`popcount((b ^ (b >> 1)) & 0x7f)`). */
const BYTE_TRANS = (() => {
  const lut = new Uint8Array(256);
  for (let b = 0; b < 256; b++) {
    let x = (b ^ (b >> 1)) & 0x7f;
    let c = 0;
    while (x) {
      x &= x - 1;
      c++;
    }
    lut[b] = c;
  }
  return lut;
})();

/**
 * Row-scan (1D) ROI candidates. The compact-cluster detector above sees a
 * barcode as a thin band of high-frequency ink, and a wide symbol often
 * loses to a text block or merges with the box/hand around it. A 1D reader
 * only needs one clean scanline, so look for rows with many black/white
 * transitions and crop the densest horizontal run of them. Cheap: one
 * pass over the mask, and it runs only after the full-frame pass missed.
 *
 * @param {BinarizedFrame} frame
 * @param {number} sourceW @param {number} sourceH
 * @param {number} [limit]
 * @returns {RoiBox[]}
 */
export function linearRoiCandidates(frame, sourceW, sourceH, limit = 2) {
  if (!(frame.width > 0 && frame.height > 0 && sourceW > 0 && sourceH > 0)) return [];
  const { width, height, bytesPerRow, data } = frame;
  const fullBytes = width >> 3;
  const tailStart = fullBytes << 3;
  /** @param {number} y */
  const transitions = (y) => {
    let count = 0;
    let prev = 0;
    const rowOff = y * bytesPerRow;
    for (let bx = 0; bx < fullBytes; bx++) {
      const b = data[rowOff + bx] ?? 0;
      count += (BYTE_TRANS[b] ?? 0) + ((b & 1) ^ prev);
      prev = b >> 7;
    }
    for (let x = tailStart; x < width; x++) {
      const bit = ((data[rowOff + (x >> 3)] ?? 0) >> (x & 7)) & 1;
      if (bit !== prev) count++;
      prev = bit;
    }
    return count;
  };

  const counts = new Uint16Array(height);
  let max = 0;
  for (let y = 0; y < height; y++) {
    const n = transitions(y);
    counts[y] = n;
    if (n > max) max = n;
  }
  // A real 1D symbol is a contiguous run of rows with a near-constant,
  // very high transition count. A lower floor (0.4) also picks up the
  // human-readable text line and label borders, and those merge with the
  // bars into a full-width, 300px-tall crop on a phone photo. 0.7 keeps
  // only the bar rows; the x extent then comes from the same rows, so the
  // crop is the symbol (plus quiet zone) instead of the whole label.
  //
  // 0.85 used to keep ONLY the densest strip in the frame. On a packed
  // label (several barcodes stacked) the widest one owns `max`, and a
  // narrower neighbour fell below the floor and never got a tight crop —
  // the P/N and Serial strips of the lot-of-codes box measured 0.76/0.70
  // of `max`. 0.7 still separates bars from text rows.
  const sx = sourceW <= width ? 1 : sourceW / width;
  const sy = sourceH <= height ? 1 : sourceH / height;
  const denseFloor = Math.max(24, max * 0.7);
  /** @type {{ y0: number, y1: number }[]} */
  const dense = [];
  for (let y = 0; y < height; y++) {
    if ((counts[y] ?? 0) < denseFloor) continue;
    const band = dense.at(-1);
    if (band && y <= band.y1 + 2) band.y1 = y;
    else dense.push({ y0: y, y1: y });
  }
  /** @type {RoiBox[]} */
  const tight = [];
  for (const band of dense) {
    const bh = band.y1 - band.y0 + 1;
    if (bh < 2 || bh > height * 0.4) continue;
    /** @type {number[]} */
    const x0s = [];
    /** @type {number[]} */
    const x1s = [];
    for (let y = band.y0; y <= band.y1; y++) {
      /** @type {number[]} */
      const flips = [];
      let prev = 0;
      for (let x = 0; x < width; x++) {
        const bit = (data[y * bytesPerRow + (x >> 3)] ?? 0) >> (x & 7) & 1;
        if (bit !== prev) flips.push(x);
        prev = bit;
      }
      if (flips.length < 8) continue;
      // Split the row at the widest white gaps (the label's vertical text,
      // bubble-wrap glare) and keep the segment with the most flips: the
      // bars are one dense segment, everything around it is sparse.
      const gap = Math.max(16, Math.round(width * 0.02));
      let start = 0;
      /** @type {{ x0: number, x1: number, n: number } | null} */
      let best = null;
      for (let i = 0; i <= flips.length; i++) {
        if (
          i === flips.length ||
          (i > 0 && (flips[i] ?? 0) - (flips[i - 1] ?? 0) > gap)
        ) {
          const seg = { x0: flips[start] ?? 0, x1: flips[i - 1] ?? 0, n: i - start };
          if (!best || seg.n > best.n) best = seg;
          start = i;
        }
      }
      if (best && best.x1 - best.x0 > width * 0.15) {
        x0s.push(best.x0);
        x1s.push(best.x1);
      }
    }
    if (x0s.length < 2) continue;
    /** @param {number[]} values */
    const median = (values) => values.slice().sort((a, b) => a - b)[(values.length / 2) | 0] ?? 0;
    const x0 = median(x0s);
    const x1 = median(x1s);
    // Only a bar strip that owns a large share of the frame width is the
    // "phone held at a label" case this tight splitter exists for. A
    // small/distant symbol (or a product box's short Code 32) keeps the
    // established looser proposals below; taking its narrow band here
    // used to push those crops past the still ROI deadline.
    if (x1 - x0 < width * 0.4) continue;
    const padX = Math.max(8, Math.round((x1 - x0) * 0.03));
    const padY = Math.max(8, Math.round(bh * 0.2));
    const x = Math.max(0, Math.round((x0 - padX) * sx));
    const y = Math.max(0, Math.round((band.y0 - padY) * sy));
    const w = Math.min(sourceW - x, Math.round((x1 - x0 + 2 * padX) * sx));
    const h = Math.min(sourceH - y, Math.round((bh + 2 * padY) * sy));
    if (w < MIN_SIDE || h < MIN_SIDE) continue;
    tight.push({
      x, y, w, h,
      score: (x1 - x0) * centerWeight(x, y, w, h, sourceW, sourceH),
    });
  }
  tight.sort((a, b) => b.score - a.score);
  // Fallback: the original looser band grouping. A frame with no clean
  // dense-row band (a symbol partly cut off, or a very noisy mask) still
  // gets the old proposal.
  // Text lines have their own transition rhythm; only the densest rows are
  // worth a crop. 24 transitions is ~12 bars even for the narrowest 1D.
  const floor = Math.max(24, max * 0.4);
  /** @type {{ y0: number, y1: number, max: number }[]} */
  const bands = [];
  for (let y = 0; y < height; y++) {
    const count = counts[y] ?? 0;
    if (count < floor) continue;
    const band = bands.at(-1);
    if (band && y <= band.y1 + 8) {
      band.y1 = y;
      band.max = Math.max(band.max, count);
    } else {
      bands.push({ y0: y, y1: y, max: count });
    }
  }

  /** @type {RoiBox[]} */
  const boxes = [];
  for (const band of bands) {
    // Random texture (noise, foliage, fabric) puts transitions on every
    // row, merging into a full-height band. A 1D symbol is a horizontal
    // band, not most of the frame; skipping it keeps the junk-frame
    // fallback cheap (a full-frame crop costs the same as the sweep).
    if (band.y1 - band.y0 + 1 > height * 0.5) continue;
    // Bucket the transition positions per 8px cell; group cells separated
    // by more than four cells (32px) and keep the densest group. That is
    // the symbol, while the hand/box edges are long runs with few flips.
    /** @type {Map<number, number>} */
    const cells = new Map();
    for (let y = band.y0; y <= band.y1; y++) {
      let prev = 0;
      for (let x = 0; x < width; x++) {
        const bit = (data[y * bytesPerRow + (x >> 3)] ?? 0) >> (x & 7) & 1;
        if (bit !== prev) {
          const cell = x >> 3;
          cells.set(cell, (cells.get(cell) ?? 0) + 1);
        }
        prev = bit;
      }
    }
    if (!cells.size) continue;
    const keys = [...cells.keys()].sort((a, b) => a - b);
    /** @type {number[][]} */
    const groups = [];
    for (const key of keys) {
      const group = groups.at(-1);
      if (group && key <= (group[group.length - 1] ?? key) + 4) group.push(key);
      else groups.push([key]);
    }
    /** @type {{ total: number, x0: number, x1: number } | null} */
    let best = null;
    for (const group of groups) {
      const total = group.reduce((sum, key) => sum + (cells.get(key) ?? 0), 0);
      if (!best || total > best.total) {
        best = {
          total,
          x0: (group[0] ?? 0) * 8,
          x1: ((group[group.length - 1] ?? 0) + 1) * 8,
        };
      }
    }
    if (!best) continue;
    // A 1D symbol is short: the transition-dense rows are its middle. Pad
    // vertically enough to keep the full bar height (and the quiet zone).
    const padX = Math.max(8, Math.round((best.x1 - best.x0) * 0.12));
    const padY = Math.max(8, Math.round((band.y1 - band.y0 + 1) * 0.6));
    const x = Math.max(0, Math.round((best.x0 - padX) * sx));
    const y = Math.max(0, Math.round((band.y0 - padY) * sy));
    const w = Math.min(sourceW - x, Math.round((best.x1 - best.x0 + 2 * padX) * sx));
    const h = Math.min(sourceH - y, Math.round((band.y1 - band.y0 + 1 + 2 * padY) * sy));
    if (w < MIN_SIDE || h < MIN_SIDE) continue;
    boxes.push({
      x, y, w, h,
      score: best.total * centerWeight(x, y, w, h, sourceW, sourceH),
    });
  }

  boxes.sort((a, b) => b.score - a.score);
  /** @type {RoiBox[]} */
  const picked = [];
  for (const box of boxes) {
    const overlap = picked.some((p) => {
      const ix = Math.max(0, Math.min(box.x + box.w, p.x + p.w) - Math.max(box.x, p.x));
      const iy = Math.max(0, Math.min(box.y + box.h, p.y + p.h) - Math.max(box.y, p.y));
      return ix * iy > 0.6 * Math.min(box.w * box.h, p.w * p.h);
    });
    if (!overlap) picked.push(box);
    if (picked.length >= limit) break;
  }
  // Tight boxes first (the upside-down / marginal-1D lever), then the
  // looser proposals: a symbol the dense-row split misses (partly cut
  // off, noisy mask, white-on-black island) is still found by the latter,
  // whose Otsu retry is the distant-1D lever. A generous old box can
  // overlap a tight one and still be the only readable crop (the inverted
  // island needs its own quiet-zone margin), so only dedupe within each
  // family; the caller's deadline bounds the extra crops.
  return [...tight, ...picked].slice(0, limit + 2);
}

/**
 * Global-histogram (Otsu) binarization of a small crop, upscaled to give
 * the 1D decoder back the module pitch it lost in the full-frame
 * downscale. The local Bradley pass is best on a whole scene, but on a
 * tightly cropped bar strip the blur makes every space a little darker
 * than its neighbourhood — a global two-class split keeps those spaces
 * white where `threshold * local mean` floods them. Runs in the worker;
 * the crop is small.
 *
 * @param {HTMLCanvasElement | OffscreenCanvas} source
 * @param {number} [scale] Upscale factor (capped so no side exceeds 1600px).
 * @returns {BinarizedFrame & { threshold: number }}
 */
export function otsuMask(source, scale = 2) {
  const sw = source.width;
  const sh = source.height;
  const factor = Math.max(1, Math.min(scale, 1600 / Math.max(sw, sh)));
  const w = Math.max(1, Math.round(sw * factor));
  const h = Math.max(1, Math.round(sh * factor));
  /** @type {HTMLCanvasElement | OffscreenCanvas} */
  const canvas =
    typeof OffscreenCanvas !== "undefined"
      ? new OffscreenCanvas(w, h)
      : Object.assign(document.createElement("canvas"), { width: w, height: h });
  const ctx = /** @type {CanvasRenderingContext2D | null} */ (canvas.getContext("2d", { willReadFrequently: true }));
  if (!ctx) throw new TypeError("otsuMask: no 2d context");
  ctx.imageSmoothingEnabled = factor !== 1;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(/** @type {CanvasImageSource} */ (source), 0, 0, w, h);
  const px = ctx.getImageData(0, 0, w, h).data;
  const luma = new Uint8Array(w * h);
  const hist = new Uint32Array(256);
  for (let i = 0; i < w * h; i++) {
    const l = (0.299 * (px[i * 4] ?? 0) + 0.587 * (px[i * 4 + 1] ?? 0) + 0.114 * (px[i * 4 + 2] ?? 0)) | 0;
    luma[i] = l;
    hist[l] = (hist[l] ?? 0) + 1;
  }
  const total = w * h;
  let sum = 0;
  for (let t = 0; t < 256; t++) sum += t * (hist[t] ?? 0);
  let weight = 0;
  let sumB = 0;
  let threshold = 128;
  let best = -1;
  for (let t = 0; t < 256; t++) {
    weight += hist[t] ?? 0;
    if (!weight) continue;
    const weightF = total - weight;
    if (!weightF) break;
    sumB += t * (hist[t] ?? 0);
    const meanB = sumB / weight;
    const meanF = (sum - sumB) / weightF;
    const variance = weight * weightF * (meanB - meanF) ** 2;
    if (variance > best) {
      best = variance;
      threshold = t;
    }
  }
  const bytesPerRow = Math.ceil(w / 8 / 4) * 4;
  const data = new Uint8Array(bytesPerRow * h);
  for (let y = 0; y < h; y++) {
    const row = y * bytesPerRow;
    for (let x = 0; x < w; x++) {
      if ((luma[y * w + x] ?? 0) < threshold) {
        const byte = row + (x >> 3);
        data[byte] = (data[byte] ?? 0) | (1 << (x & 7));
      }
    }
  }
  return { width: w, height: h, bytesPerRow, data, threshold };
}

/** @param {CanvasImageSource | HTMLVideoElement} source */
const dims = (source) => {
  const box = /** @type {{ width?: number, height?: number, videoWidth?: number, videoHeight?: number }} */ (
    /** @type {unknown} */ (source)
  );
  return {
    w: Number(box.width ?? box.videoWidth ?? 0),
    h: Number(box.height ?? box.videoHeight ?? 0),
  };
};

/**
 * Crop a source drawable at native resolution. The returned `map` takes
 * points decoded from the binarized crop and puts them back into source
 * pixel coordinates (the binarizer pads a fitting crop 1:1 and only
 * downscales one larger than maxSize).
 *
 * @param {CanvasImageSource | HTMLVideoElement} source
 * @param {RoiBox} box
 * @returns {Promise<{
 *   canvas: OffscreenCanvas | HTMLCanvasElement,
 *   width: number,
 *   height: number,
 *   x: number,
 *   y: number,
 *   map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[],
 * }>}
 */
export async function cropDrawable(source, box) {
  const { w: sw, h: sh } = dims(source);
  if (!sw || !sh) throw new TypeError("cropDrawable: source has no dimensions");
  const x = Math.max(0, Math.min(sw - 1, Math.round(box.x)));
  const y = Math.max(0, Math.min(sh - 1, Math.round(box.y)));
  const w = Math.max(1, Math.min(sw - x, Math.round(box.w)));
  const h = Math.max(1, Math.min(sh - y, Math.round(box.h)));
  /** @type {OffscreenCanvas | HTMLCanvasElement} */
  const canvas =
    typeof OffscreenCanvas !== "undefined"
      ? new OffscreenCanvas(w, h)
      : Object.assign(document.createElement("canvas"), { width: w, height: h });
  const ctx = /** @type {CanvasRenderingContext2D | null} */ (canvas.getContext("2d"));
  if (!ctx) throw new TypeError("cropDrawable: no 2d context");
  ctx.drawImage(/** @type {CanvasImageSource} */ (source), x, y, w, h, 0, 0, w, h);
  return {
    canvas,
    width: w,
    height: h,
    x,
    y,
    map: (fw, fh, points) => {
      const sx = w <= fw ? 1 : w / fw;
      const sy = h <= fh ? 1 : h / fh;
      return points.map(({ x: px, y: py }) => ({ x: x + px * sx, y: y + py * sy }));
    },
  };
}

/**
 * Horizontal mirror of a crop, at source resolution. A 180°-presented
 * 1D symbol is the common phone case (the label is upside down), and the
 * mask-level mirror in wasm is starved when the binarization itself is
 * marginal: the same bar strip re-binarized from a mirrored SOURCE reads.
 * Mirrors the pixels (no mask round-trip), so the decoder gets a clean
 * upright run sequence.
 *
 * @param {CanvasImageSource} source
 * @returns {Promise<{
 *   canvas: OffscreenCanvas | HTMLCanvasElement,
 *   width: number,
 *   height: number,
 *   map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[],
 * }>}
 */
export async function mirrorDrawable(source) {
  const { w, h } = dims(source);
  if (!w || !h) throw new TypeError("mirrorDrawable: source has no dimensions");
  /** @type {OffscreenCanvas | HTMLCanvasElement} */
  const canvas =
    typeof OffscreenCanvas !== "undefined"
      ? new OffscreenCanvas(w, h)
      : Object.assign(document.createElement("canvas"), { width: w, height: h });
  const ctx = /** @type {CanvasRenderingContext2D | null} */ (canvas.getContext("2d"));
  if (!ctx) throw new TypeError("mirrorDrawable: no 2d context");
  ctx.translate(w, 0);
  ctx.scale(-1, 1);
  ctx.drawImage(/** @type {CanvasImageSource} */ (source), 0, 0);
  return {
    canvas,
    width: w,
    height: h,
    map: (fw, fh, points) => {
      const sx = w <= fw ? 1 : w / fw;
      const sy = h <= fh ? 1 : h / fh;
      return points.map(({ x, y }) => ({ x: w - x * sx, y: y * sy }));
    },
  };
}

/**
 * White-pad a crop at source resolution. A tight bar strip can cut the
 * quiet zone Code 128 mandates (10 modules) down to a couple of modules:
 * a real DHL/GoFo label measures ~6 modules on one side and the row
 * scanners reject the otherwise clean strip, while zxing — lenient about
 * quiet zones — reads the very same photo. Drawing the strip onto paper
 * white restores the zone (and costs no extra camera frame). The
 * returned `map` takes points from the padded frame back into the
 * *source crop's* pixel coordinates, so the caller composes it with
 * `cropDrawable`'s map as before.
 *
 * @param {CanvasImageSource} source
 * @param {number} [pad] White margin on every side, in source pixels.
 * @returns {Promise<{
 *   canvas: OffscreenCanvas | HTMLCanvasElement,
 *   width: number,
 *   height: number,
 *   map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[],
 * }>}
 */
export async function padDrawable(source, pad = 24) {
  const { w, h } = dims(source);
  if (!w || !h) throw new TypeError("padDrawable: source has no dimensions");
  const pw = w + 2 * pad;
  const ph = h + 2 * pad;
  /** @type {OffscreenCanvas | HTMLCanvasElement} */
  const canvas =
    typeof OffscreenCanvas !== "undefined"
      ? new OffscreenCanvas(pw, ph)
      : Object.assign(document.createElement("canvas"), { width: pw, height: ph });
  const ctx = /** @type {CanvasRenderingContext2D | null} */ (canvas.getContext("2d"));
  if (!ctx) throw new TypeError("padDrawable: no 2d context");
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, pw, ph);
  ctx.drawImage(/** @type {CanvasImageSource} */ (source), pad, pad);
  return {
    canvas,
    width: pw,
    height: ph,
    map: (fw, fh, points) => {
      const sx = pw <= fw ? 1 : pw / fw;
      const sy = ph <= fh ? 1 : ph / fh;
      return points.map(({ x, y }) => ({ x: x * sx - pad, y: y * sy - pad }));
    },
  };
}
