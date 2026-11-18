"""M2 numeric encoder, ALL lengths 1-10: extract G, collect K classes, validate.

Chunking rule (M1-style): 3-digit groups (10b) while >=3 remain, then
remainder: 2 -> 7b, 1 -> 4b. Bits indexed by exponent; write order MSB-first.
"""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

random.seed(11)

def fields_of(L):
    F, rem = [], L
    while rem >= 3:
        F.append((3, 10)); rem -= 3
    if rem == 2: F.append((2, 7))
    elif rem == 1: F.append((1, 4))
    return F

def model_f(L, payload):
    d = [int(c) for c in payload]; out = []; off = 0
    for n, w in fields_of(L):
        v = 0
        for i in range(n): v = v*10 + d[off+i]
        out += [(v >> e) & 1 for e in range(w)]; off += n
    return out

def vec(s, L, cache={}):
    if (s, L) not in cache:
        m = render(s, level=1, mask=0, version=2)
        cache[(s, L)] = {(x, y): m[y][x] for y in range(13) for x in range(13)}
    return cache[(s, L)]

ALL = [(x, y) for y in range(13) for x in range(13)]
res = {}
for L in range(1, 11):
    ref = vec('1'*L, L)
    varying = set()
    for t in range(60):
        s = ''.join(str(random.randrange(10)) for _ in range(L))
        m = vec(s, L)
        for c, v in m.items():
            if v != ref[c]: varying.add(c)
    DATA = [c for c in ALL if c in varying]; DSET = set(DATA); N = len(DATA)
    IDX = {c: i for i, c in enumerate(DATA)}
    def T(s): return tuple(vec(s, L)[c] for c in DATA)
    F = fields_of(L); K = sum(w for _, w in F)
    def setd(d, fi, v):
        off = sum(n for n, _ in F[:fi]); n, w = F[fi]
        for i in range(n): d[off+n-1-i] = v % 10; v //= 10
    def probe(fi, e, ntries=220, thr=52):
        pv = 1 << e; votes = {}
        for t in range(ntries):
            v = random.randrange(0, 10**F[fi][0]); v &= ~((1 << (e+1)) - 1)
            wv = v + pv
            if wv >= 10**F[fi][0]: continue
            dA = [random.randrange(10) for _ in range(L)]; dB = list(dA)
            setd(dA, fi, v); setd(dB, fi, wv)
            sA = ''.join(map(str, dA)); sB = ''.join(map(str, dB))
            va, vb = T(sA), T(sB)
            if sum(x != y for x, y in zip(va, vb)) <= thr:
                key = tuple(x ^ y for x, y in zip(va, vb))
                votes[key] = votes.get(key, 0) + 1
        if not votes: return None, 0
        return max(votes.items(), key=lambda kv: kv[1])
    G = {}
    weak = 0
    for fi in range(len(F)):
        for e in range(F[fi][1]):
            c, votes = probe(fi, e)
            if c and votes >= 6:
                col = [0]*N
                for i in range(N):
                    if c[i]: col[i] = 1
                G[(fi, e)] = col
            else:
                weak += 1
                G[(fi, e)] = [0]*N
    # matrix + K collection
    def gmat():
        cols = [G[(fi, e)] for fi in range(len(F)) for e in range(F[fi][1])]
        return [[cols[j][i] for j in range(K)] for i in range(N)]
    GM = gmat()
    def obs(s): return [vec(s, L)[c] for c in DATA]
    Ks = {}
    for t in range(160):
        p = ''.join(str(random.randrange(10)) for _ in range(L))
        f = model_f(L, p)
        o = obs(p)
        Kv = tuple(o[i] ^ sum(GM[i][j] for j in range(K) if f[j]) % 2 for i in range(N))
        Ks[Kv] = Ks.get(Kv, 0) + 1
    # validate
    ok = miss = 0
    for t in range(90):
        p = ''.join(str(random.randrange(10)) for _ in range(L))
        f = model_f(L, p)
        z = obs(p)
        hit = any([sum(GM[i][j] for j in range(K) if f[j]) % 2 ^ Kv[i] for i in range(N)] == z
                  for Kv in Ks)
        if hit: ok += 1
        else: miss += 1
    print(f"len {L}: {N} cells, {K-weak}/{K} cols strong, {len(Ks)} classes, valid {ok}/{ok+miss}", flush=True)
    res[L] = {'DATA': [list(c) for c in DATA], 'K': K, 'fields': F,
              'G': {f"{fi},{e}": G[(fi, e)] for (fi, e) in G},
              'Ks': [list(k) for k in Ks]}

json.dump(res, open('/tmp/opencode/m2all.json', 'w'))
print("saved m2all.json")
