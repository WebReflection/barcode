"""DataBar Expanded probe 6: is region 2-18 the CHECK character?

If the symbol = [guard][check 17 mod][start pattern 15 mod][data...] then
changing the LAST data digit changes the head region; changing the FIRST
data digit leaves the head fixed (only later regions move).

Also test: is the (1,8,4,1,1) start pattern really constant everywhere?
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

PAT = "1111111100001010"


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def diffmap(a, b):
    n = min(len(a), len(b))
    regions = []
    i = 0
    while i < n:
        if a[i] != b[i]:
            j = i
            while j < n and a[j] != b[j]:
                j += 1
            regions.append((i, j))
            i = j
        else:
            i += 1
    return regions


base = m("[21]12345")
print("base [21]12345:", base)

print("\n-- change LAST digit: [21]12346 --")
b = m("[21]12346")
print(b)
print("diff regions:", diffmap(base, b))

print("\n-- change FIRST data digit: [21]22345 --")
b = m("[21]22345")
print(b)
print("diff regions:", diffmap(base, b))

print("\n-- change 2nd data digit: [21]13345 --")
b = m("[21]13345")
print(b)
print("diff regions:", diffmap(base, b))

print("\n-- start pattern constancy over random payloads --")
import random
random.seed(7)
ok = 0
tot = 0
for _ in range(30):
    k = random.randrange(1, 21)
    p = "[21]" + "".join(random.choice("0123456789") for _ in range(k))
    try:
        s = m(p)
    except Exception:
        continue
    tot += 1
    i = s.find("111111110000101")
    if i == 19:
        ok += 1
    else:
        print("  MISS", p, i, len(s))
print(f"start-pattern at 19: {ok}/{tot}")
