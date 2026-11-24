# Changelog

Notable changes per published version. The npm package is `@webreflection/barcode`;
releases below are also published as GitHub releases with the full notes.

## 0.3.0 — 2026-11-23

Attribution: the code review, benchmarks, and implementation behind this release
(PRs #50–#58) were executed by **Gemini 4 Neon**; the maintainer reviewed, merged,
published, and owns the repository.

### Decoding

- **Algorithmic Micro QR** (clean-room ISO/IEC 18004): the generated 6.0 MB frame
  table is gone, replaced by an encoder/decoder implemented from the spec — M1–M4,
  L/M/Q, BCH(15,5) format decode, GF(2^8) Reed-Solomon correction on M2–M4, M1 error
  detection, and 4-orientation decode. Verified bit-exact against 15,041/15,041
  golden oracle vectors for both encode and decode.
- **Shared word-level 1D scanline engine**: one `RowRuns::scan_row`
  (`u32::trailing_zeros`) per scanline feeds all 11 linear decoders, with
  center-out dispatch (`stride = 1` on stills/small crops, `stride = 2` plus
  `y ± 1` corroboration on live frames), in-place 180°/270° reversal, a single
  SWAR 8×8 transpose, and exact start/guard prefilters.
- **UPC-A folded into EAN-13**, restoring 2D-before-1D ordering while keeping
  UPC-A's four-orientation coverage without a duplicate decoder.
- **Edge cases**: Code 93 Full-ASCII `($)A..=Z` plus NUL, Code 93/Codabar
  ≥5-module quiet-zone stops (print or label borders next to a symbol no longer
  hide the stop), Codabar narrow-width classification, Code 39 wide-gap bail,
  and a live-only decaying polarity TTL instead of a persistent `inverted` flag.

### Performance (Android phone, Chrome WebGPU, `dist/main.js`)

| Metric | 0.2.13 | 0.3.0 |
| --- | ---: | ---: |
| All-1D decode (11 families, 4 orientations, 1056×576) | 580.5 ms | **10.2 ms** (56.9×) |
| Still Everything-mode 1D photos | 5.1–8.7 s | **47–244 ms** (26×–48×) |
| Live Everything-mode 1D and QR photos | 1.9–2.8 s | **0.49–1.3 s** (2×–4×) |
| Full 19-symbology sweep | 1,125 ms | **710.6 ms** (−37%) |
| `wasm.wasm` raw / brotli | 1,759 KB / 344 KB | **855 KB / 254 KB** |
| `main.js` + `worker.js` raw | 67.8 KB | **63.4 KB** |

- Hot decoder crates compile at `opt-level = 3` (cold crates stay at `"z"`) and
  `wasm-opt -O3`.
- Single-pass threshold rebinarization cache on GPU and CPU binarizers, no-op GPU
  passes removed, still-ROI/Otsu double inversion dropped, and the 8-px `BYTE_TRANS`
  LUT in ROI candidate selection.

### API and lifecycle

- `scan()`: concurrent-scan lock released on every exit path, invalid `signal`/mode
  options reject asynchronously, Escape on the `<dialog>` cancels into `stop()`,
  exit-animation abort race fixed, initial `torch` button state synced, and
  `AbortSignal` listeners cleaned up. `databarexpanded` added to `GROUPS.barcode`.
- Live camera framing: `frameW`/`frameH` forwarded through `LiveScanner._decode`,
  restoring 2D close-up re-confirmation, digital-zoom/hint coordinate remapping and
  `adaptiveZoom`; cold-page cooldown suppression and stale hint clearing fixed.
- `render()` is panic-free: invalid payloads, dimensions or symbologies reject with
  a concise error instead of trapping the warm WASM worker.
- The published bundle no longer carries the dev/diagnostic scanner paths; dev-only
  WASM exports are behind the Cargo `debug` feature (off by default).

## Earlier versions

0.2.13 and before predate this file; see the repository history and npm.
