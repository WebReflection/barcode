"""M1: leave-one-out validation within the same-mask component + structure dump."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]
N = len(DATA)

fit = json.load(open('/tmp/opencode/m1fit.json'))
G = fit['G']; comp = fit['component']
anchor = comp[0]
_memo = {}
def vec(s):
    if s not in _memo:
        _memo[s] = tuple(render(s, level=1, mask=0, version=1)[y][x] for x, y in DATA)
    return _memo[s]

def f_32(s):
    d = [int(c) for c in s]
    g1, g2 = d[0]*100+d[1]*10+d[2], d[3]*10+d[4]
    return [(g1 >> (9-i)) & 1 for i in range(10)] + [(g2 >> (6-i)) & 1 for i in range(7)]

def predict(s):
    f0, c0 = f_32(anchor), vec(anchor)
    f = f_32(s)
    out = list(c0)
    for j in range(17):
        if f[j] != f0[j]:
            for i in range(N): out[i] ^= G[i][j]
    return out

# leave-one-out: for each pair in component, predict diff, compare
ok = bad = 0
import itertools
for a, b in itertools.combinations(comp, 2):
    va, vb = vec(a), vec(b)
    pa, pb = predict(a), predict(b)
    if tuple(pa) == va and tuple(pb) == vb: ok += 1
    else: bad += 1
print(f"in-component: {ok} ok, {bad} bad")

# systematic structure: which cell for each bit (weight-1 rows)
print("\nsystematic cells per stream bit:")
for j in range(17):
    rows = [i for i in range(N) if sum(G[i][j] for _ in [0]) == 1 and sum(G[i]) == 1]
syscells = {}
for j in range(17):
    ones = [i for i in range(N) if G[i][j] == 1 and sum(G[i]) == 1]
    syscells[j] = [DATA[i] for i in ones]
    print(f"  bit{j:2}: {syscells[j]}")

# parity rows: cells with weight>=2, their bit-subsets
print("\nparity cells (weight>=2 rows):")
for i in range(N):
    w = sum(G[i])
    if w >= 2:
        bits = [j for j in range(17) if G[i][j]]
        print(f"  {DATA[i]}: w{w} bits {bits}")
