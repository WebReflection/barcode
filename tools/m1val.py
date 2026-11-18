"""M1: confirm field order; examine G; validate on fresh payloads."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]
N = len(DATA)

fit = json.load(open('/tmp/opencode/m1fit.json'))
G = fit['G']; anchor = fit['component'][0]
A0 = None
_memo = {}
def vec(s):
    if s not in _memo:
        _memo[s] = tuple(render(s, level=1, mask=0, version=1)[y][x] for x, y in DATA)
    return _memo[s]

def f_32(s, order='32'):
    d = [int(c) for c in s]
    if order == '32':
        g1, g2 = d[0]*100+d[1]*10+d[2], d[3]*10+d[4]
        return [(g1 >> (9-i)) & 1 for i in range(10)] + [(g2 >> (6-i)) & 1 for i in range(7)]
    else:  # '23' = groups swapped in stream order
        g1, g2 = d[0]*100+d[1]*10+d[2], d[3]*10+d[4]
        return [(g2 >> (6-i)) & 1 for i in range(7)] + [(g1 >> (9-i)) & 1 for i in range(10)]

def predict(s, order='32', anchor=anchor):
    f0, c0 = f_32(anchor, order), vec(anchor)
    f = f_32(s, order)
    out = list(c0)
    for j in range(17):
        if f[j] != f0[j]:
            for i in range(N): out[i] ^= G[i][j]
    return out

# held-out validation: pairs whose diff < 30 (same mask hopefully)
random.seed(555)
ok = bad = skip = 0
for t in range(120):
    s = ''.join(str(random.randrange(10)) for _ in range(5))
    s2 = ''.join(str(random.randrange(10)) for _ in range(5))
    try: va, vb = vec(s), vec(s2)
    except Exception: continue
    d = sum(x != y for x, y in zip(va, vb))
    if d >= 30: skip += 1; continue
    pa, pb = predict(s), predict(s2)
    if sum(x != y for x, y in zip(pa, pb)) < 30:
        if pa == vb and pb == va: ok += 1
        else: bad += 1
    else: skip += 1
print(f"held-out (order '32'): {ok} ok, {bad} bad, {skip} skip")

# G structure: row weights, zero rows
zw = sum(1 for i in range(N) if not any(G[i]))
print(f"zero rows (payload-independent cells): {zw}")
wts = sorted(sum(G[i]) for i in range(N))
print("row weights:", wts)
colw = [sum(G[i][j] for i in range(N)) for j in range(17)]
print("column weights (per field bit):", colw)
