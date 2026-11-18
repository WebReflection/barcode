"""Recover all 72 columns of the affine map using any-law renders.

col_j = ecc([base,j]) xor ecc([base])  (differences cancel the affine
constant C and the header). Uses only the trusted ecc prefix [0:6]
(stream 78-126, x>=9, verified territory for all four laws).
"""
import sys, json
sys.path.insert(0, "/tmp/opencode")
from bprobe import ecc_of

PREF = 6   # trusted ecc CWs

def col(base, j):
    e1 = ecc_of(set(base)); e2 = ecc_of(set(base) ^ {j})
    if e1 is None or e2 is None: return None
    return [x ^ y for x, y in zip(e1[:PREF], e2[:PREF])]

cols = {}
b0, b1 = 1, 9   # two base bits so col(b0) and col(b1) both obtainable
for j in range(72):
    if j != b0:
        c = col({b0}, j)
        if c: cols[j] = c
    if j != b1:
        c = col({b1}, j)
        if c: cols.setdefault(j, c)
print(f"{len(cols)}/72 columns")
missing = sorted(set(range(72)) - set(cols))
print("missing:", missing)

# affinity cross-check: col from two independent bases must agree
agree = dis = 0
for j in range(72):
    if j in (b0, b1): continue
    c1 = col({b0}, j); c2 = col({b1}, j)
    if c1 and c2:
        if c1 == c2: agree += 1
        else: dis += 1
print(f"affinity cross-check: {agree} agree, {dis} disagree")

json.dump({str(k): v for k, v in cols.items()}, open('/tmp/opencode/cols_any.json','w'))
