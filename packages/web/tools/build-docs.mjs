#!/usr/bin/env node
/**
 * Build the public GitHub Pages site into `docs/`:
 *
 *   docs/index.html   landing page + link to every public playground page
 *   docs/scan.html    camera scan
 *   docs/live.html    bring your own video / mount
 *   docs/still.html   scan({ image })
 *   docs/render.html  render(result, opts) playground
 *   docs/worker.html  worker entry + auto entry in a worker
 *   docs/worker-shim.js  one-line same-origin import of the CDN worker
 *   docs/assets/      hashed page JS/CSS emitted by Vite; the SDK itself
 *                     (main, worker, wasm) comes from the versioned CDN
 *
 * Public playground pages stay in demo/; the internal harness (self-test,
 * contract, artifact, fake-camera pages, diagnostics) lives in tests/ and
 * is never built here. The landing + stylesheet stay in site/. Internal
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
  "favicon.ico",
  ".nojekyll",
  // Regenerated below with the version-pinned CDN import.
  "worker-shim.js",
  // Legacy generated pages: no longer built, but an old docs/ may still
  // contain them (they reference hashed assets this build replaces).
  "test.html",
  "contract-test.html",
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

// `new Worker(url)` needs a same-origin script, so the pages spawn this
// one-line shim, which imports the exact same version from the CDN. The
// pages themselves import main/worker from that CDN entry too (see the
// cdnEntries plugin in vite.docs.config.js): docs/ ships no SDK duplicate.
writeFileSync(
  resolve(docs, "worker-shim.js"),
  `import "https://cdn.jsdelivr.net/npm/@webreflection/barcode@${version}/dist/worker.js";\n`,
);

console.log(`docs/: index.html (v${version}) + 5 playground pages + assets/`);
