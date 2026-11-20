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

## Before opening a PR

- `cargo test --workspace` (CI runs it with `--locked`; `--release` if you want
  the optimized sweep)
- `npm run check` in `packages/web`
- `npm run build` in `packages/web` — it also regenerates `docs/`; that output is
  generated, so never commit it in this PR (see above)
- the relevant browser gate (`node tools/e2e/playwright.mjs --ci --browser chromium`
  for the full suite)
