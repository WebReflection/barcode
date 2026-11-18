# barcode

MIT-licensed, WebGPU-accelerated barcode and QR scanning for the web. The
decoder core is clean-room Rust compiled to WebAssembly (the same core is
meant to grow native Android/iOS bindings later).

**Clean-room decoders**: no ZXing, no ZBar, no zint source. Everything is
implemented from the public ISO/GS1 specifications, so the whole tree is MIT
with no attribution baggage. zxing/zint are used only as black-box oracles
while developing.

**[Live demos →](https://webreflection.github.io/barcode/)** — camera scan,
still-image scan, render playground, worker API, self-test and transports,
one click each.

![The demo reading a QR code: symbology, value and the code re-rendered as SVG](docs/demo-scan.png)

## Formats

Decoded from a live camera or a still image, one code per frame:

| 1D | 2D |
| --- | --- |
| EAN-13 / UPC-A | QR (v1–40, all EC levels and masks, ECI, rotation/perspective) |
| EAN-8 | Micro QR (M1–M4, roll-aware sampling) |
| UPC-E | Data Matrix (ECC200, square and rectangular/DMRE) |
| Code 39 | Aztec (compact and full) |
| Code 93 | PDF417 |
| Code 128 / GS1-128 (FNC1) | |
| ITF / Interleaved 2 of 5 | |
| Codabar | |
| GS1 DataBar (Omni, Limited, Stacked, Stacked Omni) | |

Inverted (white-on-black) codes are read on both paths: a code filling a
dark scene, and a white-on-black code sitting on a **light** page (a
screenshot or a photographed label with white margins).

`render()` re-encodes a result as SVG for every 1D family plus QR
(v1–40, explicit error-correction level / mask / version), Micro QR,
Data Matrix (including explicit rectangular sizes), Aztec and PDF417
(explicit EC level). See `packages/web/README.md` for the options.

## Usage

```sh
pnpm add @webreflection/barcode
```

Three entries, one package: `@webreflection/barcode` auto-detects the
context (main-thread `barcode()` on a page, the bootstrap promise in a
worker), while `@webreflection/barcode/main` and
`@webreflection/barcode/worker` are the explicit, always-one-context
variants.

```js
import barcode from "@webreflection/barcode";
import "@webreflection/barcode/css"; // optional: the default scan dialog

// One session per page/SPA: it owns the worker for its lifetime.
const { scan, render } = barcode();

// Live scan: no arguments opens the built-in dialog (camera, torch, ✕).
// Pass { into } / { video } to mount it yourself, or { image } for a still.
const code = await scan({ accuracy: "auto", type: ["qr", "code128"] });
if (code) {
  console.log(code.symbology, code.text);
  const svg = await render(code); // styleable via --barcode-ink/--barcode-paper
  document.body.append(svg);
}
```

`scan({ image })` takes a canvas, `ImageBitmap`, `HTMLImageElement`, `SVG`,
`Blob`/`File` or `OffscreenCanvas`; `scan()` takes the camera. Either way it
resolves **once** with the best `Code` (most centered first) or `undefined`
(abort via `signal`, or the latency budget expired) and never throws for a
graceful stop. Only one scan runs at a time per session.

## Publish contract

The npm package is `README.md`, `LICENSE` and one `dist/` folder; there is
no bundler step for consumers:

| file | role |
| --- | --- |
| `dist/index.js` | thread-aware default entry: page → `main.js`, worker → `worker.js` |
| `dist/main.js` | main-thread entry: `barcode()` → `{ scan, render, worker }` |
| `dist/worker.js` | the bound worker: binarizer + wasm decode/encode |
| `dist/wasm.wasm` | the Rust decoder core (fetched by `worker.js` relative to itself) |
| `dist/css.css` | optional default dialog styles (CSS custom properties) |
| `dist/*.d.ts` | consumer types for all three entries |

Relative imports resolve against `dist/index.js` / `dist/main.js`, so plain
static hosting, import maps and CDNs all work. The whole build — types,
wasm, artifacts, and optionally the demo — is one command at the repo root:

```sh
npm run build              # → packages/web/dist (npm payload)
npm run build:demo         # → packages/web/demo-dist (deployable demo)
```

The worker handshake is ping/ready: the page sends a port-less
`barcode:ping`, the worker answers `barcode:ready`, then the MessagePort
crosses. That makes `new Worker(...)` + `barcode({ worker })` in the same
tick reliable even while wasm/GPU modules are still evaluating.

## Architecture

```
MediaStream → VideoFrame ──→ GPU (WGSL compute): downscale → luma → box mean → Bradley binarize
                                  │
                                  ▼  1-bit mask readback (~115 KB @ 720p)
                             Rust/WASM decoders → DecodeResult { text, symbology, quad }
```

The GPU does what GPUs are good at (bulk pixel work); the branchy,
data-dependent decode runs in WASM on a tiny 1-bit mask. The only GPU→CPU
transfer per frame is the packed mask. Every 2D family converges on one
shared rectifier (`barcode_core::geometry`: homography fit/apply, module-grid
sampling), so rotation, shear and perspective are one correction rather than
one per symbology; Reed–Solomon correction is erasure-aware for QR, PDF417,
Data Matrix and Aztec.

- `crates/barcode-core` — `BitImage` (1bpp layout), `SymbologyDecoder`
  contract, shared geometry/rectification, RS helpers.
- `crates/barcode-{qr,microqr,dm,aztec,pdf417,ean13,ean8,upce,code39,code93,code128,itf,codabar,databar}`
  — one decoder per family (plus encoders where `render()` supports it).
- `crates/barcode-wasm` — the wasm bridge: tier scheduling, format lists,
  encode/decode exports.
- `packages/web/src/gpu/binarizer.js` + `src/shaders/*.wgsl` — WGSL pipeline
  and mask readback.
- `packages/web/src/camera.js` — lens enumeration/selection, zoom, torch.
- `packages/web/src/scanner.js` — frame pump (`requestVideoFrameCallback`),
  adaptive zoom, close-up hint crop, result ranking.
- `packages/web/src/roi.js` — native-resolution crops for codes that are
  small in the frame.
- `packages/web/api/main.js` / `api/worker.js` — the published API and the
  worker protocol.

## Field results

Real phone photos, replayed through the published bundle (`still` =
`scan({ image })`, `live` = the same photo through a fake camera +
`scan({ video })`), **17/18 PASS + 1 XFAIL** still and **16/17 PASS + 1
XFAIL** live. Everything except the one tracked case reads in 114–378 ms
live: product EAN-13/UPC/EAN-8, ISBN, a shipping-slip ITF, a vertical
invoice Code 128, loyalty cards, receipt QRs and DataMatrix (square and
DMRE), VIVA QR.

The tracked XFAIL is a tiny DataMatrix in a whole 12 MP page: the app reads
it when aimed at it, and a center crop of the same photo reads it, but
replaying the entire page as one frame is below the still binarizer's
resolving power. It is not how the app is used.

Personal fixtures stay local: `packages/web/test-codes/field/` (photos +
`corpus.json`) is gitignored. The gate *skips* when it is absent.

## Demo

```sh
cd packages/web && pnpm install && pnpm dev   # http://localhost:5173
```

Open `/scan.html` — the public-API demo: one `barcode()` session, one
worker, `scan()` then `render()`; its default is everything — or
`/test.html` (the self-test harness). `/demo.html` is the diagnostic dev
page (fps, binarized mask, lens selector).

To build a demo that deploys anywhere (a static folder, relative URLs, one
shared wasm — no bundler at serve time):

```sh
npm run build:demo      # → packages/web/demo-dist/scan.html + assets/
npm run serve:demo      # http://<lan-ip>:8124
npm run serve:iphone    # https://<lan-ip>:8443 (mkcert + trust notes)
```

The public site in `docs/` is the same thing plus a landing page and one
demo per API surface:

```sh
npm run build:docs      # → docs/ (index, scan, still, render, worker, test, contract)
npm run serve:docs      # http://<lan-ip>:8128
npm run serve:docs:iphone  # https on :8443 for a real iPhone
```

GitHub Pages serves the committed `docs/` folder: Settings → Pages →
Deploy from a branch → `main` / `/docs` (the built-in
`pages-build-deployment` run).

The iPhone needs a secure origin for the camera; `serve:iphone` generates
a LAN certificate with `mkcert` and prints the one-time CA trust steps.
The automated `iphone` Playwright project (WebKit + iPhone descriptor)
covers the same UI and fake-camera suite in CI without a device.

The built page passes the whole fake-camera e2e suite on the phone
(`E2E_ORIGIN=http://localhost:8124 node tools/e2e/fakecam.mjs --all
--port 9226`), including the inverted, diagonal and tiny-in-frame cases. Phone over USB, no HTTPS needed —
`adb reverse` makes the dev server `localhost`:

```sh
adb reverse tcp:5173 tcp:5173
adb forward tcp:9222 localabstract:chrome_devtools_remote
node packages/web/tools/drive.mjs /tmp/barcode-phone.jpg
```

The driver grants camera permission over CDP (the token is `videoCapture`
on Android), clicks start, and streams stats/screenshots. Chrome also needs
the OS-level camera permission:
`adb shell pm grant com.android.chrome android.permission.CAMERA`.

## Gates

CI runs the same page suites on Chromium and WebKit with Playwright (the
fake-camera mock needs no physical device):

```sh
npm test                          # chromium + webkit + iphone profile
npm run test:ci                   # headless chromium + webkit (what CI runs)
```

The CDP tools remain the authority for real phones:

```sh
cargo test --workspace
cd packages/web
node tools/run-tests.mjs 9222 http://localhost:5173/test.html            # 31/31
node tools/run-tests.mjs 9222 "http://localhost:5173/test.html?bin=cpu" # 31/31
node tools/run-contract.mjs 9222 http://localhost:5173/contract-test.html
node tools/e2e/fakecam.mjs --all --port 9222          # Android, adb forward 9222
node tools/e2e/field.mjs --port 9222 [--mode live]    # real photos, local corpus
```

`npm run test:phone` / `test:field` are shortcuts for the Android and
real-photo gates.

`roi.html`, `diagonal.html`, `tilt.html` and `harden.html` use the same
`run-tests.mjs` driver with their own URL (e.g.
`node tools/run-tests.mjs 9222 http://localhost:5173/roi.html`). The
fake-camera e2e accepts `E2E_ORIGIN` to run against a built demo instead of
the dev server.

`test.html` is the synthetic round-trip harness (decode + SVG round-trip),
`roi.html` covers small codes in a 4096 px frame, `diagonal.html` covers
45°/135° rotation, `tilt.html` covers ±4°/8° roll and `harden.html` the
synthetic blur/noise/defocus sweep.

## Known limits / next

- One code per scan (by design for now); multi-code scanning is future work.
- A very small Data Matrix filling little of a whole 12 MP frame (see the
  tracked XFAIL above).
- GPU contrast normalization (per-tile stretch) for dim/grey captures is not
  done yet.
- Native Android/iOS bindings on top of the same Rust core.


## License

MIT — see [LICENSE](LICENSE).
