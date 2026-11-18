"""Framing sweep: predict all 72 columns per framing x params; count matches.
Framings differ in how payload bit j maps to (input CW m, bit p) and n."""
import sys, json
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import build, gf_mul, encode

cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_full.json')).items()}
POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
       0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
       0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]

def unit_parities(exp, log, n, ne, fcr, u):
    """unit_parity[m] = parity of input with symbol 1 at position m (n CWs)."""
    g_asc = [1]
    for i in range(ne):
        a = exp[(fcr+u*i) % 255]
        nxt = [0]*(len(g_asc)+1)
        for k,c in enumerate(g_asc):
            nxt[k+1] ^= c; nxt[k] ^= gf_mul(c, a, exp, log)
        g = nxt[::-1]
    return [encode([1 if i==m else 0 for i in range(n)], ne, g, exp, log) for m in range(n)]

FRAMINGS = {
  'grid0-n10':   (10, lambda j: ((6+j)//8, (6+j)%8)),
  'grid0-n10-lsb':(10, lambda j: ((6+j)//8, 7-(6+j)%8)),
  'pay-n10-hlast':(10, lambda j: (j//8, j%8)),
  'pay-n10-hfirst':(10, lambda j: (j//8+1, j%8)),
  'pay-n10-hlast-lsb':(10, lambda j: (j//8, 7-j%8)),
  'pay-n10-hfirst-lsb':(10, lambda j: (j//8+1, 7-j%8)),
  'pay-n9':      (9,  lambda j: (j//8, j%8)),
}

best = (0,)
for poly in POLYS:
    exp, log = build(poly)
    for fcr in range(16):
        for u in range(1, 17):
            ups = {n: unit_parities(exp, log, n, 14, fcr, u) for n in (9,10)}
            for fname, (n, map_fn) in FRAMINGS.items():
                for rev in (False, True):
                    hits = 0
                    for j, obs in cols.items():
                        m, p = map_fn(j)
                        if m >= n: continue
                        up = ups[n][m]
                        if rev: up = up[::-1]
                        fe = 1 << (7-p)
                        pr = [gf_mul(v, fe, exp, log) for v in up[:6]]
                        if pr == obs[:6] if not rev else pr == obs[:6]:
                            hits += 1
                    if hits > best[0]:
                        best = (hits, poly, fcr, u, fname, rev)
                        if hits == 72:
                            print(f"*** PERFECT: poly=0x{poly:x} fcr={fcr} u={u} framing={fname} rev={rev}")
                            sys.exit(0)
print("best:", best)
