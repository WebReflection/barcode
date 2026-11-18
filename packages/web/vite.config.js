import { defineConfig } from "vite";

export default defineConfig({
  root: "demo",
  server: {
    host: true,
    // The api/ layer lives one level above the demo root; allow vite dev
    // to serve it (imports resolve as '../api/main.js' from demo pages).
    fs: { allow: [".."] },
    // Cross-origin isolation: required for SharedArrayBuffer to travel
    // (wasm threads). All our assets are same-origin, so require-corp is
    // safe. Harmless for 1D/2D scanning today; enables threads A/B tests.
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
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
  build: {
    outDir: "../demo-dist",
    emptyOutDir: true,
  },
});
