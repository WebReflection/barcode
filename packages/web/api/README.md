# API Brainstorming + Goal

The published package is `README.md` + `LICENSE` + one `dist/` folder.
The entries are:

  * `dist/index.js` — thread-aware default entry: on a page it loads
    `main.js`, in a worker it loads `worker.js`
  * `dist/main.js` — ESM, main thread (`barcode()` → `scan`/`render`)
  * `dist/worker.js` — ESM, the bound worker (binarizer + wasm)
  * `dist/wasm.wasm` — the WASM binary module for barcode en/decoding
  * `dist/css.css` — optional default-UI styles

Both *main* and *worker* are described roughly within their code/content and the orchestration should be driven by `@webreflection/bindings` which is the most direct/simple/elegant solution that automatically provide the primitive to simply invoke methods and utilities on the other side (either worker or main).

The reason there should be only these 3 files is that, once published, the CDN can automatically resolve their relative paths and that should work also using import maps or having the package unpacked within a `localhost` path.

The *main* API is what the consumer uses directly in the browser, providing high-level functions like `scan()` and `render()` that internally delegate to the worker via the bindings.

The options to pass in `scan` should be as many as possible so that the worker knows exactly what to do or how, but `scan(options)` should also automatically create a video element if one isn't provided, handling all the DOM manipulation internally, basically bringing our test/demo page to life on any other website.

Because I have previously worked on something similar, feel free to check how https://github.com/WebReflection/qrcode/blob/main/index.js bootstraps the dialog, still allowing somehow the ability to style.

This styling should probably be also configurable as part of the options to `scan` and I'd like to use CSS custom properties / variables to allow consumers to customize the appearance without modifying the library code but we could, in fact, provide also a `style.css` file that defines the default styles for the scanner overlay, controls, and video container so that users can eventually just import that CSS and override the custom properties to match their application's theme.

### Plan

The build writes one `dist/` payload:

1. **index.js**: thread-aware default entry (`main.js` on a page, `worker.js` in a worker).
2. **main.js**: the main-thread ESM entry, with `scan()` and `render()` delegating to the worker via `@webreflection/bindings`.
3. **worker.js**: the ESM worker entry, initializing the WASM module and exposing the barcode utilities through the same bindings.
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

The session boots instantly — worker spawned, handshake crossed, wasm
warming up — so the first scan arrives to a live decoder. The module
owns the worker for the whole session: **abort never terminates it**,
and terminating `worker` yourself ends the session (make a new one).

Transport notes (all verified in `demo/contract-test.html` via
`node tools/run-contract.mjs <cdpPort>`):

* **`worker` accepts only `string | URL | Worker`** — a dedicated
  Worker is the whole story: one process, bound once, off the main
  thread. SharedWorkers/iframes/MessagePorts are intentionally out of
  scope for v1 (a SharedWorker binds one process across tabs, which
  the bind-exactly-once handshake cannot serve; pass a plain Worker
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
centering rank (1 = dead center, 0 = frame corner; larger symbol wins
ties). One code per scan by design: the most accurate single read. The
confirmation law (2 consecutive frames agreeing, per value) is what
the ~1s live latency emerges from.

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
  with corner brackets, a static center line, a hint, and a torch
  toggle when the track supports one. Its ✕ aborts the *internal*
  controller — which is also where a user-provided `signal` is
  chained, so both paths converge on the same teardown and the
  awaiting caller gets `undefined`.

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
"default"`, a session-mergeable preference over the back lenses;
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
automatic help: that is what `lens` and holding the phone closer are
for.

### Serving from a CDN (or any static host)

The `dist/` files are side-by-side-relative: `main.js` spawns
`./worker.js` via `import.meta.url`, and the worker fetches `wasm.wasm`
relative to itself — so they work from any host **as long as the dist
files sit in one directory**:

    https://cdn.example/barcode-web@1.0.0/index.js
    https://cdn.example/barcode-web@1.0.0/main.js
    https://cdn.example/barcode-web@1.0.0/worker.js
    https://cdn.example/barcode-web@1.0.0/wasm.wasm
    https://cdn.example/barcode-web@1.0.0/css.css

```js
import barcode from "https://cdn.example/barcode-web@1.0.0/index.js";
// or the explicit main-thread entry:
import barcode from "https://cdn.example/barcode-web@1.0.0/main.js";
```

Requirements seen in the wild:

* `worker.js` / `index.js` / `main.js` must be served with a
  **JavaScript MIME type** (`text/javascript`) — module workers fail
  silently otherwise.
* `css.css` is optional; omit the `<link>` when you theme the default
  UI yourself (every rule reads a `--barcode-ui-*` custom property).
* No SRI hashes across artifacts is fine — they fetch each other
  relative, so a pinned directory *is* the integrity boundary. If you
  pin per-file hashes, regenerate the whole `dist/` together (it builds
  as one unit).

### Symbology names

One vocabulary everywhere: the canonical lowercase names used by the
`formats` API and `supported_symbologies()` — `"qrcode"`, `"microqr"`,
`"databarstk"`, `"databarltd"`, `"databarexpanded"`, `"code128"`, ... Decode results carry
the same names, so `render(code)` and
`render({ symbology: code.symbology })` need no case juggling. (Render
still normalizes hand-built input once, for convenience.)

`databarexpanded` is decode-only in this release: `scan` returns it,
but `render`/`createSVG` do not encode it yet (the fixture encoder is
the next DataBar Expanded PR).

### render(code | undefined, opts?) → Promise&lt;SVGSVGElement | null&gt;

Re-encodes a result as a standalone SVG (`{ module, height }` to tune
module size/height; style via `--barcode-ink` / `--barcode-paper`).
`undefined` or an empty result → `null`. Stateless — fire N in
parallel.

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
