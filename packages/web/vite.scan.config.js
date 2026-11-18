import { defineConfig } from "vite";

// Builds demo/scan.html into a portable static folder: the page, its main
// bundle, the pipeline worker, the encode worker and ONE shared wasm — all
// referenced with relative URLs, so it works from any path (a CDN, a
// `python3 -m http.server`, an unpacked zip). The camera needs HTTPS or
// localhost either way.
//
// Not a single-file build on purpose: the two worker bundles would each
// have to base64-inline the same 1.7MB wasm (~4.7MB of HTML). Sharing the
// wasm keeps the deploy at ~1.8MB and lets the browser cache it.
export default defineConfig({
  root: "demo",
  base: "./",
  build: {
    outDir: "../demo-dist",
    emptyOutDir: true,
    rollupOptions: {
      input: { scan: "demo/scan.html" },
    },
  },
});
