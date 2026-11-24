# Test tiers

## Synthetic + CI tier (no devices, no humans)
- `cargo test --workspace` — Rust decoders, round-trip + rejection.
- `tools/e2e/playwright.mjs` — starts Vite itself and drives the same pages
  on Chromium and WebKit (plus an iPhone-descriptor WebKit project):
  - `dist/` artifact smoke (`tests/artifact-test.html`, plain static hosting)
  - `tests/test.html` (synthetic round-trip, GPU/CPU)
  - `tests/contract-test.html` (worker transports, render-raw)
  - `tests/unit.html` (pure modules: reticle ranking + ROI geometry)
  - `tests/multicode.html` (one packed label photo, every code centered:
    the centered code must win, a surrounding one must never be returned)
  - `scan.html` live path with a fake camera (`fakecam-spec.mjs`)
  - the committed `docs/` site (landing, render round-trip, still sample,
    worker entries, bundled self-test on Chromium)
  - the local real-photo corpus when present (`tests/field.html`)
  Run: `npm test` (all projects + field) or `npm run test:ci` (CI).

  CI installs Playwright's Chromium + WebKit and runs `test:ci`; no camera
  permission or GUI session is needed because the fake camera replaces
  `getUserMedia` before the page modules run.

## Real-phone tier (CDP; the local best results)
- Android over USB: `adb reverse tcp:5173 tcp:5173`, then
  `node tools/e2e/fakecam.mjs --all --port 9222` (fake camera on the real
  Chrome), or `node tools/run-tests.mjs 9222 <url>` for any page.
- `node tools/e2e/field.mjs --port 9222 [--mode live]` replays the
  gitignored real-photo corpus through the published `dist/main.js`.
- iPhone: real Safari automation needs WebDriverAgent/Appium (not part of
  this repo). Use `npm run serve:iphone` for a manual device session over
  LAN HTTPS with a trusted mkcert CA; the automated `iphone` Playwright
  project (WebKit + iPhone descriptor + same fake camera) is the CI proxy
  for the iOS engine.

The physical phone rig remains the authority for camera hardware
questions (focus, glare, fps); CI pins the software flow so regressions
in it are caught on every push.
