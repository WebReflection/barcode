"""M1: full column extraction for every stream bit (pair1, pair2, rem) with
filler randomization + cross-validation. 5-digit payloads."""
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

random.seed(11)
def digits(v):
    return f'{v:02d}'

def col_for(field, bit, tries=25):
    """field: 0,1 = pair index; 2 = remainder digit. bit: value exponent."""
    found = {}
    for t in range(tries):
        if field == 2:
            # remainder digit d: bit set iff d has 2^bit
            lo = [d for d in range(10) if not (d >> bit) & 1]
            hi = [d for d in range(10) if (d >> bit) & 1]
            a, b = random.choice(lo), random.choice(hi)
            sa = f'00{a}0{b}0'[:4] + str(a) if False else None
            # 5-digit: digits s0..s4; rem = s4
            rest = f'{random.randrange(10)}{random.randrange(10)}'
            sa = f'00{rest[0]}{rest[1]}{a}'
            sb = f'00{rest[0]}{rest[1]}{b}'
        else:
            # pair value v -> v + 2^bit, placed at pair `field`
            v = random.randrange(0, 100 - (1 << bit) + 1)
            v &= ~((1 << (bit + 1)) - 1)   # ensure no carry
            w = v + (1 << bit)
            if w > 99: continue
            dA, dB = digits(v), digits(w)
            rest = f'{random.randrange(10)}{random.randrange(10)}{random.randrange(10)}'
            if field == 0:
                sa, sb = dA + rest, dB + rest
            else:
                sa, sb = rest[:1] + dA + rest[1:] + rest[-1] if False else rest[0] + dA + rest[1] + rest[2], rest[0] + dB + rest[1] + rest[2]
        va, vb = vec(sa), vec(sb)
        if dsz(va, vb) < 12:
            c = dxor(va, vb)
            found[c] = found.get(c, 0) + 1
    if not found: return None, 0
    best = max(found, key=found.get)
    return best, found[best]

cols = {}
for field, name in ((0, 'p1'), (1, 'p2'), (2, 'rem')):
    for bit in range(7 if field < 2 else 4):
        if field < 2 and (1 << bit) > 99: break
        c, votes = col_for(field, bit)
        w = sum(c) if c else 0
        cols[(name, bit)] = c
        print(f"{name} bit{bit}: votes {votes}, weight {w}")

json.dump({','.join(map(str, k)): [DATA[i] for i in range(N) if c[i]]
           for k, c in cols.items() if c},
          open('/tmp/opencode/m1full.json', 'w'))
