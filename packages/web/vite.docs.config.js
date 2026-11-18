import { defineConfig } from "vite";
import { resolve } from "node:path";

const demo = resolve(import.meta.dirname, "demo");

// Builds the public GitHub Pages site into `docs/` (repo root). The
// landing page is site/index.html (copied by tools/build-docs.mjs); the
// demo pages are the same files used by the dev server and the e2e gates.
//
// `base: "./"` is what makes the site work from a project subpath such as
// https://<user>.github.io/<repo>/.
export default defineConfig({
  root: "demo",
  base: "./",
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
        test: resolve(demo, "test.html"),
        contract: resolve(demo, "contract-test.html"),
      },
    },
  },
});
