"""M2 numeric-10 under 3+3+2+2: extract all field-bit columns, verify multi-bit."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

random.seed(31)
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
IDX = {c: i for i, c in enumerate(DATA)}
def T(s): return tuple(vec(s)[c] for c in DATA)
N = len(DATA)
print(f"varying cells: {N}")

# fields: (digit offset, n digits, n bits)
FIELDS = [(0, 3, 10), (3, 3, 10), (6, 2, 7), (8, 2, 7)]
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
        dA = [random.randrange(10) for _ in range(10)]
        dB = list(dA)
        setf(dA, f, v); setf(dB, f, w)
        sA = ''.join(map(str, dA)); sB = ''.join(map(str, dB))
        try: va, vb = T(sA), T(sB)
        except Exception: continue
        df = sum(x != y for x, y in zip(va, vb))
        if df <= thr:
            key = tuple(x ^ y for x, y in zip(va, vb))
            votes[key] = votes.get(key, 0) + 1
    if not votes: return None, 0, 0
    best, bv = max(votes.items(), key=lambda kv: kv[1])
    return best, bv, sum(votes.values())

G = {}
for f in range(4):
    for bit in range(nbits(f)):
        c, votes, total = probe(f, bit)
        w = sum(c) if c else 0
        print(f"field{f} bit{bit} ({1<<bit:3}): {votes}/{total} votes, weight {w}", flush=True)
        if c and votes >= 6:
            col = [0] * N
            for i in range(N):
                if c[i]: col[i] = 1
            G[(f, bit)] = col

json.dump({'G': {f"{f},{b}": c for (f, b), c in G.items()},
           'DATA': [list(c) for c in DATA], 'FIELDS': FIELDS},
          open('/tmp/opencode/m2g.json', 'w'))
print(f"extracted {len(G)}/34 columns -> m2g.json")

# ---- verification: fresh probes, predict = XOR of columns ----
def predict(dA, dB):
    out = [0] * N
    for f in range(4):
        va, vb = fval(dA, f), fval(dB, f)
        if va == vb: continue
        x = va ^ vb
        for b in range(nbits(f)):
            if (x >> b) & 1:
                if (f, b) not in G: return None
                col = G[(f, b)]
                for i in range(N): out[i] ^= col[i]
    return out

ok = bad = 0
for t in range(120):
    dA = [random.randrange(10) for _ in range(10)]
    dB = list(dA)
    f = random.randrange(4)
    if random.random() < 0.5:  # single-bit
        b = random.randrange(nbits(f)); pv = 1 << b
        v = fval(dA, f); v &= ~((1 << (b + 1)) - 1)
        if v + pv >= 10 ** FIELDS[f][1]: continue
        setf(dB, f, v + pv)
    else:  # arbitrary value change
        v = random.randrange(10 ** FIELDS[f][1])
        setf(dB, f, v)
    sA = ''.join(map(str, dA)); sB = ''.join(map(str, dB))
    try: va, vb = T(sA), T(sB)
    except Exception: continue
    pred = predict(dA, dB)
    if pred is None: continue
    if tuple(pred) == tuple(x ^ y for x, y in zip(va, vb)): ok += 1
    else: bad += 1
print(f"verification: {ok} ok, {bad} bad (mismatches may be mask changes)")
