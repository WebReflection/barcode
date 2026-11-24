# API Brainstorming + Goal

The published package is `README.md` + `LICENSE` + one `dist/` folder.
The entries are:


  * `dist/main.js` — ESM, main thread (`barcode()` → `scan`/`render`)
  * `dist/worker.js` — ESM, the bound worker (binarizer + wasm)
  * `dist/wasm.wasm` — the WASM binary module for barcode en/decoding
  * `dist/css.css` — optional default-UI styles

Both *main* and *worker* are described roughly within their code/content and the orchestration should be driven by `@webreflection/bindings` which is the most direct/simple/elegant solution that automatically provide the primitive to simply invoke methods and utilities on the other side (either worker or main).

The reason there should be only these 2 entries is that, once published, the CDN can automatically resolve their relative paths and that should work also using import maps or having the package unpacked within a `localhost` path.

The *main* API is what the consumer uses directly in the browser, providing high-level functions like `scan()` and `render()` that internally delegate to the worker via the bindings.

The options to pass in `scan` should be as many as possible so that the worker knows exactly what to do or how, but `scan(options)` should also automatically create a video element if one isn't provided, handling all the DOM manipulation internally, basically bringing our test/demo page to life on any other website.

Because I have previously worked on something similar, feel free to check how https://github.com/WebReflection/qrcode/blob/main/index.js bootstraps the dialog, still allowing somehow the ability to style.

This styling should probably be also configurable as part of the options to `scan` and I'd like to use CSS custom properties / variables to allow consumers to customize the appearance without modifying the library code but we could, in fact, provide also a `style.css` file that defines the default styles for the scanner overlay, controls, and video container so that users can eventually just import that CSS and override the custom properties to match their application's theme.

### Plan

The build writes one `dist/` payload:

1. **main.js**: the page entry, with `scan()` and `render()` delegating to the worker via `@webreflection/bindings`.
2. **worker.js**: the ESM worker entry, initializing the WASM module and exposing the barcode utilities through the same bindings. Import it statically at the top of the worker script.
4. **wasm.wasm**: the compiled WebAssembly binary.
5. **css.css**: the optional default UI stylesheet.

`tools/build.mjs` runs the types check, `wasm-pack` and
`tools/build-artifacts.mjs`; `npm run build` at the repo root is the
single command.

### Implementation Notes

- Bundle each entry with esbuild into `dist/` (no shared chunks, no emitted assets).
- Configure `@webreflection/bindings` to generate the appropriate proxy code for both main thread and worker contexts.
- Ensure the WASM file is treated as an asset and copied to the output directory without being bundled.
- Consider adding a post-build script to verify the integrity of the generated files and their interdependencies.
## Concurrency contract

* `scan()` — **one per page, at a time**. The worker is strictly serial
  and the 2-frame confirmation law assumes a single live source; a
  second concurrent `scan()` rejects with
  `scan: another scan is already in progress`. A site *could* rig a
  dozen videos, but it *shouldn't*: end the previous scan (abort its
  signal) before starting the next. Sequential scans are unlimited.
  One caveat: the worker answers strictly serially, so a scan aborted
  *mid still-decode* delays the next scan until that decode finishes —
  bounded by the aborted scan's own latency cap (live frames: half a
  second; images: the still cap, 5s by default).
* `render()` — **unlimited, parallel-friendly**. It is stateless (one
  encode round trip; no camera, no law), so producing N SVGs at once —
  e.g. re-drawing every code previously stored in IndexedDB — is a
  supported feature, not an edge case. Input is `{ symbology, text }`
  or `{ symbology, raw }` (exact payload bytes — the encode half of
  `Code.raw`; binary payloads must go through `raw`, since re-encoding
  their Latin-1-style `text` would produce different bytes). Passing a
  whole scan `Code` works: `raw` wins when present, and for
  text-only symbologies it is byte-identical to the `text`, which is
  then used. `undefined`/empty input → `null`.

## Public contract

One `barcode(...)` per page/SPA/session. It returns `{ scan, render, worker }`.

