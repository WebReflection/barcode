"""Factor the codeword polynomial: evaluate C(x) at every field element for
each candidate poly; the RS generator roots must appear as a 14-term
arithmetic progression among the zero exponents."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import samples, build, NDATA, NECC

def eval_poly(cws, exp, log, e):
    # C(x) = sum cws[i] * x^(n-1-i), Horner
    a = exp[e % 255]
    acc = 0
    for c in cws:
        if acc == 0:
            acc = c
        else:
            acc = exp[(log[acc] + log[a]) % 255] ^ c
    return acc

def analyze(cws, poly, label):
    exp, log = build(poly)
    zeros = [e for e in range(255) if eval_poly(cws, exp, log, e) == 0]
    # find arithmetic progressions of length >= NECC among zeros
    zs = set(zeros)
    found = []
    for e0 in zeros:
        for u in range(1, 32):
            if all((e0 + u*k) % 255 in zs for k in range(NECC[label])):
                found.append((e0, u))
    return zeros, found

for lv in (3, 2, 1):
    s, d, e = samples[lv][0]
    cws = d + e
    print(f"=== lv{lv} {s!r}: {len(d)}+{len(e)} CWs ===")
    for poly in (0x11d, 0x11b):
        zeros, found = analyze(cws, poly, lv)
        print(f"  poly 0x{poly:x}: {len(zeros)} roots: {zeros[:20]}")
        if found: print(f"    PROGRESSIONS: {found[:6]}")
    # bit-reversed variant
    rb = lambda v: int(f"{v:08b}"[::-1], 2)
    cws_r = [rb(v) for v in cws]
    for poly in (0x11d,):
        zeros, found = analyze(cws_r, poly, lv)
        print(f"  poly 0x{poly:x} revbits: {len(zeros)} roots: {zeros[:20]}")
        if found: print(f"    PROGRESSIONS: {found[:6]}")
