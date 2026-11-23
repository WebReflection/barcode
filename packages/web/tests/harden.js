/**
 * Synthetic hardening sweep (replaces the propped-phone rig for day-to-day
 * validation): every specimen is re-drawn onto a scene at four sizes
 * (module px = simulated distance), with gaussian blur (defocus), uniform
 * luma noise (sensor), rotation (handheld tilt), and two exposure levels
 * (normal / dim) — then binarized and decoded through the REAL wasm
 * pipeline at three Bradley thresholds.
 *
 * Reports, per symbology and degradation cell:
 *   single — read rate at the primary threshold 0.92 (balanced mode)
 *   any    — read rate with 0.80/0.92/1.04 tried (accuracy-high mechanism)
 *
 * Driven headless by tools/harden.mjs (polls __HARDEN_DONE/__HARDEN), or
 * open by hand for the table view. Deterministic: seeded noise, fixed grid.
 */

import { decodeFrame, getWasmScanner } from "../src/decoder.js";
import { createBinarizer } from "../src/binarizer-factory.js";
import { encode_fixture_opts } from "../src/wasm/pkg/barcode_wasm.js";

const CASES = [
  { s: "ean13", t: "4006381333931" },
  { s: "ean8", t: "96385074" },
  { s: "upca", t: "036000291452" },
  { s: "upce", t: "042100005264" },
  { s: "code39", t: "ABC-1234" },
  { s: "code93", t: "TEST93" },
  { s: "code128", t: "INV-2026-0042" },
  { s: "itf", t: "12345670" },
  { s: "codabar", t: "40156" },
  { s: "databar", t: "4006381333931", expect: "(01)40063813339314" },
  { s: "databarltd", t: "1234567890123", expect: "(01)12345678901231" },
  { s: "databarstk", t: "4006381333931", expect: "(01)40063813339314" },
  { s: "databarstackomni", t: "4006381333931", expect: "(01)40063813339314" },
  { s: "qrcode", t: "HELLO WORLD" },
  { s: "qrcode", t: "https://example.com/very/long/path/that/needs/version/seven/or/higher/qr/symbols/to/fit/comfortably?with=query&params=1" },
];

// Specimens are rendered at MODULE_BASE px/module, then rescaled on the
// scene — the rescale is the optics model (box-ish AA like a camera).
const MODULE_BASE = 4;
const MODULES = [4, 3, 2, 1];
const BLURS = [0, 0.6, 1.2];
const NOISES = [0, 10];
const ROTS = [0, 6];
// Scene exposure axis: 1 = the original mid-gray paper, 0.4 = a dim capture
// (paper ~0.34, print ~0.04). Global auto-levels must lift the dim scenes
// without touching the normal ones (the gate reads p2/p98 of the frame).
const LIGHTS = [1, 0.4];
const THRESHOLDS = [0.80, 0.92, 1.04];
const SEEDS = 3;
const SCENE = 512; // multiple of 32 (binarizer contract)
const PRIMARY = 0.92;

/** @param {number} a */
function mulberry32(a) {
  return () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The wasm serde boundary is untyped; this is its actual shape. */
/** @typedef {{ width: number, height: number, bytesPerRow: number, data: Uint8Array | number[] }} Fixture */

/** Paint the 1bpp fixture mask as realistic print luma (like test.js). */
/** @param {Fixture} enc */
function maskToCanvas(enc) {
  const img = new ImageData(enc.width, enc.height);
  const px = img.data;
  for (let y = 0; y < enc.height; y++) {
    const row = y * enc.bytesPerRow;
    let o = y * enc.width * 4;
    for (let x = 0; x < enc.width; x++, o += 4) {
      const v = ((enc.data[row + (x >> 3)] ?? 0) >> (x & 7)) & 1 ? 25 : 235;
      px[o] = px[o + 1] = px[o + 2] = v;
      px[o + 3] = 255;
    }
  }
  const c = document.createElement("canvas");
  c.width = enc.width;
  c.height = enc.height;
  /** @type {CanvasRenderingContext2D} */ (c.getContext("2d")).putImageData(img, 0, 0);
  return c;
}

/**
 * Compose the degraded scene: mid-gray bg, specimen centered at
 * `module` px/module, rotated, optionally blurred, plus seeded noise,
 * then scaled by `light` (1 = normal exposure, 0.4 = dim capture).
 * @param {HTMLCanvasElement} spec
 */
function makeScene(spec, /** @type {number} */ module, /** @type {number} */ blur, /** @type {number} */ noise, /** @type {number} */ rot, /** @type {number} */ seed, /** @type {number} */ light = 1) {
  const c = document.createElement("canvas");
  c.width = SCENE;
  c.height = SCENE;
  const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext("2d", { willReadFrequently: true }));
  ctx.fillStyle = "#d7d7d7";
  ctx.fillRect(0, 0, SCENE, SCENE);
  const scale = Math.min(
    module / MODULE_BASE,
    (SCENE * 0.9) / spec.width, // never clip wide fixtures (code128 m4)
    (SCENE * 0.8) / spec.height,
  );
  const w = spec.width * scale;
  const h = spec.height * scale;
  ctx.save();
  ctx.translate(SCENE / 2, SCENE / 2);
  ctx.rotate((rot * Math.PI) / 180);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  if (blur) ctx.filter = `blur(${blur}px)`;
  ctx.drawImage(spec, -w / 2, -h / 2, w, h);
  ctx.restore();
  if (noise) {
    const img = ctx.getImageData(0, 0, SCENE, SCENE);
    const d = /** @type {Uint8ClampedArray} */ (img.data);
    const rnd = mulberry32(seed);
    for (let i = 0; i < d.length; i += 4) {
      const n = (rnd() * 2 - 1) * noise;
      for (const o of [0, 1, 2]) {
        d[i + o] = /** @type {number} */ (d[i + o]) + n;
      }
    }
    ctx.putImageData(img, 0, 0);
  }
  if (light !== 1) {
    const img = ctx.getImageData(0, 0, SCENE, SCENE);
    const d = /** @type {Uint8ClampedArray} */ (img.data);
    for (let i = 0; i < d.length; i += 4) {
      d[i] = Number(d[i]) * light;
      d[i + 1] = Number(d[i + 1]) * light;
      d[i + 2] = Number(d[i + 2]) * light;
    }
    ctx.putImageData(img, 0, 0);
  }
  return c;
}

