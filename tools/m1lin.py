"""M1 linear-code extraction: per-bit columns from same-mask digit probes."""
import sys, itertools
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]
IDX = {c: i for i, c in enumerate(DATA)}

def vec(s):
    m = render(s, level=1, mask=0, version=1)
    return tuple(m[y][x] for x, y in DATA)

def dsize(u, v): return sum(1 for a, b in zip(u, v) if a != b)
def dxor(u, v): return tuple(a ^ b for a, b in zip(u, v))

# last-digit probes
base = vec('00000')
probes = {}
for d in range(10):
    v = vec(f'0000{d}')
    probes[d] = (v, dsize(base, v))

# same-mask clique with base: diff < 10
clique = [d for d, (v, ds) in probes.items() if ds < 10]
print("same-mask as '00000':", clique)

# digit bit values
def bits(d): return [(d >> (3 - i)) & 1 for i in range(4)]
# contribution of digit-value d vs 0 = XOR of columns C3..C0
# C(b_i) = diff between digits differing only in bit i
cols = {}
for i in range(4):
    for a in clique:
        for b in clique:
            if bits(a)[i] != bits(b)[i] and all(bits(a)[j] == bits(b)[j] for j in range(4) if j != i):
                cols[i] = dxor(probes[a][0], probes[b][0])
                break
        if i in cols: break

for i, c in sorted(cols.items()):
    on = [DATA[k] for k in range(len(DATA)) if c[k]]
    print(f"C{i}: weight {len(on)}: {on}")

# consistency: C3 ^ C2 ^ C1 ^ C0 for digit 7 should equal diff(0,7)
if all(i in cols for i in range(4)):
    pred = dxor(dxor(cols[3], dxor(cols[2], dxor(cols[1], cols[0]))), base)
    v7 = vec('00007')
    print("linear prediction for '00007' matches:", pred == v7)
