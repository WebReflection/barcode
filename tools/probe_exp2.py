"""DataBar Expanded probe 2: locate fixed blocks + first-char position."""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def diff(a: str, b: str) -> str:
    """Common prefix/suffix summary + differing regions."""
    n = min(len(a), len(b))
    p = 0
    while p < n and a[p] == b[p]:
        p += 1
    sfx = 0
    while sfx < n - p and a[len(a)-1-sfx] == b[len(b)-1-sfx]:
        sfx += 1
    return f"prefix={p} suffix={sfx} lens={len(a)},{len(b)}"


print("== k=1 vs k=6 (both 102) ==")
a, b = m("[21]1"), m("[21]111111")
print(a)
print(b)
print(diff(a, b))

print("\n== k=1 vs k=2 (both 102) ==")
a, b = m("[21]1"), m("[21]12")
print(a)
print(b)
print(diff(a, b))

print("\n== k=6 vs k=7 (102 vs 133) ==")
a, b = m("[21]111111"), m("[21]1111111")
print(a)
print(b)
print(diff(a, b))

print("\n== where is the 111111110000101 block in each k? ==")
for k in range(1, 21):
    try:
        s = m(f"[21]{'1'*k}")
        i = s.find("111111110000101")
        print(f"k={k:2d} modules={len(s):3d} finder_at={i}")
    except Exception:
        pass
