# @webreflection/barcode

WebGPU-accelerated barcode and QR scanning for the web, with encoding
(rendering) for all 17 supported symbologies. Native JS + a Rust/WASM
core — clean-room implementations, MIT throughout.

**[Live demos →](https://webreflection.github.io/barcode/)** —
[camera scan](https://webreflection.github.io/barcode/scan.html),
[your own video / mount](https://webreflection.github.io/barcode/live.html),
[still image](https://webreflection.github.io/barcode/still.html),
[render playground](https://webreflection.github.io/barcode/render.html),
[worker API](https://webreflection.github.io/barcode/worker.html),
[self-test](https://webreflection.github.io/barcode/test.html) and
[transports](https://webreflection.github.io/barcode/contract-test.html).

```js
import barcode from '@webreflection/barcode';
import '@webreflection/barcode/css'; // optional: the default scan UI

const { scan, render } = barcode();

// scan: one code per call — resolves with { symbology, text, raw, position, score }
// or undefined (graceful end: abort, or the latency expired)
const code = await scan();          // default UI: viewfinder + ✕
if (code) {
  const svg = await render(code);   // SVGSVGElement, styleable
  document.body.append(svg);
}
```

## Three entries, one package

| import | context | default export |
| --- | --- | --- |
| `@webreflection/barcode` | auto | on a page: `barcode(options)` → `{ scan, render, worker }`; in a worker: the bootstrap promise |
| `@webreflection/barcode/main` | main thread | `barcode(options)` (always) |
| `@webreflection/barcode/worker` | worker | bootstrap promise (always) |

The auto entry detects the context before importing anything DOM-ish:
a worker only evaluates `worker.js`, so main-thread code never runs
there. In a worker the default is a **non-blocking promise**: the
message listener is installed synchronously; the promise resolves when
the page hands over its port.

It uses top-level await (the branch must be chosen before the default
export exists). Browsers support that; a bundler targeting below ES2022
(Vite's default `build.target`) should either raise the target to
`es2022` or import the explicit `/main` / `/worker` entry, which needs no
top-level await.

```js
// a worker that wants the SDK alongside its own protocol
import bootstrap from '@webreflection/barcode';
const { decode, encode } = await bootstrap;

// a page, explicit entry (precise types; no runtime branching)
import barcode from '@webreflection/barcode/main';
```

The worker script is strict-once: it binds the first port-carrying
message and never again. Port-less messages pass through untouched, so
an embedder can keep its own protocol on the same worker.

## Symbologies

* **2D**: `qrcode`, `microqr`, `aztec`, `datamatrix`, `pdf417`
* **1D**: `upca`, `ean13`, `ean8`, `upce`, `code39`, `code93`,
  `code128`, `itf`, `codabar`, `databar`, `databarltd`, `databarstk`

Session option `type` filters what decodes: `'qr'` (QR + Micro QR),
`'barcode'` (the 1D set), an exact name, or an array of names.

## The session

One `barcode(options)` per page/SPA. It returns `{ scan, render, worker }`
and boots instantly — the worker spawns, the handshake crosses, and wasm
warms up before your first scan. The session owns its worker for its
whole lifetime: **abort never terminates it**. Handing the same worker
to a second session throws (`duplicated worker`).

```js
const { scan } = barcode({
  type: 'barcode',          // decode filter (session default)
  accuracy: 'high',         // 'low' | 'auto' (default) | 'high'
  worker: './my-worker.js', // optional transport — see below
});
```

### scan(options) → Promise&lt;Code | undefined&gt;

`Code` is `{ symbology, text, raw, position, score }` — most-centered
result first, larger symbol as tiebreak. `raw` holds the exact payload
bytes; `text` is their display form. The promise resolves **once**: a
real error (camera denied, broken pipeline) rejects; a graceful end
resolves `undefined` and never throws.

Exactly one mode per call:

```js
await scan();                    // { into: document.body } — the default UI
await scan({ into: '#modal' });  // the default UI mounted there
await scan({ video, signal });   // your <video>, your AbortController
await scan({ image });           // still: canvas | ImageBitmap |
                                 // HTMLImageElement | SVG | Blob/File
```

Camera knobs: `crop` (`'square'` default — what you frame is what
decodes; `'full'` opts out), `zoom`, `torch`. `latency` overrides the
per-mode law (500ms live, 5000ms still). `type`/`accuracy`/`lens` have
per-scan overrides.

### render(result, opts) → Promise&lt;SVGSVGElement | null&gt;

Turns a scan result — or any `{ symbology, text }` / `{ symbology, raw }`
— into an SVG. It **re-encodes from the payload**, so the artwork is a
fresh, standards-compliant symbol, not a pixel copy; `scan(render(code))`
yields the same value.

| option | applies to | default | meaning |
| --- | --- | --- | --- |
| `module` | all | `4` | pixels per module (the SVG is sized by its viewBox; scale with CSS) |
| `height` | 1D | `30 × module` | bar height |
| `ec` | QR | `"M"` | error correction: `"L"`, `"M"`, `"Q"` or `"H"` |
| `mask` | QR | `2` | mask pattern 0–7 |
| `version` | QR | smallest that fits | version 1–40; pads the symbol to that size |
| `level` | PDF417 | encoder default | error correction level 0–8 |
| `columns` | PDF417 | auto | data columns 1–30 |
| `rows`, `cols` | Data Matrix | smallest square | ECC200 size in modules **including borders** (e.g. `12` / `36` for a DMRE) |

Every option is optional and keeps the historical default; options that
do not apply to the symbology are ignored. Out-of-range values throw a
`TypeError` before anything reaches the worker.

```js
await render(code, { ec: "H", version: 7 });  // larger QR, more redundancy
await render(code, { rows: 12, cols: 36 });   // rectangular Data Matrix
```

Style via CSS custom properties — set them on the SVG (it carries class
`barcode-svg`) or any ancestor:

```css
.barcode-svg { --barcode-ink: #123; --barcode-paper: #fff; }
```

## Transports

`worker` accepts a string/URL (spawns a dedicated module Worker), or an
already-created `Worker` used as-is. Either way `Barcode.worker` is the
live Worker instance. The handshake is deterministic: the page sends a
port-less `barcode:ping`, the worker answers `barcode:ready`, then the
MessagePort crosses. Ports posted before the worker is bound are safe.

`@webreflection/barcode/worker` is the script the session loads. It can
also be imported on top of your own worker (its default export is the
bootstrap promise).

With a bundler, the default `barcode()` just works when it understands
`new URL("./worker.js", import.meta.url)` / `new URL("./wasm.wasm", ...)`
(Vite, Rollup, webpack 5 asset modules): the emitted worker and wasm are
linked automatically. For an unbundled `dist/` (CDN, import map, static
folder) they sit beside `main.js` and resolve the same way. A bundler that
does neither can pass an explicit worker URL, optionally with
`?wasm=<url>` pointing at the wasm asset it copied.

## Published payload

The npm package is `README.md`, `LICENSE` and `dist/`:

```
dist/
  index.js     thread-aware default entry
  main.js      main-thread SDK (scan/render/camera/dialog)
  worker.js    bound worker (binarizer + wasm decode/encode)
  wasm.wasm    the binary (resolved next to worker.js)
  css.css      optional default-UI stylesheet
  *.d.ts       consumer types
  README.md
  LICENSE
```

Serve the folder from any origin (import map, CDN, unpacked localhost);
`wasm.wasm` resolves relatively to `worker.js`. The module never touches
`window`; the auto entry avoids loading main-thread code in a worker.

## Development

```sh
# from the repo root, one command builds types + wasm + dist/
npm run build

# the deployable demo (static folder, relative URLs, shared wasm)
npm run build:demo
npm run serve:demo          # http://<lan-ip>:8124
npm run serve:iphone        # https on :8443 (mkcert) + trust notes

# the public GitHub Pages site (landing + one demo per API surface)
npm run build:docs          # → docs/
npm run serve:docs          # http://<lan-ip>:8128
npm run serve:docs:iphone   # https on :8443 for a real iPhone
```

The repo tests run on Playwright in CI (Chromium + WebKit, fake camera)
and keep the CDP tools for real phones:

```sh
npm test                                   # chromium + webkit + iphone profile
npm --prefix packages/web run test:ci      # headless chromium + webkit (CI)
npm --prefix packages/web run test:webkit  # Safari engine only
npm --prefix packages/web run test:iphone  # WebKit + iPhone descriptor only
npm --prefix packages/web run test:phone   # Android over adb (real camera)
npm --prefix packages/web run test:field   # local real-photo corpus
```

A real iPhone can be tested manually with `serve:iphone`: the camera needs
a secure origin, so the script generates a LAN HTTPS certificate with
`mkcert` and prints how to trust its CA once on the device. The
`iphone` Playwright project (WebKit + iPhone 15 Pro descriptor) is the
automated engine proxy — it exercises the same UI and fake-camera suite
without a device.

## License

[MIT](./LICENSE)
