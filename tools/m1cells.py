"""M1: full 121-cell diffs for same/different mask pairs; find fixed cells."""
import sys
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

def mat(s):
    return render(s, level=1, mask=0, version=1)

def show_diff(a, b):
    m1, m2 = mat(a), mat(b)
    d = [(x, y) for y in range(11) for x in range(11) if m1[y][x] != m2[y][x]]
    print(f"{a} vs {b}: {len(d)} cells: {sorted(d)}")
    return d

d02 = show_diff('00000', '00002')
d03 = show_diff('00000', '00003')
d23 = show_diff('00002', '00003')

# which cells are EVER different across many random payloads?
import random
random.seed(2)
base = mat('00000')
ref = [[base[y][x] for x in range(11)] for y in range(11)]
varying = set()
for t in range(40):
    s = ''.join(random.choice('0123456789') for _ in range(5))
    m = mat(s)
    for y in range(11):
        for x in range(11):
            if m[y][x] != ref[y][x]:
                varying.add((x, y))
fixed = [(x, y) for y in range(11) for x in range(11) if (x, y) not in varying]
print(f"\nfixed across 40 random payloads: {len(fixed)} cells")
print(sorted(fixed))
