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

/** @typedef {import("./gpu/binarizer.js").BinarizedFrame} BinarizedFrame */

/**
 * A candidate box in the frame's SOURCE pixel coordinates (not mask
 * coordinates).
 * @typedef {{ x: number, y: number, w: number, h: number, score: number }} RoiBox
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
    // 200px QR is not buried by a 24px speck of print noise.
    const score = density * (0.55 + 0.45 * coverage) * (1 + Math.log10(1 + bw * bh) / 8);
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
