"""Recover the affine map ecc = L(payload72) XOR C using difference columns
from row-even 9-byte renders. Verify on held-out payloads."""
import sys, random, json
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of

def payload(setbits):
    b = ['\x00']*9
    for j in setbits: b[j//8] = chr(ord(b[j//8]) | (1 << (7 - j%8)))
    return ''.join(b)

def ecc_of(setbits):
    name, t = stream_law(payload(setbits), 3)
    if name != 'row-even': return None
    return cws_of(t, 78, 14)

random.seed(7)
bases = [set() ]
# collect renders: random subsets
pools = {}
cnt = 0
for trial in range(400):
    S = set(random.sample(range(72), random.randint(1, 20)))
    e = ecc_of(S)
    if e: pools[frozenset(S)] = e; cnt += 1
print(f"{cnt} row-even renders collected")

# columns via differences
cols = {}
for S, e in pools.items():
    for j in range(72):
        S2 = frozenset(S ^ {j})
        if S2 in pools:
            col = [x^y for x,y in zip(e, pools[S2])]
            if j in cols:
                if cols[j] != col: print(f"INCONSISTENT col {j}")
            else: cols[j] = col
print(f"{len(cols)}/72 columns recovered")
json.dump({str(k): v for k,v in cols.items()}, open('/tmp/opencode/cols2.json','w'))

# verify affinity on held-out pairs: ecc(A xor-symmetric-diff) etc.
# test: ecc(S1) xor ecc(S2) == xor of cols over symmetric difference
ok = bad = 0
items = list(pools.items())
for (S1,e1),(S2,e2) in zip(items[::2], items[1::2]):
    diff = S1 ^ S2
    if not all(j in cols for j in diff): continue
    pred = [x^y for x,y in zip(e1,e2)]
    real = [0]*14
    for j in diff:
        for k in range(14): real[k] ^= cols[j][k]
    if pred == real: ok += 1
    else: bad += 1
print(f"affinity check: {ok} ok, {bad} bad")
