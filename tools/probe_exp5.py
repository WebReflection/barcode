"""DataBar Expanded probe 5: strict finder localization + segmentation.

Hypotheses to test:
  H1: symbol = [left guard 1+1][char1][finder L][chars...][finder R][...][right guard 1+1]
  H2: finders are 15 modules, 5 elements; L = space-first, R = bar-first,
      and R widths == reversed L widths (bidirectional decode).
  H3: characters have 8 elements (k=4).

Method: enumerate all 5-run windows with 15 modules passing the strict
ratio rule (w2+w3 in 10..12, w4+w5 == 2); report parity (space-first vs
bar-first) and whether the reverse appears elsewhere in the symbol.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def analyze(p):
    s = m(p)
    r = runs(s)
    w = [x for _, x in r]
    n = len(w)
    # module offset per run
    offs = [0] * n
    acc = 0
    for i, x in enumerate(w):
        offs[i] = acc
        acc += x
    print(f"== {p}  modules={len(s)} runs={n}")
    print(f"   head: {w[:4]}  tail: {w[-4:]}")
    cands = []
    for i in range(n - 4):
        ww = w[i:i+5]
        if sum(ww) != 15:
            continue
        if not (10 <= ww[1] + ww[2] <= 12):
            continue
        if ww[3] + ww[4] != 2:
            continue
        # reversed twin present?
        rev = ww[::-1]
        twin = any(w[j:j+5] == rev for j in range(n - 4))
        cands.append((i, offs[i], ww, 'L' if i % 2 == 0 else 'R', twin))
    for i, off, ww, par, twin in cands:
        print(f"   run{i:3d} mod{off:3d} {par}-first {ww} revtwin={twin}")
    return s, r


for p in ["[21]1", "[21]12345", "[01]95012345678903", "[17]261231",
          "[10]ABCDEFGHIJ", "[01]95012345678903[10]ABC123"]:
    analyze(p)
