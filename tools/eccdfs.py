"""Find hidden func cells in the ecc region: for candidate RS params,
compute expected parity bits for a unit render; DFS over stream>=78 for a
skip set S (|S|<=14) making observed bits match expected."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import build, gf_mul, encode, gen
from rsunit2 import stream_law

def bits_of(cws):
    out = []
    for c in cws:
        out += [(c >> (7-i)) & 1 for i in range(8)]
    return out

exp, log = build(0x11d)
# unit render e_0: payload 02 + 8 NULs, row-even
name, t = stream_law('\x02' + '\x00'*8, 3)
assert name == 'row-even'
data = [2] + [0]*8
obs = t[78:]
print(f"observed ecc region: {len(obs)} bits")

sols = []
def dfs(i, j, skipped):
    if len(sols) > 200: return
    if j == 112:
        sols.append(tuple(sorted(skipped))); return
    if i >= 112: return
    if obs[i] == None: pass
    # match
    if obs[i] == bits[j]:
        dfs(i+1, j+1, skipped)
    if len(skipped) < 14:
        dfs(i+1, j, skipped | {i})

for poly in (0x11d, 0x11b):
    e2, l2 = build(poly)
    for fcr in (0,1,2,3):
        g = gen(e2, l2, 14, fcr)
        par = encode(data, 14, g, e2, l2)
        bits = bits_of(par)
        sols = []
        dfs(0, 0, frozenset())
        sizes = {}
        for s0 in sols: sizes[len(s0)] = sizes.get(len(s0),0)+1
        print(f"poly={poly:#x} fcr={fcr}: {len(sols)} sols, sizes={sorted(sizes.items())[:5]}")
        if sols:
            mn = min(len(s0) for s0 in sols)
            small = [s0 for s0 in sols if len(s0)==mn][:6]
            print(f"   minimal ({mn}):", small)
