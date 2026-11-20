# Committed regression fixtures

Unlike `test-codes/field/` (personal, gitignored), these are small,
non-sensitive specimens that a shipped bug was reproduced on. The e2e
suite scans them directly with `scan({ image })`; every fixture must
either decode to its recorded value or be a documented hard negative
(used to bound latency).

| file | expected | context |
| --- | --- | --- |
| `code32-aic-2026-11-19.jpeg` | `code32` / `A938261563` | iPhone screenshot of the docs scanner on an Italian medicine box. The printed `A938261563` is Code 32 (Italian Pharmacode): the raw Code 39 carrier is `VYTH1V`. The wide band was missed by the compact-cluster ROI; the local threshold also merged the thin spaces. Covers both the linear ROI and the Otsu ROI fallback. |
| `gs1-datamatrix-box-2026-11-19.jpeg` | `datamatrix` / `901050883\x1d100006316163\x1d` (and `ean13` / `4006387122584`) | iPhone photo of a product box, shot 90° rotated, holding **both** an EAN-13 and a GS1 DataMatrix. Pinned four fixes: FNC1 in the first codeword is a mode flag (a GS1 DM used to decode to `None`); a crease cut the finder L into 6–29px pieces of an 88px leg (no corner formed); the low-contrast label only binarizes at the 0.7 threshold; and a still's linear fast-path used to hide every 2D tier (the scan returned EAN-13 and never asked the DataMatrix). The DataMatrix is more centered than the EAN-13, so the default "everything" scan resolves the DataMatrix. On a GPU phone the fixed-0.5 GPU floor force-inks this glossy label into noise; the still DataMatrix tier therefore re-reads it on a CPU mask (`STILL_CPU_FAMILIES` in `api/worker.js`), which takes the box from a >5s miss to ~0.6s. |
| `qr-hard-negative-2026-11-19.jpeg` | no read (hard negative) | The same box, a small QR on a crumpled/glossy surface that neither zxing nor this SDK reads at any rotation/upscale. It used to burn 23s in "everything" mode because the still latency was advisory. The deadline test pins the whole still scan to `latency` + a small fixed overshoot. No content assertion: a future decoder that reads it should flip this row to an expected value. |
