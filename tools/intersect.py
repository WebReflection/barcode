"""Find T (skip set) that is a valid solution for EVERY long payload."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH
from mqr_probe import render

FUNC = {0,1,32,33,34,35,200,201,202,203,204,205,206,207}
SPv0 = [k for k in range(208) if k not in FUNC]
f = lambda x,y: y % 2 == 0
bit = {}
for k in range(208):
    x, y = PATH[k]; bit[k] = None  # filled per render

def sols_for(m, E, cap=4000, maxskip=6, N=170):
    b = {}
    for k in range(208):
        x, y = PATH[k]; b[k] = m[y][x] ^ (1 if f(x,y) else 0)
    out = []
    def dfs(i, ei, skipped):
        if len(out) > cap: return
        if ei == len(E): out.append(tuple(sorted(skipped))); return
        if i >= N: return
        k = SPv0[i]
        if b[k] == E[ei]: dfs(i+1, ei+1, skipped)
        if len(skipped) < maxskip: dfs(i+1, ei, skipped | {i})
    dfs(0, 0, frozenset())
    return set(out)

pay = [('12345678901234567890123456789012345',1),
       ('307418529630741852963074185296',1),
       ('307418529630741852963074185',1),
       ('3074185296307418529630741852',2),
       ('307418529630741852963074',1),
       ('123456789012345678901234',1)]
common = None
for s, lv in pay:
    m = render(s, level=lv, mask=0, version=4)
    E = expected_stream(s)
    S = sols_for(m, E)
    common = S if common is None else (common & S)
    print(f"n={len(s)} lv={lv} len(E)={len(E)}: {len(S)} sols; running intersection: {len(common)}")
    if common: print("   ", sorted(common)[:10])
print("\nFINAL intersection:", sorted(common) if common else None)
