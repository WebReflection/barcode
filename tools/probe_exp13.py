"""DataBar Expanded probe 13: population study of 15-module blocks + AI chars.

For many payloads: segment [g2][chk17][S15][c][c][B]... and collect
 - the multiset of 15-module block width-vectors (are they table-driven?)
 - the first char (AI carrier) per AI value.
"""
import sys
import random
from collections import Counter

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def units(p):
    """Greedy unit segmentation after [g][chk][S]: 8-el chars, 5-el blocks.
    Returns (chars, blocks) with element widths; None on mismatch."""
    s = m(p)
    r = runs(s)
    if tuple(r[:2]) != (('0', 1), ('1', 1)) or tuple(r[10:15]) != START:
        return None
    i = 15
    end = len(r)
    chars, blocks = [], []
    while i < end:
        # try block first if remaining is exactly a block+guard or block+1
        rem = end - i
        if rem == 6 and sum(x for _, x in r[i:i+5]) == 15:
            blocks.append([x for _, x in r[i:i+5]])
            i += 5
            continue
        if rem < 8:
            break
        c = [x for _, x in r[i:i+8]]
        if sum(c) != 17:
            return None
        chars.append(c)
        i += 8
        if end - i >= 5 and sum(x for _, x in r[i:i+5]) == 15 and (end - i) % 17 != 0:
            blocks.append([x for _, x in r[i:i+5]])
            i += 5
    return chars, blocks, len(s)


random.seed(11)
blockcount = Counter()
aic1 = {}
ok = fail = 0
payloads = []
for ai in ["00", "01", "02", "04", "10", "11", "12", "13", "15", "16", "17",
           "20", "21", "22", "30", "37", "40", "80", "90", "91", "92", "95",
           "96", "97", "98", "99"]:
    payloads.append(f"[{ai}]" + "12345")
for _ in range(40):
    k = random.randrange(1, 15)
    d = "".join(random.choice("0123456789") for _ in range(k))
    payloads.append(f"[21]{d}")

for p in payloads:
    try:
        u = units(p)
    except Exception:
        continue
    if u is None:
        fail += 1
        continue
    ok += 1
    chars, blocks, mods = u
    for b in blocks:
        blockcount[tuple(b)] += 1
    ai = p[1:3]
    if ai not in aic1:
        aic1[ai] = tuple(chars[0])

print(f"segmented ok={ok} fail={fail}")
print(f"\n== distinct 15-module blocks: {len(blockcount)} ==")
for b, c in blockcount.most_common():
    print(f"   {list(b)}  x{c}")
print(f"\n== AI -> first char widths ==")
for ai in sorted(aic1):
    print(f"   {ai}: {list(aic1[ai])}")
