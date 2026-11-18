"""M2 header hunt: per-length constant-pred cells; count bits via cross-length diffs."""
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

ALL = [(x, y) for y in range(13) for x in range(13)]
h = json.load(open('/tmp/opencode/m2h.json'))
sh = json.load(open('/tmp/opencode/m2short.json'))

def analyze(L, anchor, DATAj, Gj, FIELDS, n=12):
    DATA = [tuple(c) for c in DATAj]; N = len(DATA)
    def T(s): return tuple(vec(s, L)[c] for c in DATA)
    def fval(d, f):
        off, nn, w = FIELDS[f]; v = 0
        for i in range(nn): v = v * 10 + d[off + i]
        return v
    G = {}
    for k, col in Gj.items():
        f, b = map(int, k.split(',')); G[(f, b)] = col
    K = sum(F[2] for F in FIELDS)
    def fmodel(d):
        out = []
        for f in range(len(FIELDS)):
            v = fval(d, f); w = FIELDS[f][2]
            out += [(v >> i) & 1 for i in range(w)]
        return out
    d0 = [int(c) for c in anchor]; f0 = fmodel(d0); c0 = T(anchor)
    def sf(j):
        n = 0
        for fi in range(len(FIELDS)):
            w = FIELDS[fi][2]
            if j < n + w: return (fi, j - n)
            n += w
    # per-cell set of pred values across renders
    pvals = {c: set() for c in ALL}
    for t in range(n):
        s = ''.join(str(random.randrange(10)) for _ in range(L))
        d = [x ^ y for x, y in zip(fmodel([int(c) for c in s]), f0)]
        pred = {c: c0[i] for i, c in enumerate(DATA)}
        for j in range(K):
            if d[j]:
                fi, bi = sf(j)
                col = G[(fi, bi)]
                for i, c in enumerate(DATA):
                    if col[i]: pred[c] ^= 1
        for c in ALL:
            pvals[c].add(pred.get(c))
    const = {c: next(iter(v)) for c, v in pvals.items() if len(v) == 1 and None not in v}
    return const

F10 = [(0, 3, 10), (3, 3, 10), (6, 3, 10), (9, 1, 4)]
F9 = [(0, 3, 10), (3, 3, 10), (6, 3, 10)]
F8 = [(0, 3, 10), (3, 3, 10), (6, 2, 7)]
P10 = analyze(10, '1234567890', h['DATA'], h['G'], F10)
P9 = analyze(9, '123456789', sh['D9'], sh['G9'], F9)
P8 = analyze(8, '12345678', sh['D8'], sh['G8'], F8)
print(f"constant-pred cells: len10 {len(P10)}, len9 {len(P9)}, len8 {len(P8)}")
allc = set(P10) & set(P9) & set(P8)
b0 = [c for c in allc if P8[c] != P9[c] and P8[c] == P10[c]]
b1 = [c for c in allc if P8[c] == P9[c] and P8[c] != P10[c]]
b23 = [c for c in allc if P8[c] == P9[c] == P10[c]]
print("count bit0 (8=0,9=1,10=0):", sorted(b0))
print("count bit1 (8=0,9=0,10=1):", sorted(b1))
print("const across 8/9/10:", sorted(b23))

# --- sigma-corrected count extraction ---
import itertools
def pred_all(L, anchor, DATAj, Gj, FIELDS, n=12):
    DATA = [tuple(c) for c in DATAj]; N = len(DATA)
    def T(s): return tuple(vec(s, L)[c] for c in DATA)
    def fval(d, f):
        off, nn, w = FIELDS[f]; v = 0
        for i in range(nn): v = v * 10 + d[off + i]
        return v
    G = {}
    for k, col in Gj.items():
        f, b = map(int, k.split(',')); G[(f, b)] = col
    K = sum(F[2] for F in FIELDS)
    def fmodel(d):
        out = []
        for f in range(len(FIELDS)):
            v = fval(d, f); w = FIELDS[f][2]
            out += [(v >> i) & 1 for i in range(w)]
        return out
    d0 = [int(c) for c in anchor]; f0 = fmodel(d0); c0 = T(anchor)
    def sf(j):
        n = 0
        for fi in range(len(FIELDS)):
            w = FIELDS[fi][2]
            if j < n + w: return (fi, j - n)
            n += w
    P = {}
    for t in range(n):
        s = ''.join(str(random.randrange(10)) for _ in range(L))
        d = [x ^ y for x, y in zip(fmodel([int(c) for c in s]), f0)]
        pred = {c: c0[i] for i, c in enumerate(DATA)}
        for j in range(K):
            if d[j]:
                fi, bi = sf(j)
                col = G[(fi, bi)]
                for i, c in enumerate(DATA):
                    if col[i]: pred[c] ^= 1
        # verify constancy per cell
        for c, v in pred.items():
            P.setdefault(c, set()).add(v)
    return {c: next(iter(v)) for c, v in P.items() if len(v) == 1}

Q8 = pred_all(8, '12345678', sh['D8'], sh['G8'], F8)
Q9 = pred_all(9, '123456789', sh['D9'], sh['G9'], F9)
Q10 = pred_all(10, '1234567890', h['DATA'], h['G'], F10)
common = sorted(set(Q8) & set(Q9) & set(Q10))
print(f"common constant cells: {len(common)}")
d89 = {c: Q8[c] ^ Q9[c] for c in common}
d810 = {c: Q8[c] ^ Q10[c] for c in common}
from collections import Counter
sig = Counter(d89.values()).most_common(1)[0][0]
sig2 = Counter(d810.values()).most_common(1)[0][0]
print(f"sigma (majority d89) = {sig}, (d810) = {sig2}")
bit0 = [c for c in common if d89[c] != sig and d810[c] == sig]
bit1 = [c for c in common if d89[c] == sig and d810[c] != sig]
fmt = [c for c in common if d89[c] == sig and d810[c] == sig]
print(f"count bit0 cells: {sorted(bit0)}")
print(f"count bit1 cells: {sorted(bit1)}")
print(f"format/other constant cells: {len(fmt)}")
json.dump({'Q8': {f"{x},{y}": v for (x, y), v in Q8.items()},
           'Q9': {f"{x},{y}": v for (x, y), v in Q9.items()},
           'Q10': {f"{x},{y}": v for (x, y), v in Q10.items()},
           'sigma89': sig, 'sigma810': sig2},
          open('/tmp/opencode/m2hdr.json', 'w'))
print("saved m2hdr.json")
