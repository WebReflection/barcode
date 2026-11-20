#!/usr/bin/env node
/**
 * One command, full build:
 *
 *   node tools/build.mjs            # types check + d.ts + wasm + dist/
 *   node tools/build.mjs --demo     # ... and the deployable demo
 *
 * The published npm payload is `dist/` (see tools/build-artifacts.mjs);
 * this wrapper exists so the repo never needs a hand-written sequence of
 * tsc / wasm-pack / esbuild invocations in CI or in a fresh checkout.
 *
 * Prerequisites: Node deps (`npm install` or `pnpm install`) and a Rust
 * toolchain with the wasm32 target + wasm-pack:
 *   rustup target add wasm32-unknown-unknown
 *   cargo install wasm-pack
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const DEMO = process.argv.includes("--demo");

const PATH = [
  resolve(homedir(), ".cargo/bin"),
  resolve(homedir(), ".rustup/toolchains/stable-aarch64-apple-darwin/bin"),
  resolve(homedir(), ".rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"),
  process.env.PATH,
].filter(Boolean).join(delimiter);

/** @param {string} file @param {string[]} args @param {{ env?: Record<string, string | undefined> }} [opts] */
const run = (file, args, opts = {}) => {
  console.log(`\n$ ${[file, ...args].join(" ")}`);
  execFileSync(file, args, {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, PATH, ...opts.env },
  });
};

const tsc = resolve(root, "node_modules/typescript/bin/tsc");
if (!existsSync(tsc)) {
  console.error("missing node_modules — run `npm install` (or `pnpm install`) first");
  process.exit(2);
}

// Generate the modules the type check imports FIRST. On a clean checkout
// `src/wasm/pkg` and `dist/` do not exist yet; tsc must not run before
// them or CI fails with "cannot find module".
//
// wasm-pack is a native binary; PATH above includes ~/.cargo/bin.
//
// NOTE: SIMD is opted in PER FUNCTION with `#[target_feature(enable =
// "simd128")]`, not with a global `+simd128` target feature. The global
// flag also re-vectorizes the branchy run scanners, and that measured
// SLOWER end-to-end (EAN/UPC/Code39/Code93/Codabar each lost 20-25ms on
// the phone while only Code 128 gained). Function-level opt-in keeps the
// win without the regressions:
// - barcode-core mirrors/180°-rotates packed rows 16 bytes at a time
//   (bit reversal + byte shuffle) instead of per pixel;
// - barcode-code128 runs the fuzzy closest-pattern search four patterns
//   at a time with f32x4 (the 1D tier's dominant junk-frame cost);
// - barcode-wasm expands the DataMatrix 2x fallback mask eight source
//   bytes at a time (bit spread in i16x8 lanes).
// Same-page interleaved A/B on an arm64 phone (ASUS AI2302, Android 15),
// 576x1056 noisy live mask, alternating call order, min of 11 runs:
// full 17-symbology sweep 1204 -> 1125 ms (-6.6%), all-1D tier 604 -> 525
// ms (-13%), Code 128 alone 156 -> 76 ms (-51%); every other decoder and
// PDF417/DataMatrix/QR stay within run-to-run noise. Their hot loops are
// branchy run scanners, so autovectorization/global SIMD does not help.
// Isolated per-load timings are noisy enough to fake a 15-20% delta.
// Native builds keep the portable paths; the browser self-test gates cover
// the wasm SIMD build on every CI run.
try {
  run("wasm-pack", [
    "build", "../../crates/barcode-wasm",
    "--target", "web",
    "--out-dir", "../../packages/web/src/wasm/pkg",
  ]);
} catch (err) {
  if (/** @type {NodeJS.ErrnoException} */ (err).code === "ENOENT") {
    console.error("\nwasm-pack not found — `cargo install wasm-pack` (and rustup target add wasm32-unknown-unknown)");
    process.exit(2);
  }
  throw err;
}

run(process.execPath, ["tools/build-artifacts.mjs"]);

run(process.execPath, [tsc, "--noEmit"]);
run(process.execPath, [tsc, "-p", "tsconfig.types.json"]);

// Publish coupling: prepublishOnly runs this script, and the Pages site
// must be the exact version + demo bundle npm just received. Feature PRs
// never commit generated docs/ (the docs guard enforces that), so this
// only writes the working tree; a maintainer ships a docs-rebuild PR.
run(process.execPath, ["tools/build-docs.mjs"]);

if (DEMO) {
  const vite = resolve(root, "node_modules/vite/bin/vite.js");
  run(process.execPath, [vite, "build", "--config", "vite.scan.config.js"]);
}

console.log(`\ndone: ${DEMO ? "dist/ (npm payload) + demo-dist/ (deployable demo)" : "dist/ (npm payload)"} + docs/`);
