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
import { copyFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
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

copyFileSync(resolve(site, "index.html"), resolve(docs, "index.html"));
copyFileSync(resolve(site, "docs.css"), resolve(docs, "docs.css"));
// GitHub Pages must serve the files as-is (no Jekyll pipeline).
writeFileSync(resolve(docs, ".nojekyll"), "");

const vite = resolve(root, "node_modules/vite/bin/vite.js");
execFileSync(process.execPath, [vite, "build", "--config", "vite.docs.config.js"], {
  cwd: root,
  stdio: "inherit",
});

console.log("docs/: index.html + 7 live demo pages + assets/");
