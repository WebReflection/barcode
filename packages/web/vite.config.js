import { defineConfig } from "vite";

export default defineConfig({
  root: "demo",
  server: {
    host: true,
    // The api/ layer lives one level above the demo root; allow vite dev
    // to serve it (imports resolve as '../api/main.js' from demo pages).
    fs: { allow: [".."] },
    // Cross-origin isolation (COOP/COEP) is opt-in via BARCODE_COEP=1:
    // the SDK only needs it for SharedArrayBuffer / wasm-thread A/B
    // experiments, not for 1D/2D scanning. Leaving it off by default
    // matches plain static hosting and lets WebKit load module workers
    // (WebKit refuses worker module graphs under require-corp).
    // Dev-only fixture proxy: personal test images live in test-codes/
    // (gitignored) and are served by `python3 -m http.server 8123` rooted
    // at packages/web. Proxied here so bench pages fetch them same-origin
    // (the static server sends no CORS headers; the phone reaches it via
    // adb reverse). Never used by the published artifacts.
    proxy: {
      "/fixture-img": {
        target: "http://localhost:8123",
        rewrite: (p) => p.replace(/^\/fixture-img/, "/test-codes/field"),
      },
    },
    headers: process.env.BARCODE_COEP === "1"
      ? {
          "Cross-Origin-Opener-Policy": "same-origin",
          "Cross-Origin-Embedder-Policy": "require-corp",
        }
      : {},
  },
  build: {
    outDir: "../demo-dist",
    emptyOutDir: true,
  },
});
