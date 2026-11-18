"""Factor C(x) under order/bit variants: ecc reversed, data reversed,
bit-reversed CWs. Any variant that yields a 14-term root progression
wins. Run for all three levels."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import samples, build, NDATA, NECC

def eval_poly(cws, exp, log, e):
    a = exp[e % 255]; acc = 0
    for c in cws:
        if acc == 0: acc = c
        else: acc = exp[(log[acc] + log[a]) % 255] ^ c
    return acc

def roots_and_progs(cws, poly, ne):
    exp, log = build(poly)
    zs = [e for e in range(255) if eval_poly(cws, exp, log, e) == 0]
    zset = set(zs); progs = []
    for e0 in zs:
        for u in range(1, 32):
            if all((e0+u*k) % 255 in zset for k in range(ne)):
                progs.append((e0, u))
    return zs, progs

POLYS = [0x11d, 0x11b, 0x12b, 0x12d, 0x14d, 0x15f, 0x163, 0x169, 0x171, 0x187]
rb = lambda v: int(f"{v:08b}"[::-1], 2)

for lv in (3, 2, 1):
    s, d, e = samples[lv][0]
    ne = NECC[lv]
    variants = {
        'plain': d+e, 'ecc-rev': d+e[::-1], 'data-rev': d[::-1]+e,
        'both-rev': d[::-1]+e[::-1],
        'bitrev': [rb(v) for v in d+e],
        'bitrev+ecc-rev': [rb(v) for v in d]+[rb(v) for v in e[::-1]],
    }
    print(f"=== lv{lv} {s!r} ===")
    for vn, cws in variants.items():
        for poly in POLYS:
            zs, progs = roots_and_progs(cws, poly, ne)
            tag = f"  {vn:16} 0x{poly:x}: roots={len(zs)}"
            if progs: tag += f"  PROGS={progs[:4]}"
            print(tag)
