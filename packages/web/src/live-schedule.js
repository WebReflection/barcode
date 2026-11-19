/**
 * Live-frame scheduling. The exotic 2D families (DataMatrix, PDF417)
 * measured ~250 ms each on a phone — 320/390 ms on a 920px frame — and
 * are what makes "scan everything" feel slow while the user is still
 * aiming. Run them at most every `FULL_INTERVAL_MS` of wall time; every
 * other frame keeps the cheap rotation-invariant families (QR, Micro QR,
 * Aztec, DataBar stacked).
 *
 * A scheduled FULL frame itself runs cheap-first (worker.js): the cheap
 * tier decodes first and only a miss pays for the exotic remainder, so
 * QR/1D hits confirm on cheap frames alone. When cheap frames are fast
 * relative to the interval, several run between full sweeps; when a full
 * sweep costs more than the interval (slow phones), every frame runs
 * escalated and `markFullSweep` stretches the spacing automatically.
 *
 * The linear tier only runs when the last full sweep saw no 2D code —
 * that preserves the "2D before 1D" false-positive rule (a QR's texture
 * goads loose 1D symbologies into hits; 1D must only vote on frames
 * where the whole 2D tier already missed).
 *
 * Still images never alternate: one shot, one full sweep.
 */

/** Minimum wall-clock spacing between full exotic sweeps (live only). */
const FULL_INTERVAL_MS = 250;

/** Families cheap enough for every live frame (measured on a phone). */
export const CHEAP_2D = ['qrcode', 'microqr', 'aztec', 'databarstk'];

/** Every supported family, in the wasm's canonical order. */
export const ALL_FORMATS = [
  'qrcode', 'microqr', 'upca', 'datamatrix', 'aztec', 'pdf417', 'ean13',
  'ean8', 'upce', 'code39', 'code32', 'code93', 'code128', 'itf', 'codabar',
  'databar', 'databarltd', 'databarstk', 'databarexpanded',
];

const EXPENSIVE = new Set(['datamatrix', 'pdf417']);

/** The linear (1D) tier families. */
export const LINEAR = [
  'upca', 'ean13', 'ean8', 'upce', 'code39', 'code32', 'code93', 'code128',
  'itf', 'codabar', 'databar', 'databarltd', 'databarexpanded',
];

/** Families decoded by the 2D tier (DataBar Stacked is a stacked tier). */
const TWO_D = new Set(['qrcode', 'microqr', 'datamatrix', 'aztec', 'pdf417', 'databarstk']);

/**
 * True when a scan result came from the 2D tier — the evidence that
 * gates the linear tier (a 2D code's texture false-positives 1D).
 * Results carry the canonical lowercase names already (`Symbology::name`
 * in the wasm), so this is a plain set lookup.
 * @param {string} symbology
 * @returns {boolean}
 */
export const is2DResult = (symbology) => TWO_D.has(symbology);

/**
 * True when the requested formats include an exotic family (or no filter
 * at all, i.e. everything).
 * @param {string[] | undefined} formats
 * @returns {boolean}
 */
export const needsScheduling = (formats) => {
  if (!formats?.length) return true; // everything
  const names = formats.map((f) => f.trim().toLowerCase());
  const exotic = names.some((n) => EXPENSIVE.has(n));
  // A request for ONLY exotic families has nothing cheap to interleave:
  // decode every frame (a DataMatrix-only scan must be as fast as the
  // camera allows).
  return exotic && names.some((n) => !EXPENSIVE.has(n));
};

/**
 * The formats to run on a cheap live frame: the requested cheap 2D
 * families, plus the linear tier when the last full sweep found no 2D
 * code (the "2D before 1D" false-positive gate, at most one interval
 * stale). An empty list means the frame can skip decoding entirely.
 * @param {string[] | undefined} formats
 * @param {boolean} allowLinear
 * @returns {string[]}
 */
export const cheapFormats = (formats, allowLinear = false) =>
  (formats?.length ? formats : ALL_FORMATS).filter((f) => {
    const name = f.trim().toLowerCase();
    return CHEAP_2D.includes(name) || (allowLinear && LINEAR.includes(name));
  });

/** Wall time of the last completed full sweep (per worker module). */
let lastFullAt = -Infinity;

/**
 * True when this live frame should run the exotic families. Call
 * {@link markFullSweep} after the sweep completes (the sweep itself can
 * take longer than the interval, so the clock starts at its end).
 * @param {number} now
 * @returns {boolean}
 */
export const dueForFullSweep = (now) => now - lastFullAt >= FULL_INTERVAL_MS;

/** Record a completed full sweep. @param {number} now */
export const markFullSweep = (now) => {
  lastFullAt = now;
};
