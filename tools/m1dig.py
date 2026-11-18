"""M1: probe all '0000d', pairwise diff sizes -> same-mask cliques."""
import sys, itertools
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

mats = {}
for d in range(10):
    mats[d] = render(f'0000{d}', level=1, mask=0, version=1)

def diffset(a, b):
    return {(x, y) for y in range(11) for x in range(11) if mats[a][y][x] != mats[b][y][x]}

print("pairwise diff sizes (last digit):")
for a, b in itertools.combinations(range(10), 2):
    d = diffset(a, b)
    print(f"  {a}-{b}: {len(d)}")
