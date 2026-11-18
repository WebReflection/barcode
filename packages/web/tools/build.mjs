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

/** @param {string} file @param {string[]} args */
const run = (file, args, opts = {}) => {
  console.log(`\n$ ${[file, ...args].join(" ")}`);
  execFileSync(file, args, { cwd: root, stdio: "inherit", env: { ...process.env, PATH }, ...opts });
};

const tsc = resolve(root, "node_modules/typescript/bin/tsc");
if (!existsSync(tsc)) {
  console.error("missing node_modules — run `npm install` (or `pnpm install`) first");
  process.exit(2);
}

run(process.execPath, [tsc, "--noEmit"]);
run(process.execPath, [tsc, "-p", "tsconfig.types.json"]);

// wasm-pack is a native binary; PATH above includes ~/.cargo/bin.
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

if (DEMO) {
  const vite = resolve(root, "node_modules/vite/bin/vite.js");
  run(process.execPath, [vite, "build", "--config", "vite.scan.config.js"]);
}

console.log(`\ndone: ${DEMO ? "dist/ (npm payload) + demo-dist/ (deployable demo)" : "dist/ (npm payload)"}`);
