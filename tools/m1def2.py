"""M1: wide random scan for all columns; unanimous cross-validated."""
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

random.seed(42)

def scan(field, bit, npairs=250):
    pv = 1 << bit
    votes = {}
    for t in range(npairs):
        if field == 'rem':
            los = [d for d in range(10) if not (d >> bit) & 1]
            his = [d for d in range(10) if (d >> bit) & 1]
            a, b = random.choice(los), random.choice(his)
            f4 = f'{random.randrange(10)}{random.randrange(10)}{random.randrange(10)}{random.randrange(10)}'
            sa, sb = f4 + str(a), f4 + str(b)
        else:
            pi = int(field)
            v = random.randrange(0, 100 - pv + 1)
            v &= ~((1 << (bit + 1)) - 1)
            w = v + pv
            if w > 99: continue
            f = ''.join(str(random.randrange(10)) for _ in range(3))
            sa = (f'{v:02d}' + f) if pi == 0 else (f[0] + f'{v:02d}' + f[1] + f[2])
            sb = (f'{w:02d}' + f) if pi == 0 else (f[0] + f'{w:02d}' + f[1] + f[2])
        try: va, vb = vec(sa), vec(sb)
        except Exception: continue
        if dsz(va, vb) < 12:
            c = dxor(va, vb)
            votes[c] = votes.get(c, 0) + 1
    if not votes: return None, 0
    return max(votes.items(), key=lambda kv: kv[1])

cols = {}
for field in ('0', '1', 'rem'):
    for bit in range(7 if field != 'rem' else 4):
        c, votes = scan(field, bit)
        cols[(field, bit)] = (c, votes)
        print(f"{field} bit{bit} (val {1<<bit:3}): votes {votes} weight {sum(c) if c else 0}")

json.dump({','.join(map(str, k)): [DATA[i] for i in range(N) if c[i]]
           for k, (c, v) in cols.items() if c and v >= 2},
          open('/tmp/opencode/m1def2.json', 'w'))
