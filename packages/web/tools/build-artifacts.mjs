/**
 * Build the published `dist/` payload (package.json `files: ["dist"]`):
 *
 *   main.js      — ESM, the consumer SDK, page entry (scan/render/camera/dialog)
 *   worker.js    — ESM, the bound worker (binarizer + wasm decode/encode)
 *   wasm.wasm    — the wasm binary (resolved next to worker.js)
 *   css.css      — the optional default-UI stylesheet
 *   *.d.ts       — the consumer-facing types (api/*.d.ts, hand-authored)
 *   README.md    — API summary
 *   LICENSE      — MIT
 *
 * The JS artifacts are single self-contained files: no shared chunks, no
 * emitted assets — a bundler emitting a fourth file would break the
 * publish/CDN layout (import maps / unpacked localhost). The scanner's
 * pipeline worker is therefore never referenced from the artifact: main.js
 * injects decode() through the bound worker instead (see src/scanner.js).
 *
 * Run from packages/web:  node tools/build-artifacts.mjs
 */
import { createRequire } from "node:module";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = resolve(root, "dist");
const require = createRequire(import.meta.url);
// esbuild ships inside vite's dependency tree — under pnpm it lives in
// .pnpm; resolve it wherever it is (no direct devDependency needed).
function findEsbuild() {
  for (const base of [root, resolve(root, "node_modules/vite")]) {
    try {
      return require(require.resolve("esbuild/lib/main.js", { paths: [base] }));
    } catch (_) { /* try next */ }
  }
  const { readdirSync } = require("node:fs");
  const pnpm = resolve(root, "node_modules/.pnpm");
  const dir = readdirSync(pnpm).find((d) => d.startsWith("esbuild@"));
  if (dir) return require(resolve(pnpm, dir, "node_modules/esbuild"));
  throw new Error("esbuild not found — install vite");
}
const esbuild = findEsbuild();


/** `*?url` → sibling artifact URL (worker.js fetches ./wasm.wasm). */
const wasmUrl = {
  name: "wasm-url",
  setup(build) {
    build.onResolve({ filter: /\.wasm\?url$/ }, (args) => ({
      path: args.path,
      namespace: "wasm-url",
    }));
    build.onLoad({ filter: /.*/, namespace: "wasm-url" }, () => ({
      contents: `export default new URL('wasm.wasm', import.meta.url).href;`,
      loader: "js",
    }));
  },
};

/** `*?raw` (WGSL shaders) → inlined string, comment-stripped: the shader
 * sources are documentation-grade in src/ but must not bloat worker.js. */
const raw = {
  name: "raw",
  setup(build) {
    build.onResolve({ filter: /\?raw$/ }, (args) => ({
      path: resolve(dirname(args.importer), args.path),
      namespace: "raw",
    }));
    build.onLoad({ filter: /.*/, namespace: "raw" }, (args) => {
      const src = readFileSync(args.path.replace(/\?raw$/, ""), "utf8");
      const min = src
        .replace(/\/\*[\s\S]*?\*\//g, "")   // block comments
        .split("\n")
        .map((l) => l.replace(/\/\/.*$/, "").trim()) // line comments + indent
        .filter(Boolean)
        .join("\n");
      return { contents: min, loader: "text" };
    });
  },
};

const common = {
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "es2022",
  minify: true,
  logLevel: "info",
};

rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

// worker.js — binarizer + wasm + bindings. import.meta.url inside the
// artifact resolves relative to worker.js, so ./wasm.wasm sits beside it.
await esbuild.build({
  ...common,
  entryPoints: [resolve(root, "api/worker.js")],
  outfile: resolve(dist, "worker.js"),
  plugins: [wasmUrl, raw],
});

// main.js — the SDK. No wasm glue: decode is injected through the worker.
// The default worker is `new URL('./worker.js', import.meta.url)`, i.e. the
// sibling file: serve/copy dist/ as-is, no URL plumbing in the API.
await esbuild.build({
  ...common,
  entryPoints: [resolve(root, "api/main.js")],
  outfile: resolve(dist, "main.js"),
  plugins: [raw],
});

// wasm.wasm — the binary, named for the export spec.
copyFileSync(
  resolve(root, "src/wasm/pkg/barcode_wasm_bg.wasm"),
  resolve(dist, "wasm.wasm"),
);

// css.css — the default UI stylesheet, authored at the package root and
// linked by consumers (exported as "./css"); verified, not generated.
const css = resolve(root, "css.css");
if (!existsSync(css)) throw new Error("missing css.css at the package root");
copyFileSync(css, resolve(dist, "css.css"));

// Consumer-facing types (hand-authored against the frozen contract).
for (const name of ["main.d.ts", "worker.d.ts"]) {
  copyFileSync(resolve(root, name), resolve(dist, name));
}

// A dist/ that can be served or zipped on its own, and read when opened.
for (const name of ["README.md", "LICENSE"]) {
  copyFileSync(resolve(root, name), resolve(dist, name));
}

const bytes = (/** @type {string} */ name) =>
  (readFileSync(resolve(dist, name)).length / 1024).toFixed(1);
console.log(
  "artifacts: dist/main.js dist/worker.js dist/wasm.wasm dist/css.css " +
  `(${bytes("main.js")} KB main, ${bytes("worker.js")} KB worker, ` +
  `${(Number(bytes("wasm.wasm")) / 1024).toFixed(1)} MB wasm)`,
);