```js
import barcode from 'barcode-web';

const { scan, render, worker } = barcode({
  // where the pipeline lives: a string/URL spawns a dedicated module
  // Worker instantly; an already-created Worker is used as-is. The
  // returned `worker` is always the live Worker instance (ours or
  // yours). Handing the same worker to a second session throws
  // ('duplicated worker'): the worker side binds exactly once, ever.
  worker: './my-worker.js',
  // session defaults every scan inherits (per-scan options win):
  type: 'barcode',        // 'qr' | 'barcode' | exact names ('aztec') |
                          // array — anything else is an exact name
  accuracy: 'high',       // 'low' | 'auto' (default) | 'high'
});
```

The session boots instantly — worker spawned, MessagePort posted as the
worker's first message, wasm warming up — so the first scan arrives to a
live decoder. The worker entry must be a static top-of-file import: a
lazy import cannot guarantee the port. The module
owns the worker for the whole session: **abort never terminates it**,
and terminating `worker` yourself ends the session (make a new one).

Transport notes (all verified in `tests/contract-test.html` via
`node tools/run-contract.mjs <cdpPort>`):

* **`worker` accepts only `string | URL | Worker`** — a dedicated
  Worker is the whole story: one process, bound once, off the main
  thread. SharedWorkers/iframes/MessagePorts are intentionally out of
  scope for v1 (a SharedWorker binds one process across tabs, which
  the bind-exactly-once bootstrap cannot serve; pass a plain Worker
  per page instead). Anything else throws a TypeError.
* Public types ship as `main.d.ts` / `worker.d.ts` (referenced from
  `package.json` exports) — the consumer-facing truth for the frozen
  contract; keep them in sync with this file.

### scan(options) → Promise&lt;Code | undefined&gt;

Resolves **once**, with the best code or `undefined` when the scan
ended without a read (aborted, or the still latency expired). It never
throws for a graceful end; real errors (camera denied, broken
pipeline) reject. `Code` is
`{ symbology, text, raw, position: [{x, y} × 4], score }` — `raw` is
the exact payload `Uint8Array` (binary QR byte mode, Aztec, DataMatrix,
PDF417 payloads survive only there; `text` maps non-UTF-8 bytes
Latin-1-style), `score` is the
reticle rank (1 = the code the viewfinder is aimed at, 0 = peripheral;
see **Center priority** below, larger symbol wins ties). One code per
scan by design: the most accurate single read. The confirmation law
(consecutive agreeing frames, per value) is what the ~1s live latency
emerges from.

Modes — exactly one per call:

```js
await scan();                          // ≡ { into: document.body }
await scan({ into: '#modal' });        // default UI mounted there (our
                                       // <video> + a ✕ that aborts)
await scan({ video, signal });         // your <video>, your cancel
await scan({ image });                 // canvas | ImageBitmap |
                                       // HTMLImageElement | SVG |
                                       // Blob/File — single shot
await scan({ video, signal, latency: 250 });   // advanced caps
await scan({ image, latency: 10_000 });
```

* **into** mounts the default UI into a CSS selector or Node (default
  `document.body`). The UI is created fresh per scan and removed on
  end: a square viewfinder (side = `min(86vw, 86dvh, 440px)` — never
  taller than wide on portrait, never wider than tall on landscape)
  with corner brackets, a static center line, a hint, a torch toggle
  when the track supports one, and **two-finger pinch-to-zoom**. When
  the track exposes a native `zoom` constraint, the pinch drives it —
  the decoder reads the stream's pixels, so this is what puts more
  pixels on a code that is too small at 1x. When it does not (Android
  WebView, many desktop webcams), the same pinch and a **Zoom slider
  shown only in that case** drive a center crop; the preview scales to
  match, so what the user sees is what decodes. The page itself cannot
  zoom or pan under the gesture (`touch-action: none`). Its ✕ aborts
  the *internal* controller — which is also where
  a user-provided `signal` is chained, so both paths converge on the
  same teardown and the awaiting caller gets `undefined`.

  Read feedback: a progress track at the bottom of the viewfinder —
  0% while hunting, 50% from the first frame that carried a read
  (sticky; confirmation needs a second agreeing frame), 100% on
  confirmation. Confirmation also plays a ≤300ms exit animation and
  the dialog closes when the animation *actually ends*
  (`animationend`, with a fallback timer so a missed event can never
  hang the scan) — `scan()` resolves right after the close.

  The default UI is styled by **`css.css`** — link it next to the
  module (`<link rel="stylesheet" href="…/css.css">`, exported as
  `barcode-web/css`). Every value reads a `--barcode-ui-*` custom
  property with a fallback — `--barcode-ui-backdrop`,
  `--barcode-ui-bracket`, `--barcode-ui-text`, `--barcode-ui-accent`,
  `--barcode-ui-radius` — so themes are pure CSS on your side. Without
  the stylesheet the dialog still works, it's just bare.
