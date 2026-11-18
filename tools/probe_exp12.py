"""DataBar Expanded probe 12: (17,4) character enumeration + c2/c3 dump.

Step 1: enumerate all (17,4)-style patterns: 4 bars + 4 spaces, 17 modules,
elements >= 1, adjacent (bar,space) pair sums <= 9. Count them and organize
by (bars_sum, spaces_sum) with the widest-pair rule wo+we = 9.
Step 2: dump the c2/c3 element widths for the d-variants of [21]12345.
"""
import sys
from functools import lru_cache

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


@lru_cache(maxsize=None)
def comp(k, t, w):
    """#compositions of t into k parts in [1,w]."""
    if k == 0:
        return 1 if t == 0 else 0
    if t < k or t > k * w:
        return 0
    return sum(comp(k - 1, t - p, w) for p in range(1, min(w, t) + 1))


# (17,4): bars (4) + spaces (4) = 17 modules. Enumerate group splits
# (bars_sum b, spaces_sum s), b+s=17, with widest (wb, ws), wb+ws = 9,
# wb >= ceil(b/4), ws >= ceil(s/4).
print("== candidate groups for (17,4), wo+we=9 ==")
total_all = 0
for b in range(4, 14):
    s = 17 - b
    if s < 4:
        continue
    for wb in range(1, 9):
        ws = 9 - wb
        if wb * 4 < b or ws * 4 < s:
            continue
        t = comp(4, b, wb) * comp(4, s, ws)
        if t:
            print(f"  bars={b:2d} spaces={s:2d} widest=({wb},{ws}) count={t}")
        total_all += t
print("sum over all candidate groups:", total_all)


def slots(p):
    """return dict of slot -> element widths for a 102-module symbol."""
    s = m(p)
    r = runs(s)
    assert len(s) == 102
    def el(a, b):
        return [x for _, x in r[a:b]]
    return {"chk": el(2, 10), "c1": el(15, 23), "c2": el(23, 31),
            "B": el(31, 36), "c3": el(36, 44)}


base = slots("[21]12345")
print("\n== c2/c3 for digit variants (items 2,1 | d1..d5 | F) ==")
print("base [21]12345 c2=", base["c2"], " c3=", base["c3"], " B=", base["B"])
for p in ["[21]92345", "[21]19345", "[21]12945", "[21]12395", "[21]12349"]:
    a = slots(p)
    print(f"{p:12s} c2={a['c2']} c3={a['c3']} B={a['B']}")
