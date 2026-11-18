"""DataBar Expanded probe 14: block value vs structure position.

For each payload: (nchars, [block canonical values in order]) where a
block is canonicalized to space-first (reverse if bar-first).
"""
import sys
import random

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def units(p):
    s = m(p)
    r = runs(s)
    if tuple(r[:2]) != (('0', 1), ('1', 1)) or tuple(r[10:15]) != START:
        return None
    i = 15
    end = len(r)
    chars, blocks, pols = [], [], []
    while i < end:
        rem = end - i
        if rem == 6 and sum(x for _, x in r[i:i+5]) == 15:
            blocks.append(tuple(x for _, x in r[i:i+5]))
            pols.append(r[i][0])
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
            blocks.append(tuple(x for _, x in r[i:i+5]))
            pols.append(r[i][0])
            i += 5
    return len(s), chars, blocks, pols, i, end


def canon(block, first_polarity):
    """canonical space-first form"""
    w = list(block)
    if first_polarity == '1':  # bar-first -> reverse
        return tuple(reversed(w))
    return tuple(w)


random.seed(5)
tests = []
for k in [1, 3, 6, 7, 10, 11, 13, 14, 17, 18, 20]:
    tests.append(f"[21]" + "12345678901234567890"[:k])
for _ in range(25):
    k = random.randrange(1, 21)
    d = "".join(random.choice("0123456789") for _ in range(k))
    tests.append(f"[21]{d}")

from collections import Counter
sig = Counter()
for p in tests:
    try:
        u = units(p)
    except Exception:
        continue
    if u is None:
        print(f"{p:26s} SEGFAIL")
        continue
    mods, chars, blocks, pols, i, end = u
    cb = [canon(b, q) for b, q in zip(blocks, pols)]
    tail = end - i
    print(f"{p:26s} mods={mods:3d} nchars={len(chars)} blocks={[list(b) for b in cb]} leftover_runs={tail}")
    sig[(len(chars), tuple(cb))] += 1

print("\n== structure signatures ==")
for k, v in sorted(sig.items()):
    print(f"   nchars={k[0]} blocks={k[1]} x{v}")
