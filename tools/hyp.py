"""Hypothesis search at the strip 2->3 boundary.
Base: SPv (row-even mask). For the 35-digit lv1 render, try local path
variants and score derived-mask deviations from row-even."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH
from mqr_probe import render

FUNC = {0,1,32,33,34,35,200,201,202,203,204,205,206,207}
SPv0 = [k for k in range(208) if k not in FUNC]
f = lambda x,y: y % 2 == 0

s = '12345678901234567890123456789012345'
m = render(s, level=1, mask=0, version=4)
E = expected_stream(s)
print("len(E)", len(E))

def score(SPv):
    dev = 0
    for i in range(len(E)):
        x, y = PATH[SPv[i]]
        if (m[y][x] ^ (1 if f(x,y) else 0)) != E[i]: dev += 1
    return dev

print("base dev:", score(SPv0))

# H1: remove one cell near index 92-99 (it's a skip)
best = []
for pos in range(90, 102):
    SP = SPv0[:pos] + SPv0[pos+1:]
    best.append((score(SP), f"skip stream idx {pos} = {PATH[SPv0[pos]]}"))
# H2: insert an extra cell near the boundary
CAND = [(12,0),(11,0),(10,0),(9,0),(10,1),(9,1),(12,1),(11,1),(10,16),(9,16)]
kmap = {v:k for k,v in enumerate(PATH)}
for pos in range(92, 99):
    for c in CAND:
        SP = SPv0[:pos] + [kmap[c]] + SPv0[pos:]
        best.append((score(SP), f"insert {c} at idx {pos}"))
best.sort()
print("\ntop hypotheses:")
for sc, desc in best[:8]: print(f"  dev={sc:3}  {desc}")

# H3: strip 3 swapped pair order / reversed direction
# build alternative SPv: strips 0-2 same, strip 3+ variants
def strips_spv(strip3_mode):
    def funct(x,y): return (x<=7 and y<=7) or (x==8 and y<=8) or (y==8 and x<=7)
    STR = [(16,15),(14,13),(12,11),(10,9),(8,7),(6,5),(4,3),(2,1),(0,None)]
    P = []
    for si,(hi,lo) in enumerate(STR):
        if si == 3 and strip3_mode == 'swap': hi, lo = lo, hi
        rows = range(16,-1,-1) if si%2==0 else range(17)
        if si == 3 and strip3_mode == 'down': rows = range(16,-1,-1)
        for y in rows:
            for x in ((hi,) if lo is None else (hi,lo)):
                if not funct(x,y): P.append((x,y))
    return P
for mode in ('base','swap','down'):
    P = strips_spv(mode)
    SP = [kmap[c] for c in P if c in kmap]
    print(f"strip3 {mode}: dev={score(SP)}")
