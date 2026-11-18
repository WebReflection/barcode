"""M1: targeted pair1-val8 scan + full linear-model verification on held-out pairs."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]
N = len(DATA)

_memo = {}
def vec(s):
    if s not in _memo:
        _memo[s] = tuple(render(s, level=1, mask=0, version=1)[y][x] for x, y in DATA)
    return _memo[s]
def dxor(u, v): return tuple(a ^ b for a, b in zip(u, v))
def dsz(u, v): return sum(a != b for a, b in zip(u, v))

G = {}
for k, v in json.load(open('/tmp/opencode/m1def2.json')).items():
    a, b = k.split(',')
    G[(a, int(b))] = tuple(1 if tuple(c) in [tuple(x) for x in v] else 0 for c in DATA)

# --- targeted pair1 val-8 ---
random.seed(7)
votes = {}
for t in range(400):
    v = random.choice([0, 16, 32, 48, 64, 80])
    w = v + 8
    f = ''.join(str(random.randrange(10)) for _ in range(3))
    sa = f[0] + f'{v:02d}' + f[1] + f[2]
    sb = f[0] + f'{w:02d}' + f[1] + f[2]
    try: va, vb = vec(sa), vec(sb)
    except Exception: continue
    if dsz(va, vb) < 30:
        c = dxor(va, vb)
        votes[c] = votes.get(c, 0) + 1
if votes:
    best = max(votes.items(), key=lambda kv: kv[1])
    print("pair1 val8:", best[1], "votes, weight", sum(best[0]))
    if best[1] >= 2:
        G[('1', 3)] = best[0]
        raw = json.load(open('/tmp/opencode/m1def2.json'))
        raw['1,3'] = [DATA[i] for i in range(N) if best[0][i]]
        json.dump(raw, open('/tmp/opencode/m1def2.json', 'w'))
else:
    print("pair1 val8: STILL no same-mask pair")

# --- verification: held-out probe pairs, predict diff from G ---
def stream_bits(s):
    """data fields for 5-digit numeric: hdr(?) + p1(7) + p2(7) + rem(4).
    Return dict field->value; header unknown for now (ignore: hdr constant
    for all 5-digit payloads so it cancels in diffs)."""
    d = [int(c) for c in s]
    return {('0', i): ((d[0] * 10 + d[1]) >> (6 - i)) & 1 for i in range(7)} | \
           {('1', i): ((d[2] * 10 + d[3]) >> (6 - i)) & 1 for i in range(7)} | \
           {('rem', i): (d[4] >> (3 - i)) & 1 for i in range(4)}

def predict_diff(sA, sB):
    bA, bB = stream_bits(sA), stream_bits(sB)
    out = [0] * N
    for k, col in G.items():
        if k not in bA or k not in bB: return None
        if bA[k] != bB[k]:
            for i in range(N): out[i] ^= col[i]
    return out

ok = bad = skip = 0
random.seed(99)
for t in range(60):
    s = [random.randrange(10) for _ in range(5)]
    i = random.randrange(5)
    s2 = list(s); s2[i] = (s2[i] + random.randrange(1, 10)) % 10
    sA, sB = ''.join(map(str, s)), ''.join(map(str, s2))
    try: va, vb = vec(sA), vec(sB)
    except Exception: continue
    if dsz(va, vb) >= 30: continue   # mask change, skip
    pred = predict_diff(sA, sB)
    if pred is None: continue
    if pred == dxor(va, vb): ok += 1
    else:
        bad += 1
        if bad <= 2:
            print("MISMATCH", sA, sB)
            exp = [DATA[i2] for i2 in range(N) if dxor(va, vb)[i2]]
            pr = [DATA[i2] for i2 in range(N) if pred[i2]]
            print("  actual:", exp)
            print("  pred:  ", pr)
print(f"\nverification: {ok} ok, {bad} bad (mask-changed pairs skipped)")
