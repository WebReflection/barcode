# Contributing

## Generated output stays out of feature PRs

`docs/` is the generated GitHub Pages site. `npm run build:docs` emits content-hashed
filenames under `docs/assets/`, so **every rebuild rewrites unrelated filenames**. A
feature PR that includes `docs/` therefore turns every other open PR into a merge
conflict, and the generated diff is noise for review.

Rules:

- **Feature/bug PRs must not change `docs/`.** Keep them to source, tests, and docs
  prose that is not generated (for example this file).
- After the logical changes land on `main`, the maintainer runs `npm run build:docs`
  and lands the generated site in a dedicated **docs-rebuild** PR, or directly on
  `main`.
- npm modules and wasm release artifacts are also produced from `main` by the
  maintainer, not from feature branches.

CI enforces this with the **docs guard** workflow: a PR that touches `docs/` fails
unless it carries the `docs-rebuild` label. The label is the maintainer's explicit
opt-in for a generated-docs PR; do not add it to feature work.

## After a release

Publishing is the maintainer's action. Once npm reports the new version, run
the published-artifact smoke from `packages/web`:

```sh
npm run test:live
```

It checks npm `dist-tags.latest`, the four jsDelivr `dist/` files, the
deployed Pages badge and worker shim, and renders one symbol through the CDN
build in headless Chromium. The same check runs nightly in the `live`
workflow; a red run right after a publish is usually propagation, so re-run
before investigating.

## Local-only development data

`tools/` holds the probing scripts used to derive the clean-room implementations.
Some of their inputs are **not** committed, because they are large and only useful
to re-run a historical derivation:

- `tools/m[0-9]*.json` — the Micro QR oracle extraction dumps behind
  `tools/gen_frames.py` / `tools/gen_microqr.py`. The committed test vectors are
  `crates/barcode-microqr/tests/golden_*.txt`; these dumps stay on the maintainer's
  disk.
- `packages/web/test-codes/field/` — the personal real-photo corpus.

Both are gitignored. Missing files are expected in a fresh clone; nothing in the
build or test suite reads them.

## Before opening a PR

- `cargo test --workspace` (CI runs it with `--locked`; `--release` if you want
  the optimized sweep)
- `npm run check` in `packages/web`
- `npm run build` in `packages/web` — it also regenerates `docs/`; that output is
  generated, so never commit it in this PR (see above)
- the relevant browser gate (`node tools/e2e/playwright.mjs --ci --browser chromium`
  for the full suite)
