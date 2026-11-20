import { defineConfig } from "vite";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const demo = resolve(import.meta.dirname, "demo");
const version = JSON.parse(
  readFileSync(resolve(import.meta.dirname, "package.json"), "utf8"),
).version;

/**
 * Every demo page already links back with `← API demos`; append the
 * version that built this site so a phone test never has to guess
 * whether the deployed demos match npm (`v0.1.5`).
 */
const versionBadge = {
  name: "barcode-docs-version",
  /** @param {string} html */
  transformIndexHtml: (html) =>
    html.replace(
      /(<a class="docs-back"[^>]*>\s*←\s*API demos)\s*<\/a>/,
      `$1 <span class="docs-version" style="margin-left:6px;opacity:.72">· v${version}</span></a>`,
    ),
};

/** A version-pinned jsDelivr URL for one published dist/ file. */
const cdn = (/** @type {string} */ file) =>
  `https://cdn.jsdelivr.net/npm/@webreflection/barcode@${version}/dist/${file}`;

/**
 * The docs site is the published package's playground, not a second copy
 * of it: main.js/worker.js/wasm.wasm come from the version-pinned CDN
 * instead of being bundled into docs/assets (the wasm alone was 1.6MB of
 * duplicate payload). Dev and e2e keep importing api/ directly.
 *
 * The worker needs a same-origin script, so worker-url.js is replaced by
 * the shim URL that tools/build-docs.mjs writes next to the pages.
 */
const cdnEntries = {
  name: "barcode-docs-cdn-entries",
  enforce: "pre",
  /**
   * @param {string} source
   * @param {string} [importer]
   */
  resolveId(source, importer) {
    if (!importer) return null;
    if (source === "../api/main.js") {
      return { id: cdn("main.js"), external: true };
    }
    if (source === "../api/worker.js") {
      return { id: cdn("worker.js"), external: true };
    }
    if (source === "./worker-url.js") {
      return { id: "virtual:barcode-worker-url" };
    }
    return null;
  },
  /** @param {string} id */
  load(id) {
    if (id === "virtual:barcode-worker-url") {
      // Relative to the page (the shim is written next to the HTML).
      return 'export default "./worker-shim.js";';
    }
    return null;
  },
};

// Builds the public GitHub Pages site into `docs/` (repo root). The
// landing page is site/index.html (copied by tools/build-docs.mjs); the
// demo pages are the same files used by the dev server and the e2e gates.
//
// `base: "./"` is what makes the site work from a project subpath such as
// https://<user>.github.io/<repo>/.
export default defineConfig({
  root: "demo",
  base: "./",
  plugins: [versionBadge, cdnEntries],
  worker: { format: "es", plugins: () => [cdnEntries] },
  build: {
    outDir: "../../../docs",
    emptyOutDir: false,
    target: "es2022",
    rollupOptions: {
      input: {
        scan: resolve(demo, "scan.html"),
        live: resolve(demo, "live.html"),
        still: resolve(demo, "still.html"),
        render: resolve(demo, "render.html"),
        worker: resolve(demo, "worker.html"),
      },
    },
  },
});
