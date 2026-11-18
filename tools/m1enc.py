"""M1 full encoder validation: build symbols from spec, compare vs zint renders.

cells = G.f ^ K5_m ^ countfix, where K5_m = anchor-class constant ^ mask map m,
countfix = (5 ^ L) at the 3 count cells. Tries all 4 mask classes per payload.
"""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

fit = json.load(open('/tmp/opencode/m1fit.json'))
G = fit['G']; DATA = [tuple(c) for c in fit['DATA']]; N = len(DATA)
IDX = {c: i for i, c in enumerate(DATA)}
anchor = fit['component'][0]

masks = json.load(open('/Users/ag/git/barcode/tools/m1masks.json'))
fmt = json.load(open('/Users/ag/git/barcode/tools/m1fmt.json'))
# normalize masks json
MD = json.load(open('/Users/ag/git/barcode/tools/m1masks.json'))
MDATA = [tuple(c) for c in MD['DATA']]
assert MDATA == DATA, "mask DATA order mismatch"
maps = [tuple(m[0]) for m in MD['masks']]
print("mask maps:", len(maps), "weights:", [sum(m) for m in maps])
CC = [(9, 10), (10, 10), (10, 9)]  # b2, b1, b0 of count=L
CC_IDX = [IDX[c] for c in CC]
FMT_CELLS = [(8, 1), (8, 2), (8, 3), (8, 4), (8, 5), (8, 6), (8, 7), (4, 8), (5, 8), (6, 8), (7, 8)]

# K5 per class: need obs of a class-m len-5 payload ^ G.f -- use anchor (class of masks[0]? verify)
def fvec(s):
    d = [int(c) for c in s]
    g1, g2 = d[0]*100+d[1]*10+d[2], d[3]*10+d[4]
    return [(g1 >> (9-i)) & 1 for i in range(10)] + [(g2 >> (6-i)) & 1 for i in range(7)]

def obs5(s):
    m = render(s, level=1, mask=0, version=1)
    return {c: m[y][x] for c in DATA for x, y in [c]}

# anchor class constant: K_anchor = obs ^ G.f0
f0 = fvec(anchor)
o0 = obs5(anchor)
K_a = [o0[DATA[i]] ^ sum(G[i][j] for j in range(17) if f0[j]) % 2 for i in range(N)]
# count-cell values in K_a should equal 5 ^ mv_a[count] -> derive mv_a at count cells:
mv_a_cc = [K_a[i] ^ ((5 >> (2 - k)) & 1) for k, i in enumerate(CC_IDX)]
print("mv_anchor at count cells:", mv_a_cc)
# class maps: m1masks relative to anchor class; verify anchor's class map is all-zeros? print weights
# K_m = K_a ^ map_m  (maps = mv_m ^ mv_anchor)
Ks = [[K_a[i] ^ maps[mi][i] for i in range(N)] for mi in range(len(maps))]

# funct template from a render
tmpl = render(anchor, level=1, mask=0, version=1)

def encode(payload, mi):
    L = len(payload); d = [int(c) for c in payload]
    bits = []
    i = 0
    while L - i >= 3:
        v = d[i]*100 + d[i+1]*10 + d[i+2]
        bits += [(v >> (9-j)) & 1 for j in range(10)]; i += 3
    if L - i == 2:
        v = d[i]*10 + d[i+1]
        bits += [(v >> (6-j)) & 1 for j in range(7)]
    elif L - i == 1:
        v = d[i]
        bits += [(v >> (3-j)) & 1 for j in range(4)]
    f = (bits + [0]*17)[:17]
    K = Ks[mi][:]
    # count correction: K currently encodes count=5 at CC; want L
    for k, ci in enumerate(CC_IDX):
        want = (L >> (2-k)) & 1
        K[ci] ^= want ^ ((5 >> (2-k)) & 1)
    cells = [sum(G[i][j] for j in range(17) if f[j]) % 2 ^ K[i] for i in range(N)]
    m = [row[:] for row in tmpl]
    for c in DATA:
        x, y = c
        m[y][x] = cells[IDX[c]]
    return m

random.seed(2026)
ok = miss = 0
fails = []
stat = {}
pls = []
for t in range(150):
    L = random.randrange(1, 6)
    p = ''.join(str(random.randrange(10)) for _ in range(L))
    if p not in pls: pls.append(p)
for p in pls:
    stat.setdefault(len(p), [0, 0])
    z = render(p, level=1, mask=0, version=1)
    hit = None
    for mi in range(len(maps)):
        e = encode(p, mi)
        if e == z: hit = mi; break
    stat[len(p)][0 if hit is None else 1] += 1
print(f"M1 encoder validation: {ok} ok, {miss} unmatched (of {len(pls)})")
for L in sorted(stat): print(f"  len {L}: {stat[L][1]} ok / {stat[L][0]} miss")
import pickle
# dump a failing case diff
p = fails[0] if fails else None
if p:
    z = render(p, level=1, mask=0, version=1)
    for mi in range(4):
        e = encode(p, mi)
        diff = [(DATA[i], e[y][x] ^ z[y][x]) for i, (x, y) in enumerate(DATA) if e[y][x] != z[y][x]]
        print(f"  mask{mi}: {len(diff)} cell diffs, sample {diff[:6]}")
