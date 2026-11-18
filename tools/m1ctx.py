"""M1: same-logical-bit probes at different pair values -> context dependence?"""
import sys
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]

def vec(s):
    m = render(s, level=1, mask=0, version=1)
    return tuple(m[y][x] for x, y in DATA)

def dxor(u, v): return tuple(a ^ b for a, b in zip(u, v))
def dsize(u, v): return sum(a != b for a, b in zip(u, v))

# pair1 value-1 bit: d1 even->odd with d0 = X fixed
print("pair1 value-1 bit (d1: 0->1), d0 = X:")
res = {}
for X in range(10):
    sA = f'{X}0000'; sB = f'{X}1000'
    d = dxor(vec(sA), vec(sB))
    res[X] = d
    cells = [DATA[i] for i in range(len(DATA)) if d[i]]
    print(f"  X={X}: {dsize(vec(sA),vec(sB))} cells: {cells if dsize(vec(sA),vec(sB))<12 else 'MASK-CHANGE'}")

same = sum(1 for X in range(1, 10) if res[X] == res[0])
print(f"identical to X=0: {same}/9")

# also: value-2 bit via 0->2 at each X
print("pair1 value-2 bit (d1: 0->2):")
res2 = {}
for X in range(10):
    sA = f'{X}0000'; sB = f'{X}2000'
    d = dxor(vec(sA), vec(sB))
    res2[X] = d
    n = dsize(vec(sA), vec(sB))
    cells = [DATA[i] for i in range(len(DATA)) if d[i]]
    print(f"  X={X}: {n} cells: {cells if n < 12 else 'MASK-CHANGE'}")
same2 = sum(1 for X in range(1, 10) if res2[X] == res2[0])
print(f"identical to X=0: {same2}/9")
