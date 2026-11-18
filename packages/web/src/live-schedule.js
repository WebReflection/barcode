/**
 * Live-frame scheduling. The exotic 2D families (DataMatrix, PDF417)
 * measured ~250 ms each on a phone and are what makes "scan everything"
 * feel slow while the user is still aiming. Run them at most every
 * `FULL_INTERVAL_MS` of wall time; every other frame keeps the cheap
 * rotation-invariant families (QR, Micro QR, Aztec, DataBar stacked).
 *
 * Wall time, not frame parity: a fresh scan (or a one-shot worker frame)
 * starts with a full sweep because the previous one is long past, while a
 * continuous hunt interleaves cheap frames between full sweeps.
 *
 * The linear tier only runs on full frames — that preserves the "2D
 * before 1D" false-positive rule (a QR's texture goads loose 1D
 * symbologies into hits; 1D must only vote on frames where the whole 2D
 * tier already missed).
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
  'ean8', 'upce', 'code39', 'code93', 'code128', 'itf', 'codabar',
  'databar', 'databarltd', 'databarstk',
];

const EXPENSIVE = new Set(['datamatrix', 'pdf417']);

/** The linear (1D) tier families. */
export const LINEAR = [
  'upca', 'ean13', 'ean8', 'upce', 'code39', 'code93', 'code128', 'itf',
  'codabar', 'databar', 'databarltd',
];

/** Families decoded by the 2D tier (DataBar Stacked is a stacked tier). */
const TWO_D = new Set(['qrcode', 'microqr', 'datamatrix', 'aztec', 'pdf417', 'databarstk']);

/** Normalize a decode result's title-cased symbology. @param {string} name */
const norm = (name) => name.toLowerCase().replace(/[^a-z0-9]/g, '');

/**
 * True when a scan result came from the 2D tier — the evidence that
 * gates the linear tier (a 2D code's texture false-positives 1D).
 * @param {string} symbology
 * @returns {boolean}
 */
export const is2DResult = (symbology) => TWO_D.has(norm(symbology));

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
