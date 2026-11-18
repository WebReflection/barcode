"""M1: extract bit-columns for every digit slot; dedupe; analyze code structure."""
import sys, json
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]

def vec(s):
    m = render(s, level=1, mask=0, version=1)
    return tuple(m[y][x] for x, y in DATA)

def dxor(u, v): return tuple(a ^ b for a, b in zip(u, v))
def dsize(u, v): return sum(1 for a, b in zip(u, v) if a != b)

def bits(d): return [(d >> (3 - i)) & 1 for i in range(4)]  # MSB-first idx 0..3

def probe_slot(base, fmt):
    """fmt: format string with {d}; returns dict bitidx -> column (only same-mask)."""
    b0 = vec(base)
    fam = {}
    for d in range(10):
        v = vec(fmt.format(d=d))
        fam[d] = (v, dsize(b0, v))
    clique = [d for d, (v, ds) in fam.items() if ds < 10]
    cols = {}
    for i in range(4):
        for a, b in itertools.permutations(clique, 2):
            if bits(a)[i] != bits(b)[i] and all(bits(a)[j] == bits(b)[j] for j in range(4) if j != i):
                cols[i] = dxor(fam[a][0], fam[b][0]); break
        if i in cols: continue
    return cols, clique

import itertools
slotcols = {}
for slot, base in enumerate(['00000', '00000', '00000', '00000', '00000']):
    # vary digit at position `slot` of 5
    fmt = '00000'[:slot] + '{d}' + '00000'[slot+1:]
    cols, clique = probe_slot('00000', fmt)
    slotcols[slot] = cols
    print(f"slot {slot} (fmt {fmt}): clique {clike if False else clique}, cols: "
          + ", ".join(f"i{i}:w{sum(c[k] for k in range(len(DATA)))}" for i, c in cols.items()))

# dedupe columns across slots -> distinct data bits
allc = {}
for slot, cols in slotcols.items():
    for i, c in cols.items():
        key = frozenset(k for k in range(len(DATA)) if c[k])
        allc.setdefault(key, []).append((slot, i))
print(f"\ndistinct columns: {len(allc)}")
for key, owners in allc.items():
    print(f"  weight {len(key)} cells {sorted(DATA[k] for k in key)}  <- slots {owners}")
json.dump({','.join(map(str, sorted(key))): owners for key, owners in allc.items()},
          open('/tmp/opencode/m1cols.json', 'w'))
