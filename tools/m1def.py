"""M1 definitive column extraction: per-field bits, filler scan, cross-validation."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]
N = len(DATA)
POS = {c: i for i, c in enumerate(DATA)}

_memo = {}
def vec(s):
    if s not in _memo:
        _memo[s] = tuple(render(s, level=1, mask=0, version=1)[y][x] for x, y in DATA)
    return _memo[s]

def dxor(u, v): return tuple(a ^ b for a, b in zip(u, v))
def dsz(u, v): return sum(a != b for a, b in zip(u, v))

def collect(target, probe_pairs, ntries=40):
    """probe_pairs: generator of (sa, sb). Keep unanimous small-diff columns."""
    votes = {}
    for sa, sb in probe_pairs:
        if len(votes) > 60: break
        try: va, vb = vec(sa), vec(sb)
        except Exception: continue
        if dsz(va, vb) < 12:
            c = dxor(va, vb)
            votes[c] = votes.get(c, 0) + 1
    if not votes: return None, 0
    best = max(votes.items(), key=lambda kv: kv[1])
    return best

def fillers(n=30):
    return [f'{random.randrange(10)}{random.randrange(10)}{random.randrange(10)}' for _ in range(n)]

cols = {}
# pair fields: bits of the 7-bit pair value
for pf in (0, 1):
    for b in range(7):
        pv = 1 << b
        pairs = []
        vs = [v for v in range(0, 100 - pv + 1) if v % (2 * pv) == 0]
        random.shuffle(vs)
        for v in vs[:12]:
            f = random.choice(fillers(6))
            if pf == 0:
                sa, sb = f'{v:02d}' + f, f'{v + pv:02d}' + f
            else:
                sa, sb = f[0] + f'{v:02d}' + f[1] + f[2], f[0] + f'{v + pv:02d}' + f[1] + f[2]
            pairs.append((sa, sb))
        c, votes = collect((pf, b), pairs)
        cols[(pf, b)] = (c, votes)
        w = sum(c) if c else 0
        print(f"pair{pf} bit{b} (val {pv:3}): votes {votes} weight {w}")

# remainder digit (5-digit payloads): vary last digit, fillers = first 4
for b in range(4):
    pv = 1 << b
    pairs = []
    los = [d for d in range(10) if not (d >> b) & 1]
    his = [d for d in range(10) if (d >> b) & 1]
    for t in range(25):
        f4 = f'{random.randrange(10)}{random.randrange(10)}{random.randrange(10)}{random.randrange(10)}'
        a, bb = random.choice(los), random.choice(his)
        pairs.append((f4 + str(a), f4 + str(bb)))
    c, votes = collect(('rem', b), pairs)
    cols[('rem', b)] = (c, votes)
    w = sum(c) if c else 0
    print(f"rem bit{b} (val {pv:2}): votes {votes} weight {w}")

json.dump({','.join(map(str, k)): [DATA[i] for i in range(N) if c[i]]
           for k, (c, v) in cols.items() if c},
          open('/tmp/opencode/m1def.json', 'w'))
