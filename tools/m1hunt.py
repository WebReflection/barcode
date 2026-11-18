"""M1: aggressive same-mask column collector for all digit slots + lengths."""
import sys, json, itertools, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]
N = len(DATA)

_memo = {}
def vec(s):
    if s not in _memo:
        m = render(s, level=1, mask=0, version=1)
        _memo[s] = tuple(m[y][x] for x, y in DATA)
    return _memo[s]

def dxor(u, v): return tuple(a ^ b for a, b in zip(u, v))
def dsize(u, v): return sum(1 for a, b in zip(u, v) if a != 0 for _ in [0]) if False else sum(a != b for a, b in zip(u, v))

def bits(d): return [(d >> (3 - i)) & 1 for i in range(4)]

# For each slot, collect columns via filler search.
# pair-value bit deltas achievable by single-digit changes:
#   slot in pair (0,1): pair = 10*d0+d1. d1 change: delta = dd (0..9) -> single-bit if dd in {1,2,4,8} and no carry
#                       d0 change: delta = 10*dd -> never single-bit (10*dd = 2*5*dd)
#   slot 4 (remainder): delta = dd -> single-bit if dd in {1,2,4,8}
# Work at the PAIR-VALUE level: extract columns for the 7-bit pair value bits
# using pair-value single-bit deltas from digit substitutions, then map.

cols = {}   # (stream_bit_position) -> column vector (over DATA)
conflicts = []

def try_pair(sa, sb, tag):
    """sa, sb: payloads; tag: (bitpos) if the encoded value differs in exactly 1 bit."""
    va, vb = vec(sa), vec(sb)
    if dsize(va, vb) <= 10:
        col = dxor(va, vb)
        key = tag
        if key in cols:
            if cols[key] != col: conflicts.append((key, sa, sb))
        else:
            cols[key] = col
        return True
    return False

# slot 4 (5-digit remainder): digits d, value delta = d
# value bits of the remainder digit: 4-bit binary
fillers = ['0']
for dA, dB, bitidx in [(0,2,2),(2,3,3),(0,4,1),(2,6,1),(4,6,2),(3,7,2),(2,0,2),(3,2,3),
                       (6,4,2),(7,3,2),(6,7,1),(7,6,1),(4,0,1),(0,8,0),(8,9,3),(9,8,3),(1,3,3),(5,7,1),(1,5,1),(0,1,3)]:
    done = False
    for f in fillers:
        if try_pair(f+'0000'+str(dA) if False else f'0000{dA}', f'0000{dB}', ('rem', bitidx)):
            done = True; break
    # 4-digit variants: '000d' -> pairs '00','0d'
    for f in ['']:
        if try_pair(f'000{dA}', f'000{dB}', ('rem', bitidx)):
            done = True; break

# slot 1 (second digit of pair 1): '0d000' vs base, delta = d
# pair1 value = d; single-bit deltas: d in {1,2,4,8} vs 0 (and chains)
for dA, dB, bitidx in [(0,1,3),(0,2,2),(0,4,1),(0,8,0),(2,3,3),(2,6,1),(4,5,3),(4,6,2),(8,9,3),(2,0,2),(6,2,1),(3,1,3),(7,5,1),(5,4,3),(6,4,2)]:
    for fill in ['00000', '10000', '20000', '30000', '01000', '00100', '00010', '00001', '00002', '00004']:
        sA = fill[:1] + str(dA) + fill[2:]
        sB = fill[:1] + str(dB) + fill[2:]
        if try_pair(sA, sB, ('p1', bitidx)): break

# slot 3 (second digit of pair 2): '000d0'
for dA, dB, bitidx in [(0,1,3),(0,2,2),(0,4,1),(0,8,0),(2,3,3),(2,6,1),(4,5,3),(4,6,2),(8,9,3),(3,1,3),(7,5,1),(5,4,3),(6,4,2),(2,0,2),(6,2,1)]:
    for fill in ['00000', '10000', '01000', '00100', '00001', '00002', '00004', '00008', '20000']:
        sA = fill[:3] + str(dA) + fill[4:]
        sB = fill[:3] + str(dB) + fill[4:]
        if try_pair(sA, sB, ('p2', bitidx)): break

print("conflicts:", conflicts)
print(f"columns: {len(cols)}")
for key in sorted(cols):
    c = cols[key]
    on = [DATA[k] for k in range(N) if c[k]]
    print(f"  {key}: w{len(on)}: {on}")
json.dump({','.join(map(str, k)): [DATA[i] for i in range(N) if c[i]] for k, c in cols.items()},
          open('/tmp/opencode/m1allcols.json', 'w'))
