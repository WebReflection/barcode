"""DataBar Expanded probe 18: check char = flat mod-211 table (like Limited's 89).

RSS-14 precedent (docs line 62): checksum = element widths of data chars, each
char read TOWARD its adjacent finder, global 3^k weights in reading order.
For Expanded chain [CHECK][START][c1][c2][B1][c3][c4][B2]...:
  c1 right of START -> read R->L (rev)   [matches AI-char reversal!]
  c2 left  of B1    -> read L->R
  c3 right of B1    -> read R->L
  ...
Test: f = sum 3^k e_k (mod 211) must collide EXACTLY when check vectors collide.
"""
import sys
import random
from collections import defaultdict

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))
GL = (('0', 1), ('1', 1))


def m(p):
    return svg_module_string(p, zint.BARCODE_RSS_EXP)


def units(p):
    """(check widths, [data char widths...]) or None."""
    s = m(p)
    r = runs(s)
    if tuple(r[:2]) != GL or tuple(r[10:15]) != START:
        return None
    chk = [x for _, x in r[2:10]]
    i = 15
    end = len(r)
    chars = []
    while i < end:
        rem = end - i
        if rem == 6 and sum(x for _, x in r[i:i + 5]) == 15:
            i += 5
            continue
        if rem < 8:
            break
        c = [x for _, x in r[i:i + 8]]
        if sum(c) != 17:
            return None
        chars.append(c)
        i += 8
        if end - i >= 5 and sum(x for _, x in r[i:i + 5]) == 15 and (end - i) % 17 != 0:
            i += 5
    return chk, chars


random.seed(42)
payloads = []
for k in range(1, 21):
    payloads.append('[21]' + '12345678901234567890'[:k])
for _ in range(400):
    k = random.randrange(1, 21)
    payloads.append('[21]' + ''.join(random.choice('0123456789') for _ in range(k)))

rows = []
for p in payloads:
    u = units(p)
    if u:
        rows.append((p, u[0], u[1]))
print(f'{len(rows)} payloads segmented')

def f_of(chars, mode, per_char, mod=211):
    """mode: 'alt' c1 R->L, c2 L->R,...; 'norev'; 'rev'."""
    tot = 0
    k = 0
    for idx, w in enumerate(chars):
        if mode == 'alt':
            ww = w[::-1] if idx % 2 == 0 else w
        elif mode == 'rev':
            ww = w[::-1]
        else:
            ww = w
        if per_char:
            tot += sum(3 ** j * x for j, x in enumerate(ww)) % mod
            k = 0
        else:
            for x in ww:
                tot += 3 ** k * x
                k += 1
    return tot % mod


for mode in ('alt', 'rev', 'norev'):
    for per_char in (False, True):
        by_f = defaultdict(set)
        by_v = defaultdict(set)
        for p, chk, chars in rows:
            f = f_of(chars, mode, per_char)
            v = tuple(chk)
            by_f[f].add(v)
            by_v[v].add(f)
        bad_f = sum(1 for s in by_f.values() if len(s) > 1)
        bad_v = sum(1 for s in by_v.values() if len(s) > 1)
        nv = len(by_v)
        print(f"mode={mode:6s} per_char={int(per_char)}: distinct vectors={nv} "
              f"(exp 211), f->multi-vector violations={bad_f}, "
              f"vector->multi-f violations={bad_v}")
