"""M2 count extraction: u = obs^residual per length; cross-length diffs expose count bits."""
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

def uvec(L, anchor, DATA, Gd, FIELDS):
    DATA = [tuple(c) for c in DATA]
    N = len(DATA)
    def T(s): return tuple(vec(s, L)[c] for c in DATA)
    def fval(d, f):
        off, n, w = FIELDS[f]; v = 0
        for i in range(n): v = v * 10 + d[off + i]
        return v
    G = {}
    for k, col in Gd.items():
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
    us = []
    for t in range(60):
        s = ''.join(str(random.randrange(10)) for _ in range(L))
        d = [x ^ y for x, y in zip(fmodel([int(c) for c in s]), f0)]
        pred = list(c0)
        for j in range(K):
            if d[j]:
                fi, bi = sf(j)
                for i in range(N): pred[i] ^= G[(fi, bi)][i]
        us.append(tuple(x ^ y for x, y in zip(T(s), pred)))
    print(f"len {L}: u-constancy: {len(set(us))} distinct across 60 renders (want 1)")
    return us[0], DATA

h = json.load(open('/tmp/opencode/m2h.json'))
sh = json.load(open('/tmp/opencode/m2short.json'))
F10 = [(0, 3, 10), (3, 3, 10), (6, 3, 10), (9, 1, 4)]
F9 = [(0, 3, 10), (3, 3, 10), (6, 3, 10)]
F8 = [(0, 3, 10), (3, 3, 10), (6, 2, 7)]
u10, DAT10 = uvec(10, '1234567890', h['DATA'], h['G'], F10)
u9, DAT9 = uvec(9, '123456789', sh['D9'], sh['G9'], F9)
u8, DAT8 = uvec(8, '12345678', sh['D8'], sh['G8'], F8)

# align to common cell set
common = set(DAT10) & set(DAT9) & set(DAT8)
print(f"common cells: {len(common)}")
def gv(u, DATA):
    return {c: u[DATA.index(c)] for c in common}
v10, v9, v8 = gv(u10, DAT10), gv(u9, DAT9), gv(u8, DAT8)
# count-bit patterns (u8^u9, u8^u10, u9^u10) per bit of counts 8,9,10:
# bit0: (0,1,0)->(d89,d810,d910)=(1,0,1); bit1: (0,0,1)->(0,1,1); bit2/3: (0,0,0)->(0,0,0)
hits = {0: [], 1: []}
for c in sorted(common):
    d89 = v8[c] ^ v9[c]; d810 = v8[c] ^ v10[c]; d910 = v9[c] ^ v10[c]
    if (d89, d810, d910) == (1, 0, 1): hits[0].append(c)
    if (d89, d810, d910) == (0, 1, 1): hits[1].append(c)
print("count bit0 cells:", hits[0])
print("count bit1 cells:", hits[1])
# absolute values at hits for the all-zeros-offset question
for c in hits[0] + hits[1]:
    print(f"  {c}: u8={v8[c]} u9={v9[c]} u10={v10[c]}")
json.dump({'u8': list(u8), 'u9': list(u9), 'u10': list(u10),
           'DAT8': [list(c) for c in DAT8], 'DAT9': [list(c) for c in DAT9],
           'DAT10': [list(c) for c in DAT10]},
          open('/tmp/opencode/m2count.json', 'w'))
