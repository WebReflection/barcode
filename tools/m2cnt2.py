"""M2 count, definitive: full-cell residuals, sigma translation, brute-force offset."""
import sys, json, random, itertools
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

random.seed(55)
_rend = {}
def vec(s, L):
    if (s, L) not in _rend:
        _rend[(s, L)] = {(x, y): render(s, level=1, mask=0, version=2)[y][x]
                         for y in range(13) for x in range(13)}
    return _rend[(s, L)]
ALL = [(x, y) for y in range(13) for x in range(13)]
h = json.load(open('/tmp/opencode/m2h.json'))
sh = json.load(open('/tmp/opencode/m2short.json'))

def classes_full(L, anchor, DATAj, Gj, FIELDS, n=110):
    DATA = [tuple(c) for c in DATAj]; N = len(DATA)
    cellix = {c: i for i, c in enumerate(DATA)}
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
    d0 = [int(c) for c in anchor]; f0 = fmodel(d0)
    aobs = vec(anchor, L)
    c0 = tuple(aobs[c] for c in DATA)
    def sf(j):
        n = 0
        for fi in range(len(FIELDS)):
            w = FIELDS[fi][2]
            if j < n + w: return (fi, j - n)
            n += w
    cls = {}
    for t in range(n):
        s = ''.join(str(random.randrange(10)) for _ in range(L))
        d = [x ^ y for x, y in zip(fmodel([int(c) for c in s]), f0)]
        predDATA = list(c0)
        for j in range(K):
            if d[j]:
                fi, bi = sf(j)
                col = G[(fi, bi)]
                for i in range(N): predDATA[i] ^= col[i]
        m = vec(s, L)
        # full residual: obs ^ pred; pred = c0 outside DATA
        r = tuple(m[c] ^ (aobs[c] if c not in cellix else predDATA[cellix[c]]) for c in ALL)
        cls.setdefault(r, []).append(s)
    return cls, aobs

F10 = [(0, 3, 10), (3, 3, 10), (6, 3, 10), (9, 1, 4)]
F9 = [(0, 3, 10), (3, 3, 10), (6, 3, 10)]
F8 = [(0, 3, 10), (3, 3, 10), (6, 2, 7)]
C10, aobs10 = classes_full(10, '1234567890', h['DATA'], h['G'], F10)
C9, aobs9 = classes_full(9, '123456789', sh['D9'], sh['G9'], F9)
C8, aobs8 = classes_full(8, '12345678', sh['D8'], sh['G8'], F8)
print(f"classes: len10 {len(C10)}, len9 {len(C9)}, len8 {len(C8)}")
print("weights10:", sorted((sum(r) for r in C10), reverse=True))

# sigma via set translation, over cells where both residual dicts are defined (ALL cells here)
def find_sigma(Ra, Rb):
    best, hits = None, -1
    for ra in Ra:
        for rb in Rb:
            sig = tuple(x ^ y for x, y in zip(ra, rb))
            n = sum(1 for r in Rb if tuple(x ^ y for x, y in zip(r, sig)) in set(Ra))
            if n > hits: best, hits = sig, n
    return [best], hits
s89 = find_sigma(set(C8), set(C9))
s810 = find_sigma(set(C8), set(C10))
print(f"sigma89 candidates: {len(s89)}, sigma810: {len(s810)}")
sig89 = s89[0]; sig810 = s810[0]
# mv_aL relative to mv_a10: mv_a8 = sig810 ^ mv_a10 (since r = mv ^ mv_a; R10 = {mv^mv_a10})
# Q_L[c] = pred_L[c] = count_L[c] ^ mv_aL[c]; mv_a8 = sig810 ^ mv_a10
CC4 = [(12, 12), (11, 12), (12, 11), (11, 11)]
# pred at cell c = aobs_L[c] ^ (mask flip? no): pred outside DATA = aobs (anchor obs) -> Q = aobs?!
# CAREFUL: pred outside DATA = anchor's obs = count_L[cc] ^ mv_aL[cc] only if anchor unmasked-count...
# Actually obs_anchor = unmasked ^ mv_a -> pred = obs_anchor = unmasked ^ mv_a. Same formula. Good:
# use pred = aobs_L at CC4 (they are outside DATA for that length, else use class-const obs)
def qval(L, aobs, cls, c):
    # pred at c: if c outside DATA -> aobs[c]; else c0 (same for all renders) -> aobs[c] as well?
    # pred = c0 only at cells where G rows are all-zero; for DATA cells with G activity pred varies.
    return aobs[c]
# For CC4 use pred = aobs[c] IF the cell is non-DATA or G-inactive; verify via class-constancy:
def pred_const(L, cls, aobs, DATAj, c):
    return all(len({vec(s, L)[c] ^ (tuple(x ^ y for x, y in zip(vec(s, L), vec(s, L)))and 0 or 0)} ) >= 0 for s in [])
# simpler: pred at c is class-independent iff obs^r is; compute obs^r per class:
def unmasked_offset(L, cls, aobs, DATAj, c):
    """returns set of obs^r values at c across classes (should be 1 element = count ^ mv_a10-ish)"""
    vals = set()
    for r in cls:
        s = cls[r][0]
        m = vec(s, L)
        vals.add(m[c] ^ r[ALL.index(c)])
    return vals
for c in CC4:
    v8 = unmasked_offset(8, C8, aobs8, sh['D8'], c)
    v9 = unmasked_offset(9, C9, aobs9, sh['D9'], c)
    v10 = unmasked_offset(10, C10, aobs10, h['DATA'], c)
    print(f"  {c}: obs^r sets: len8 {v8}, len9 {v9}, len10 {v10}")
