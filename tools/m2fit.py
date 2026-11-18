"""M2 numeric-10 field layout: cluster+fit candidate models."""
import sys, json, random, itertools
from collections import defaultdict
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

random.seed(77)
_rend = {}
def vec(s):
    if s not in _rend:
        _rend[s] = {(x, y): render(s, level=1, mask=0, version=2)[y][x]
                    for y in range(13) for x in range(13)}
    return _rend[s]

FIXED = {(x, y) for x in range(13) for y in range(13)}
ref = vec('1111111111')
varying = set()
for t in range(40):
    s = ''.join(str(random.randrange(10)) for _ in range(10))
    m = vec(s)
    for c, v in m.items():
        if v != ref[c]: varying.add(c)
DATA = [c for c in [(x, y) for y in range(13) for x in range(13)] if c in varying]
IDX = {c: i for i, c in enumerate(DATA)}
N = len(DATA)
print(f"M2 varying cells: {N}")

def V(s): return tuple(vec(s)[c] for c in DATA)

pls = []
for t in range(150):
    p = ''.join(str(random.randrange(10)) for _ in range(10))
    if p not in pls: pls.append(p)

parent = {p: p for p in pls}
def find(p):
    while parent[p] != p: parent[p] = parent[parent[p]]; p = parent[p]
    return p
for a, b in itertools.combinations(pls, 2):
    if sum(x != y for x, y in zip(V(a), V(b))) <= 16:
        parent[find(a)] = find(b)
comp = defaultdict(list)
for p in pls: comp[find(p)].append(p)
comp = max(comp.values(), key=len)
print(f"largest component: {len(comp)}")

def bits_of(v, w): return [(v >> (w-1-i)) & 1 for i in range(w)]

MODELS = {
 '5pairs':  lambda d: sum([bits_of(d[2*i]*10+d[2*i+1], 7) for i in range(5)], []),
 '3+3+2+2': lambda d: bits_of(d[0]*100+d[1]*10+d[2], 10) + bits_of(d[3]*100+d[4]*10+d[5], 10)
                     + bits_of(d[6]*10+d[7], 7) + bits_of(d[8]*10+d[9], 7),
 '3+3+3+1': lambda d: bits_of(d[0]*100+d[1]*10+d[2], 10) + bits_of(d[3]*100+d[4]*10+d[5], 10)
                     + bits_of(d[6]*100+d[7]*10+d[8], 10) + bits_of(d[9], 4),
 '3+2+2+2+1': lambda d: bits_of(d[0]*100+d[1]*10+d[2], 10) + bits_of(d[3]*10+d[4], 7)
                     + bits_of(d[5]*10+d[6], 7) + bits_of(d[7]*10+d[8], 7) + bits_of(d[9], 4),
 'digits10': lambda d: sum([bits_of(d[i], 4) for i in range(10)], []),
}

def fit(model, comp):
    K = len(model([int(c) for c in comp[0]]))
    f0 = model([int(c) for c in comp[0]]); c0v = V(comp[0])
    eqs = []
    for p in comp[1:]:
        f = model([int(c) for c in p])
        df = [x ^ y for x, y in zip(f0, f)]
        if not any(df): continue
        eqs.append((df, [x ^ y for x, y in zip(c0v, V(p))]))
    aug = [list(df) + list(dc) for df, dc in eqs]
    r = 0; pivots = []
    for cidx in range(K):
        pr = next((rr for rr in range(r, len(aug)) if aug[rr][cidx]), None)
        if pr is None: continue
        aug[r], aug[pr] = aug[pr], aug[r]
        for rr in range(len(aug)):
            if rr != r and aug[rr][cidx]:
                aug[rr] = [x ^ y for x, y in zip(aug[rr], aug[r])]
        pivots.append(cidx); r += 1
    bad = sum(1 for rr in aug if not any(rr[:K]) and any(rr[K:]))
    Gs = [[0]*K for _ in range(N)]
    for ri, cidx in enumerate(pivots):
        for i in range(N): Gs[i][cidx] = aug[ri][K + i]
    return Gs, len(pivots), bad, K

for name, mf in MODELS.items():
    Gs, rank, bad, K = fit(mf, comp)
    print(f"{name:12}: K={K} rank={rank} bad={bad}")
    if bad == 0:
        json.dump({'model': name, 'G': Gs, 'DATA': [list(c) for c in DATA],
                   'component': comp}, open('/tmp/opencode/m2fit.json', 'w'))
        print("  -> SAVED")
