"""Probe 20: check char carries (c, length). Collision test keyed on (f, k).

If v_check = c + 211*j (j = length or similar), then two payloads with the
same checksum f AND same length k must have IDENTICAL check width vectors.
Distinct vectors <= 211 * #k_values.
"""
import sys
import random
from collections import defaultdict

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from exp_seg import chars_of

random.seed(42)
payloads = []
for k in range(1, 21):
    payloads.append('[21]' + '12345678901234567890'[:k])
for _ in range(400):
    k = random.randrange(1, 21)
    payloads.append('[21]' + ''.join(random.choice('0123456789') for _ in range(k)))

rows = []
for p in payloads:
    u = chars_of(p)
    if u:
        rows.append((p, len(p) - 4, u[0], u[1]))  # (payload, k, check, datachars)
print(f'{len(rows)} payloads')


def f_of(chars, mode, per_char, include_check=False, chk=None, mod=211):
    seq = []
    if include_check:
        seq.append(chk)
    for idx, w in enumerate(chars):
        if mode == 'alt':
            seq.append(w[::-1] if idx % 2 == 0 else w)
        elif mode == 'rev':
            seq.append(w[::-1])
        else:
            seq.append(w)
    tot = 0
    k = 0
    for w in seq:
        if per_char:
            tot += sum(3 ** j * x for j, x in enumerate(w)) % mod
        else:
            for x in w:
                tot += 3 ** k * x
                k += 1
    return tot % mod


for mode in ('alt', 'rev', 'norev'):
    for per_char in (False, True):
        for inc in (False, True):
            by = defaultdict(set)
            byv = defaultdict(set)
            for p, k, chk, chars in rows:
                f = f_of(chars, mode, per_char, inc, chk)
                v = tuple(chk)
                by[(f, k)].add(v)
                byv[v].add((f, k))
            bad = sum(1 for s in by.values() if len(s) > 1)
            bad2 = sum(1 for s in byv.values() if len(s) > 1)
            print(f"mode={mode:6s} per_char={int(per_char)} inc_chk={int(inc)}: "
                  f"(f,k)->multi-vector violations={bad}, "
                  f"vector->multi-(f,k) violations={bad2}")
