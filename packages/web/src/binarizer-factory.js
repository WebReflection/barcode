/**
 * Binarizer factory: GPU when available, CPU fallback otherwise. Both
 * implement async process(video) -> BinarizedFrame with the same mask
 * contract, so the scanner and decoders are agnostic.
 */
import { GpuBinarizer } from "./gpu/binarizer.js";
import { CpuBinarizer } from "./cpu-binarizer.js";

/** @typedef {import("./gpu/binarizer.js").BinarizerOptions} BinarizerOptions */

/**
 * @param {BinarizerOptions} [opts]
 * @returns {Promise<{ process(video: HTMLVideoElement | HTMLCanvasElement | ImageBitmap | OffscreenCanvas): Promise<import("./gpu/binarizer.js").BinarizedFrame>, destroy(): void, kind: "gpu" | "cpu", setThreshold(t: number): void }>}
 */
export async function createBinarizer(opts = {}) {
  const full = {
    maxSize: opts.maxSize ?? 1056,
    windowDivisor: opts.windowDivisor ?? 20,
    threshold: opts.threshold ?? 0.92,
  };
  if ("gpu" in navigator && navigator.gpu) {
    try {
      const gpu = await GpuBinarizer.create(full);
      return { process: (v) => gpu.process(v), destroy: () => gpu.destroy(), kind: "gpu", setThreshold: (t) => gpu.setThreshold(t) };
    } catch (err) {
      // With requireGpu the failure must be loud — a silent CPU fallback
      // makes "which path am I testing?" unanswerable (and the CPU path is
      // 10-20x slower, which reads as jank).
      if (opts.requireGpu) throw err;
      console.warn("GPU binarizer unavailable, falling back to CPU:", err);
    }
  } else if (opts.requireGpu) {
    throw new Error("GPU binarizer required but WebGPU is unavailable in this context.");
  }
  const cpu = new CpuBinarizer(full);
  return { process: (v) => cpu.process(v), destroy: () => cpu.destroy(), kind: "cpu", setThreshold: (t) => cpu.setThreshold(t) };
}