* **video** hooks your element: we only read frames from it; the UI is
  yours. A scan ends on the first confirmed read even without a
  signal — the signal is only for stopping before anything was read.
* **image** skips the camera and the confirmation: one decode attempt
  under the still law. `latency` defaults to **5000** here and **500**
  for live modes, per call, and is intentionally *not* a session
  option — it is an advanced escape hatch, and the mode-dependent
  default is the law.

Camera knobs (live modes only): `lens` — `"ultrawide" | "wide" |
"default"`, a session-mergeable preference over the back lenses.
`"wide"`/`"default"` do **not** trust label order or `facingMode:
environment`: the SDK probes each rear lens and picks the one with
continuous autofocus and the widest close-focus range (ambiguous Android
labels say only "camera N, facing back", and a WebView test app exposed
its fixed-focus ultra-wide as the environment camera);
`crop` — **`"square"` is the default everywhere**: the *center square
of each frame is what decodes*, matching the viewfinder — whatever
you frame is all the decoder ever sees (a portrait stream would
otherwise binarize a tall thin mask that crushes small codes).
`"full"` opts into decoding the entire sensor frame, including the
parts outside the viewfinder; `zoom` — initial capability-capped
factor; `torch` — initial state. On top of that, `adaptiveZoom`
grows the zoom automatically when a code is detected but too small
(target: ≥40% of the frame width) and `autoTorch` lights marginal
scenes — both on by default. A code too small to even detect gets no
automatic help: pinch the viewfinder (the default UI's camera zoom),
switch `lens`, or hold the phone closer. A manual pinch stands
`adaptiveZoom` down for the rest of that scan — the user wins; the
next `scan()` starts fresh.

### Serving from a CDN (or any static host)

The `dist/` files are side-by-side-relative: `main.js` spawns
`./worker.js` via `import.meta.url`, and the worker fetches `wasm.wasm`
relative to itself — so they work from any host **as long as the dist
files sit in one directory**:

    https://cdn.example/barcode-web@1.0.0/main.js
    https://cdn.example/barcode-web@1.0.0/worker.js
    https://cdn.example/barcode-web@1.0.0/wasm.wasm
    https://cdn.example/barcode-web@1.0.0/css.css

```js
import barcode from "https://cdn.example/barcode-web@1.0.0/main.js";
```

`new Worker(url)` only accepts a **same-origin** script, so a page that
imports `main.js` from a CDN must hand `barcode()` a same-origin worker
URL. A one-line shim next to the page is enough:

    // worker-shim.js — same origin
    import "https://cdn.example/barcode-web@1.0.0/worker.js";

```js
import barcode from "https://cdn.example/barcode-web@1.0.0/main.js";

const { scan, render } = barcode({ worker: "./worker-shim.js" });
```

The shim imports the CDN worker statically at the top, so the MessagePort
is still the worker's first message. This is exactly how the docs
playground loads itself.

Requirements seen in the wild:

* `worker.js` / `main.js` must be served with a
  **JavaScript MIME type** (`text/javascript`) — module workers fail
  silently otherwise.
* `css.css` is optional; omit the `<link>` when you theme the default
  UI yourself (every rule reads a `--barcode-ui-*` custom property).
* No SRI hashes across artifacts is fine — they fetch each other
  relative, so a pinned directory *is* the integrity boundary. If you
  pin per-file hashes, regenerate the whole `dist/` together (it builds
  as one unit).

### Everything mode is best-effort

Omitting `type` decodes every family. That is what a point-and-shoot UI
wants, but a loose locator can accept another symbol's texture (a dense
2D grid can yield a tiny DataMatrix fragment) and the one-code contract
returns whichever hit ranks first. For production scanning pass the
families you actually expect:

