"""DataBar Expanded probe 4: locate ALL finder-pattern candidates.

RSS-14 finder rule (sample text): 5 elements, 15 modules; sum(e2+e3) in
10..12, sum(e4+e5) = 2. For Expanded the exact table is unknown, so use a
looser shape test: 5 consecutive runs, 15 modules total.

Also: segment into 8-element characters between finders and report widths.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def finders(r):
    """All 5-run windows summing to 15 modules; return (start_run, widths)."""
    out = []
    w = [x for _, x in r]
    for i in range(len(w) - 4):
        if sum(w[i:i+5]) == 15:
            out.append((i, w[i:i+5]))
    return out


for p in ["[21]1", "[21]12345", "[01]95012345678903", "[17]261231",
          "[10]ABC", "[10]ABCDEFGHIJ", "[01]95012345678903[10]ABC123"]:
    s = m(p)
    r = runs(s)
    fs = finders(r)
    print(f"{p:34s} modules={len(s):3d} runs={len(r):2d} finder_windows={len(fs)}")
    for i, w in fs:
        # module offset of run i
        off = sum(x for _, x in r[:i])
        print(f"    run{i:3d} mod{off:3d}: {w}")
