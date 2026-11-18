"""M2 numeric-10: per-field-bit column extraction via carry-free pair probes.
Tests model '5pairs' (M4-style): if true, all 35 pair-bit columns are
consistent across fillers; if the true layout is 3-groups, carries will
scatter the diff sets."""
import sys, json, random
from collections import defaultdict
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

random.seed(11)
_rend = {}
def vec(s):
    if s not in _rend:
        _rend[s] = {(x, y): render(s, level=1, mask=0, version=2)[y][x]
                    for y in range(13) for x in range(13)}
    return _rend[s]

def build(pairvals, filler_digits):
    """10 digits: pair i = pairvals[i] (or None -> filler digit pair)."""
    ds = []
    fi = 0
    for i in range(5):
        if pairvals[i] is None:
            ds += [filler_digits[2*i], filler_digits[2*i+1]]
        else:
            ds += [pairvals[i]//10, pairvals[i] % 10]
    return ''.join(map(str, ds))

DATA = None  # set after varying-cell scan below
def probe(pidx, bit, ntries=60):
    pv = 1 << bit
    votes = {}
    for t in range(ntries):
        v = random.randrange(0, 100 - pv)
        v &= ~((1 << (bit+1)) - 1)
        w = v + pv
        if w > 99: continue
        pv_ = [None]*5
        pv_[pidx] = v
        pw = [None]*5
        pw[pidx] = w
        filler = [random.randrange(10) for _ in range(10)]
        # filler digits at non-probed positions must be identical: build both from same filler
        dsA = []; dsB = []
        for i in range(5):
            fa, fb = filler[2*i], filler[2*i+1]
            if i == pidx:
                dsA += [v//10, v % 10]; dsB += [w//10, w % 10]
            else:
                dsA += [fa, fb]; dsB += [fa, fb]
        sA = ''.join(map(str, dsA)); sB = ''.join(map(str, dsB))
        try: va, vb = T(sA), T(sB)
        except Exception: continue
        d = sum(x != y for x, y in zip(va, vb))
        if d <= 42:
            c = tuple(x ^ y for x, y in zip(va, vb))
            votes[c] = votes.get(c, 0) + 1
    if not votes: return None, 0, 0
    best, bv = max(votes.items(), key=lambda kv: kv[1])
    return best, bv, sum(votes.values())

ref = vec('1111111111')
varying = set()
for t in range(30):
    s = ''.join(str(random.randrange(10)) for _ in range(10))
    m = vec(s)
    for c, v in m.items():
        if v != ref[c]: varying.add(c)
DATA = [c for c in [(x, y) for y in range(13) for x in range(13)] if c in varying]
def T(s): return tuple(vec(s)[c] for c in DATA)

results = {}
for pidx in range(5):
    for bit in range(7):
        c, votes, total = probe(pidx, bit)
        w = sum(c) if c else 0
        results[(pidx, bit)] = (c, votes, total)
        print(f"pair{pidx} bit{bit} (val {1<<bit:2}): {votes}/{total} votes, weight {w}")
json.dump({f"{k[0]},{k[1]}": [c for c in DATA] if False else
           [list(DATA[i]) for i in range(len(DATA)) if c[i]]
           for k, (c, v, t) in results.items() if c and v >= 6},
          open('/tmp/opencode/m2cols.json', 'w'))
print("saved")