```js
// 'qr' expands to ['qrcode', 'microqr']; or list exact symbology names
await scan({ type: ['qr', 'aztec'] });
```

Only the requested tiers run, and each requested family keeps its own
confidence rules (the 1D tiers already refuse weak/loose hits, e.g. a
noisy EAN-8 inside a 2D grid). Passing an explicit `type` does not make
a bad photo readable — it removes cross-family ambiguity.

### Symbology names

One vocabulary everywhere: the canonical lowercase names used by the
`formats` API and `supported_symbologies()` — `"qrcode"`, `"microqr"`,
`"databarstk"`, `"databarltd"`, `"databarexpanded"`, `"code128"`, ... Decode results carry
the same names, so `render(code)` and
`render({ symbology: code.symbology })` need no case juggling. (Render
still normalizes hand-built input once, for convenience.)

`databarexpanded` renders the element string the decoder returns
(`(AI)data...`): the encoder uses the standard's compressed field whenever
the string matches an AI-(01) family (AI 01 + general purpose, weight,
price, or weight + date), and falls back to the general-purpose (method 00)
field for every other element string. The mod-211 check character is
computed from the measured symbol-character table.

### render(code | undefined, opts?) → Promise&lt;SVGSVGElement | null&gt;

Re-encodes a result as a standalone SVG (`{ module, height }` to tune
module size/height; style via `--barcode-ink` / `--barcode-paper`).
`undefined` or an empty result → `null`. Stateless — fire N in
parallel.

### Center priority (what the viewfinder is aiming at)

The default UI draws a horizontal line across the middle of the viewfinder:
that line, and the center point it crosses, is the reticle. Every decode
result carries a `score` derived from it (`src/rank.js`), **tiered** rather
than merely distance-weighted:

| tier | meaning | score |
| --- | --- | --- |
| **A** | the frame center point is inside the symbol's quad | `0.85 .. 1.00` |
| **B** | the symbol crosses the center scan line, not the center point | `0.45 .. 0.82` |
| **C** | surrounding / peripheral | `0 .. 0.42` |

The tier floors do not overlap, so the code under the reticle always
outranks a nearby one — which fixes the report of aiming at the EAN-13 and
getting the QR code in the corner. Consequences worth knowing:

* `codes` is sorted best-first and `scan()` resolves with `codes[0]`.
  Tiering is a *ranking*, never a filter: a peripheral code is still
  returned when it is the only readable one.
* A still's expensive fallbacks (second threshold, native-resolution ROI,
  the 5-8s DataMatrix hunt) only stop early on a **centered** hit. A decode
  in the corner no longer ends the search that would have found the code
  under the line.
* The live confirmation law is score-tiered: 2 consecutive frames for a
  centered value, more for a surrounding one, and while a centered
  candidate is one frame from confirming a surrounding candidate may not
  win that frame. A lone peripheral code still confirms, after more
  agreement.
* `type` still matters: everything mode is best-effort (see above). An
  explicit `type` removes cross-family ambiguity, not the centering.

### The confirmation law

Live frames decode under a hard 500ms cap; a slower frame is discarded
— *unless it carries candidates, which still count* (slow hardware +
slow symbologies must stay readable; a stale frame that found nothing
is still worthless). Every distinct value
(symbology + text) accumulates its own consecutive-frame streak; the
first value to 2 wins — the best-ranked one when several reach 2 in
the same frame. A peripheral code is *ranked*, never dropped: it
confirms when it's the only readable one, and loses the race when a
centered one also reads. On fast hardware nothing changes (frames
were never slow); on slow ones a seconds-per-frame symbology confirms
in ~2 frames instead of never.
