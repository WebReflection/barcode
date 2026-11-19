# Committed regression fixtures

Unlike `test-codes/field/` (personal, gitignored), these are small,
non-sensitive specimens that a shipped bug was reproduced on. The e2e
suite scans them directly with `scan({ image })`; they must stay readable.

| file | expected | context |
| --- | --- | --- |
| `code32-aic-2026-11-19.jpeg` | `code32` / `A938261563` | iPhone screenshot of the docs scanner on an Italian medicine box. The printed `A938261563` is Code 32 (Italian Pharmacode): the raw Code 39 carrier is `VYTH1V`. The wide band was missed by the compact-cluster ROI; the local threshold also merged the thin spaces. Covers both the linear ROI and the Otsu ROI fallback. |