async function run() {
  const quick = new URLSearchParams(location.search).has("quick");
  const modules = quick ? [3, 2] : MODULES;
  const blurs = quick ? [0, 1.2] : BLURS;
  const noises = quick ? [0] : NOISES;
  const rots = quick ? [0] : ROTS;
  const lights = LIGHTS;
  const seeds = quick ? 1 : SEEDS;

  const bin = await createBinarizer({ maxSize: SCENE });
  const scanner = await getWasmScanner();

  /** @type {any[]} */
  const out = [];
  for (const [ci, c] of CASES.entries()) {
    const t0 = performance.now();
    const expect = c.expect ?? c.t;
    const spec = maskToCanvas(encode_fixture_opts(c.s, c.t, MODULE_BASE, 400, -1, -1, 0, -1, 0, 0, 0));
    /** @type {Record<string, {single: number, any: number, of: number}>} */
    const cells = {};
    let singleHits = 0, anyHits = 0, total = 0;
    for (const m of modules) {
      for (const b of blurs) {
        for (const n of noises) {
          for (const r of rots) {
            for (const light of lights) {
              // noise 0 => identical scene for every seed; decode it once.
              const nSeeds = n === 0 ? 1 : seeds;
              let single = 0, any = 0;
              for (let seed = 0; seed < nSeeds; seed++) {
                const scene = makeScene(spec, m, b, n, r, ((ci * 31 + seed * 7919) | 0) + m * 101 + b * 7 + n * 13 + r, light);
                // Primary first, early exit — mirrors the accuracy-high
                // pipeline (sweep alternates only to RESCUE a primary miss).
                // Formats filtered to the case's symbology, as prod pages do.
                bin.setThreshold(PRIMARY);
                const frame0 = await bin.process(scene);
                let hitAny = decodeFrame(scanner, frame0, [c.s]).some((x) => x.text === expect);
                if (hitAny) {
                  single++;
                } else {
                  for (const th of THRESHOLDS) {
                    if (th === PRIMARY) continue;
                    bin.setThreshold(th);
                    const frame = await bin.process(scene);
                    if (decodeFrame(scanner, frame, [c.s]).some((x) => x.text === expect)) { hitAny = true; break; }
                  }
                }
                if (hitAny) any++;
              }
              cells[`m${m} b${b} n${n} r${r} l${light}`] = { single, any, of: nSeeds };
              singleHits += single;
              anyHits += any;
              total += nSeeds;
            }
          }
        }
      }
    }
    out.push({
      s: c.s, t: c.t, expect,
      single: { hits: singleHits, of: total },
      any: { hits: anyHits, of: total },
      cells,
    });
    const outEl = /** @type {HTMLElement} */ (document.getElementById("out"));
  outEl.textContent +=
      `\n${c.s.padEnd(8)} single ${(100 * singleHits / total).toFixed(0).padStart(3)}%  any ${(100 * anyHits / total).toFixed(0).padStart(3)}%  (${Math.round(performance.now() - t0)}ms)`;
  }

  const result = {
    meta: {
      when: new Date().toISOString(),
      binarizer: bin.kind,
      grid: { modules, blurs, noises, rots, lights, thresholds: THRESHOLDS, seeds },
      scene: SCENE,
      primaryThreshold: PRIMARY,
    },
    cases: out,
  };
  (/** @type {Record<string, unknown>} */ (/** @type {unknown} */ (window))).__HARDEN = result;
  (/** @type {Record<string, unknown>} */ (/** @type {unknown} */ (window))).__HARDEN_DONE = true;
  /** @type {HTMLElement} */ (document.getElementById("out")).textContent += "\n\nDONE";
}

try {
  await run();
} catch (err) {
  const hardenError = String(err instanceof Error ? err.stack : err);
  (/** @type {Record<string, unknown>} */ (/** @type {unknown} */ (window))).__HARDEN_ERROR = hardenError;
  /** @type {HTMLElement} */ (document.getElementById("out")).textContent +=
    `\nERROR ${hardenError}`;
}
