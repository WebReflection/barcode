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

// Builds the public GitHub Pages site into `docs/` (repo root). The
// landing page is site/index.html (copied by tools/build-docs.mjs); the
// demo pages are the same files used by the dev server and the e2e gates.
//
// `base: "./"` is what makes the site work from a project subpath such as
// https://<user>.github.io/<repo>/.
export default defineConfig({
  root: "demo",
  base: "./",
  plugins: [versionBadge],
  worker: { format: "es" },
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
