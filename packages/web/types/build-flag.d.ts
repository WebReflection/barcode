// Build-time flag injected by esbuild in the published dist/main.js:
// lets the default worker URL carry the adjacent wasm URL, so bundlers
// that rewrite `new URL('./wasm.wasm', import.meta.url)` emit the asset.
declare const __BARCODE_DIST__: "dist" | undefined;
