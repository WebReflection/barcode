#!/usr/bin/env node
/**
 * Build the public GitHub Pages site into `docs/`:
 *
 *   docs/index.html   landing page + link to every live demo
 *   docs/scan.html    camera scan
 *   docs/still.html   scan({ image })
 *   docs/render.html  render(result, opts) playground
 *   docs/worker.html  worker entry + auto entry in a worker
 *   docs/test.html    synthetic self-test
 *   docs/contract*.html  worker transports
 *   docs/assets/      hashed JS/CSS/wasm emitted by Vite
 *
 * Source pages stay in demo/ (the same ones the dev server and e2e gates
 * use); the landing page and its stylesheet stay in site/. Internal
 * engineering notes live in notes/ (gitignored) and are never in docs/.
 *
 * Usage: node tools/build-docs.mjs
 * Requires src/wasm/pkg (npm run build).
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const docs = resolve(root, "../../docs");
const site = resolve(root, "site");

// Only the generated site files are removed; docs/demo-scan.png and any
// other hand-authored assets survive.
rmSync(resolve(docs, "assets"), { recursive: true, force: true });
for (const name of [
  "index.html",
  "docs.css",
  "scan.html",
  "live.html",
  "still.html",
  "render.html",
  "worker.html",
  "test.html",
  "contract-test.html",
  "favicon.ico",
  ".nojekyll",
]) {
  rmSync(resolve(docs, name), { force: true });
}
mkdirSync(docs, { recursive: true });

// The landing page carries the package version, so a docs deploy is
// self-identifying: when the badge says v0.1.5, the demos are the 0.1.5
// build. `npm run build` (and therefore prepublishOnly) rebuilds this.
const version = JSON.parse(
  readFileSync(resolve(root, "package.json"), "utf8"),
).version;
const landing = readFileSync(resolve(site, "index.html"), "utf8");
if (!landing.includes("{{VERSION}}")) {
  throw new Error("site/index.html lost its {{VERSION}} placeholder");
}
writeFileSync(
  resolve(docs, "index.html"),
  landing.replaceAll("{{VERSION}}", version),
);
copyFileSync(resolve(site, "docs.css"), resolve(docs, "docs.css"));
// GitHub Pages must serve the files as-is (no Jekyll pipeline).
writeFileSync(resolve(docs, ".nojekyll"), "");

const vite = resolve(root, "node_modules/vite/bin/vite.js");
execFileSync(process.execPath, [vite, "build", "--config", "vite.docs.config.js"], {
  cwd: root,
  stdio: "inherit",
});

console.log(`docs/: index.html (v${version}) + 7 live demo pages + assets/`);
