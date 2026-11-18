"""Verify M2 5-pairs model: predict diffs from extracted columns on fresh probes."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

random.seed(2026)
_rend = {}
def vec(s):
    if s not in _rend:
        _rend[s] = {(x, y): render(s, level=1, mask=0, version=2)[y][x]
                    for y in range(13) for x in range(13)}
    return _rend[s]
ref = vec('1111111111')
varying = set()
for t in range(30):
    s = ''.join(str(random.randrange(10)) for _ in range(10))
    m = vec(s)
    for c, v in m.items():
        if v != ref[c]: varying.add(c)
DATA = [c for c in [(x, y) for y in range(13) for x in range(13)] if c in varying]
def T(s): return tuple(vec(s)[c] for c in DATA)
N = len(DATA)

raw = json.load(open('/tmp/opencode/m2cols.json'))
G = {}
for k, cells in raw.items():
    pi, bi = map(int, k.split(','))
    col = [0]*N
    for x, y in cells: col[DATA.index((x, y))] = 1
    G[(pi, bi)] = col
print(f"columns: {len(G)}")

def predict(sA, sB):
    """XOR of columns for pair-value bit differences."""
    dA = [int(c) for c in sA]; dB = [int(c) for c in sB]
    out = [0]*N
    for i in range(5):
        va, vb = dA[2*i]*10+dA[2*i+1], dB[2*i]*10+dB[2*i+1]
        if va == vb: continue
        x = va ^ vb
        for b in range(7):
            if (x >> b) & 1:
                if (i, b) not in G: return None
                col = G[(i, b)]
                for k in range(N): out[k] ^= col[k]
    return out

ok = bad = 0
for t in range(50):
    ds = [random.randrange(10) for _ in range(10)]
    i = random.randrange(5)
    if random.random() < 0.5:
        # single-bit pair delta, carry-free
        b = random.randrange(7); pv = 1 << b
        v = ds[2*i]*10+ds[2*i+1]
        v &= ~((1 << (b+1)) - 1)
        if v + pv > 99: continue
        v2 = v + pv
    else:
        v = random.randrange(100); v2 = random.randrange(100)
    ds2 = list(ds); ds2[2*i] = v2//10; ds2[2*i+1] = v2 % 10
    sA = ''.join(map(str, ds)); sB = ''.join(map(str, ds2))
    try: va, vb = T(sA), T(sB)
    except Exception: continue
    d = sum(x != y for x, y in zip(va, vb))
    if d > 34: continue  # mask change
    pred = predict(sA, sB)
    if pred is None: continue
    if tuple(pred) == tuple(x ^ y for x, y in zip(va, vb)): ok += 1
    else:
        bad += 1
        if bad <= 2:
            print("MISMATCH", sA, sB, "diffsize", d)
print(f"verification: {ok} ok, {bad} bad")
