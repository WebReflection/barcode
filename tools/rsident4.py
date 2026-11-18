"""Test: RS input = [00hhhhhh] + 9 payload CWs (header own leading CW, low bits)."""
import sys, json
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import build, gf_mul, encode

cols = {int(k): v for k,v in json.load(open('/tmp/opencode/cols5.json')).items()}
# payload bit j -> CW 1 + j//8, bit p = j%8 (MSB-first), symbol 2^(7-p)
POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
       0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
       0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]
for poly in POLYS:
    exp, log = build(poly)
    for fcr in range(16):
        for u in range(1, 17):
            g_asc = [1]
            for i in range(14):
                a = exp[(fcr+u*i) % 255]
                nxt = [0]*(len(g_asc)+1)
                for k,c in enumerate(g_asc):
                    nxt[k+1] ^= c; nxt[k] ^= gf_mul(c, a, exp, log)
                g = nxt[::-1]
            ok = True; checked = 0
            for m in range(10):
                up = encode([1 if i==m else 0 for i in range(10)], 14, g, exp, log)
                for p in range(8):
                    if m == 0: continue    # header CW bits not probed
                    j = (m-1)*8 + p
                    if j in cols:
                        fe = 1 << (7-p)
                        pred = [gf_mul(up[k], fe, exp, log) for k in range(14)]
                        checked += 1
                        if pred != cols[j]: ok = False
            if ok and checked > 20:
                print(f"*** MATCH poly=0x{poly:x} fcr={fcr} u={u} checked={checked}")
                sys.exit(0)
print("no match (header-CW-first)")
