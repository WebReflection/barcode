"""M2 encoder: per-length (G, K-class) tables for len 8/9/10, validated vs zint.

K_p = obs ^ G.f for each render; distinct K_p = distinct mask classes.
"""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

h = json.load(open('/tmp/opencode/m2h.json'))
sh = json.load(open('/tmp/opencode/m2short.json'))

FIELDS = {10: [(0,3,10),(3,3,10),(6,3,10),(9,1,4)],
          9: [(0,3,10),(3,3,10),(6,3,10)],
          8: [(0,3,10),(3,3,10),(6,2,7)]}
GD = {10: h['G'], 9: sh['G9'], 8: sh['G8']}
DD = {10: [tuple(c) for c in h['DATA']], 9: [tuple(c) for c in sh['D9']], 8: [tuple(c) for c in sh['D8']]}

def gmat(L):
    """G as matrix: col index = flattened (field, exponent) in FIELDS order."""
    Gd = GD[L]; N = len(DD[L])
    cols = []
    for fi, (off, n, w) in enumerate(FIELDS[L]):
        for e in range(w):
            col = Gd.get(f"{fi},{e}")
            cols.append(col)
    return [[cols[j][i] if cols[j] else 0 for j in range(len(cols))] for i in range(N)]

def model_f(L, payload):
    d = [int(c) for c in payload]
    out = []
    for off, n, w in FIELDS[L]:
        v = 0
        for i in range(n): v = v*10 + d[off+i]
        out += [(v >> e) & 1 for e in range(w)]   # index = exponent
    return out

def obs(s, L):
    m = render(s, level=1, mask=0, version=2)
    return [m[y][x] for x, y in DD[L]]

def cells_of(G, f, K):
    return [sum(G[i][j] for j in range(len(f)) if f[j]) % 2 ^ K[i] for i in range(len(K))]

random.seed(5)
KT = {}
for L in (8, 9, 10):
    G = gmat(L); N = len(DD[L])
    Ks = {}
    for t in range(45):
        p = ''.join(str(random.randrange(10)) for _ in range(L))
        f = model_f(L, p)
        K = [o ^ sum(G[i][j] for j in range(len(f)) if f[j]) % 2
             for i, o in enumerate(obs(p, L))]
        Ks[tuple(K)] = Ks.get(tuple(K), 0) + 1
    KT[L] = list(Ks)
    print(f"len {L}: {len(Ks)} distinct mask classes (freqs {sorted(Ks.values(), reverse=True)})")

# validate
random.seed(888)
pls = []
for t in range(500):
    L = random.choice((8, 9, 10))
    p = ''.join(str(random.randrange(10)) for _ in range(L))
    if p not in pls: pls.append(p)
ok = miss = 0
stat = {}
for p in pls:
    L = len(p); G = gmat(L); f = model_f(L, p)
    m = render(p, level=1, mask=0, version=2)
    z = [m[y][x] for x, y in DD[L]]
    hit = None
    for mi, K in enumerate(KT[L]):
        e = cells_of(G, f, K)
        if e == z: hit = mi; break
    stat.setdefault(L, [0, 0])
    if hit is None: stat[L][0] += 1; miss += 1
    else: stat[L][1] += 1; ok += 1
print(f"M2 encoder: {ok} ok, {miss} miss of {len(pls)}")
for L in sorted(stat): print(f"  len {L}: {stat[L][1]} ok / {stat[L][0]} miss")
json.dump({str(L): {'G': GD[L], 'DATA': [list(c) for c in DD[L]],
                    'K': [list(k) for k in KT[L]]} for L in KT},
          open('/tmp/opencode/m2table.json', 'w'))
print("saved m2table.json")
