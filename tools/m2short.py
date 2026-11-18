"""M2 len 8/9: extract G (3+3+3 / 3+3+2), residuals, match mask maps, read count."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

random.seed(41)
_rend = {}
def vec(s, L):
    if (s, L) not in _rend:
        _rend[(s, L)] = {(x, y): render(s, level=1, mask=0, version=2)[y][x]
                         for y in range(13) for x in range(13)}
    return _rend[(s, L)]

def varying_cells(L, ref):
    vary = set()
    for t in range(30):
        s = ''.join(str(random.randrange(10)) for _ in range(L))
        m = vec(s, L)
        for c, v in m.items():
            if v != ref[c]: vary.add(c)
    return [c for c in [(x, y) for y in range(13) for x in range(13)] if c in vary]

def extract(L, FIELDS, ref):
    DATA = varying_cells(L, ref)
    IDX = {c: i for i, c in enumerate(DATA)}
    N = len(DATA)
    def T(s): return tuple(vec(s, L)[c] for c in DATA)
    def fval(d, f):
        off, n, w = FIELDS[f]; v = 0
        for i in range(n): v = v * 10 + d[off + i]
        return v
    def setf(d, f, v):
        off, n, w = FIELDS[f]
        for i in range(n): d[off + n - 1 - i] = v % 10; v //= 10
    def nbits(f): return FIELDS[f][2]
    def probe(f, bit, ntries=70, thr=45):
        pv = 1 << bit; votes = {}
        for t in range(ntries):
            v = random.randrange(0, 10 ** FIELDS[f][1])
            v &= ~((1 << (bit + 1)) - 1)
            w = v + pv
            if w >= 10 ** FIELDS[f][1]: continue
            dA = [random.randrange(10) for _ in range(L)]
            dB = list(dA)
            setf(dA, f, v); setf(dB, f, w)
            sA = ''.join(map(str, dA)); sB = ''.join(map(str, dB))
            try: va, vb = T(sA), T(sB)
            except Exception: continue
            if sum(x != y for x, y in zip(va, vb)) <= thr:
                key = tuple(x ^ y for x, y in zip(va, vb))
                votes[key] = votes.get(key, 0) + 1
        if not votes: return None, 0, 0
        best, bv = max(votes.items(), key=lambda kv: kv[1])
        return best, bv, sum(votes.values())
    G = {}
    for f in range(len(FIELDS)):
        for bit in range(nbits(f)):
            c, votes, total = probe(f, bit)
            if c and votes >= 6:
                col = [0] * N
                for i in range(N):
                    if c[i]: col[i] = 1
                G[(f, bit)] = col
    K = sum(nbits(f) for f in range(len(FIELDS)))
    print(f"len {L}: {N} cells, {len(G)}/{K} columns", flush=True)
    return DATA, T, fval, G, K

def residuals(L, anchor, DATA, T, fval, G, K, FIELDS):
    N = len(DATA)
    def fmodel(d):
        out = []
        for f in range(len(FIELDS)):
            v = fval(d, f); w = FIELDS[f][2]
            out += [(v >> i) & 1 for i in range(w)]
        return out
    d0 = [int(c) for c in anchor]
    f0 = fmodel(d0); c0 = T(anchor)
    def sf(j):
        n = 0
        for fi in range(len(FIELDS)):
            w = FIELDS[fi][2]
            if j < n + w: return (fi, j - n)
            n += w
    res = {}
    for t in range(130):
        s = ''.join(str(random.randrange(10)) for _ in range(L))
        d = [x ^ y for x, y in zip(fmodel([int(c) for c in s]), f0)]
        pred = list(c0)
        for j in range(K):
            if d[j]:
                fi, bi = sf(j)
                if (fi, bi) not in G: pred = None; break
                for i in range(N): pred[i] ^= G[(fi, bi)][i]
        if pred is None: continue
        r = tuple(x ^ y for x, y in zip(T(s), pred))
        res.setdefault(r, []).append(s)
    return res

ref10 = vec('1234567890', 10)
# len 9: 3+3+3
D9, T9, fv9, G9, K9 = extract(9, [(0, 3, 10), (3, 3, 10), (6, 3, 10)], ref10)
r9 = residuals(9, '123456789', D9, T9, fv9, G9, K9, [(0, 3, 10), (3, 3, 10), (6, 3, 10)])
print(f"len 9 residual classes: {len(r9)} weights {sorted((sum(k) for k in r9), reverse=True)[:8]}")
# len 8: 3+3+2
D8, T8, fv8, G8, K8 = extract(8, [(0, 3, 10), (3, 3, 10), (6, 2, 7)], ref10)
r8 = residuals(8, '12345678', D8, T8, fv8, G8, K8, [(0, 3, 10), (3, 3, 10), (6, 2, 7)])
print(f"len 8 residual classes: {len(r8)} weights {sorted((sum(k) for k in r8), reverse=True)[:8]}")
json.dump({'G9': {f"{f},{b}": c for (f, b), c in G9.items()}, 'D9': [list(c) for c in D9],
           'G8': {f"{f},{b}": c for (f, b), c in G8.items()}, 'D8': [list(c) for c in D8]},
          open('/tmp/opencode/m2short.json', 'w'))
