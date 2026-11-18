"""M1: identify mask law per render; '00000' data should be ~all zeros."""
import sys
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

# funct/fixed per the observed map: 8x8 block + (8,0),(9,0),(10,0),(8,8)
# candidate data cells = everything else (47)
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [(x, y) for y in range(11) for x in range(11) if (x, y) not in FIXED]
print("data cells:", len(DATA))

laws = {
    'row-even': lambda x, y: y % 2 == 0,
    '(xy)%6<3': lambda x, y: (x * y) % 6 < 3,
    'coarse':   lambda x, y: (x // 3 + y // 2 + 1) % 2 == 1,
    'ABC':      lambda x, y: ((x * y) % 2 + (x * y) % 3) % 2 == 0,
}

for s in ('00000', '00002', '00003', '10000', '12345'):
    m = render(s, level=1, mask=0, version=1)
    print(s)
    for name, f in laws.items():
        bits = [m[y][x] ^ (1 if f(x, y) else 0) for x, y in DATA]
        z = bits.count(0)
        print(f"  {name:10}: zeros {z}/47  {''.join(map(str,bits))}")
