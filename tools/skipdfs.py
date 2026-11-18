"""Enumerate ALL small skip-sets S (|S|<=9, cells < 160) such that reading
PATH\\S in order reproduces E for the 35-digit lv1 render (row-even mask).
Report the skip positions across solutions."""
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
bit = {}
for k in range(208):
    x, y = PATH[k]
    bit[k] = m[y][x] ^ (1 if f(x,y) else 0)

N = 160
sols = []
def dfs(i, ei, skipped):
    if len(sols) > 4000: return
    if ei == len(E):
        sols.append(tuple(sorted(skipped))); return
    if i >= N: return
    k = SPv0[i] if i < len(SPv0) else None
    # consume cell i
    if k is not None and bit[k] == E[ei]:
        dfs(i+1, ei+1, skipped)
    # skip cell i
    if len(skipped) < 9:
        dfs(i+1, ei, skipped | {i})

dfs(0, 0, frozenset())
print(f"{len(sols)} solutions (cap 4000)")
from collections import Counter
cnt = Counter()
for sol in sols:
    for i in sol: cnt[i] += 1
print("skip positions by frequency (top 20):")
for i, c in cnt.most_common(20):
    print(f"  stream idx {i} (PATH {SPv0[i]} = {PATH[SPv0[i]]}): {c}/{len(sols)}")
print("\nsolution size distribution:", Counter(len(s0) for s0 in sols))
if sols:
    mn = min(len(s0) for s0 in sols)
    mins = [s0 for s0 in sols if len(s0)==mn][:5]
    print("minimal solutions:", mins)
