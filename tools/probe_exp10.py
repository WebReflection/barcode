"""DataBar Expanded probe 10: which slot encodes which item?

102-module symbols have known slots:
  chk @2-18, c1 @34-50, c2 @51-67, F @68-82, c3 @83-99 (modules)
Vary one payload item at a time; record which slot regions change.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

SLOTS = [("chk", 2, 19), ("c1", 34, 51), ("c2", 51, 68), ("F", 68, 83), ("c3", 83, 100)]


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def diffregions(a, b):
    n = min(len(a), len(b))
    out = []
    i = 0
    while i < n:
        if a[i] != b[i]:
            j = i
            while j < n and a[j] != b[j]:
                j += 1
            out.append((i, j))
            i = j
        else:
            i += 1
    return out


def slotsof(regions):
    hits = set()
    for lo, hi in regions:
        for name, a, b in SLOTS:
            if lo < b and hi > a:
                hits.add(name)
    return sorted(hits)


base = m("[21]12345")
print("base [21]12345; items: 2,1,1,2,3,4,5[,F]; pairs (2,1)(1,2)(3,4)(5,F)?")
cases = [
    ("[21]92345", "data d1 1->9"),
    ("[21]19345", "data d2 2->9"),
    ("[21]12945", "data d3 3->9"),
    ("[21]12395", "data d4 4->9"),
    ("[21]12349", "data d5 5->9"),
]
for p, label in cases:
    b = m(p)
    if len(b) != len(base):
        print(f"{label:16s} {p}: LENGTH {len(b)} != 102, skipped")
        continue
    regs = diffregions(base, b)
    print(f"{label:16s} {p}: regions={regs} slots={slotsof(regs)}")
