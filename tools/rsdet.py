"""Deterministic RS identification from full unit parities.
Q_m = col_{8m} * inv(0x80)  (payload byte m = input CW m, grid-6).
H1: Q_8 = x^14 mod g  -> g = x^14 + Q_8; check Q_m = x^(22-m) mod g.
H2: Q_8 = x^15 mod g  -> g | (x^15 + Q_8); factor, check Q_m = x^(23-m) mod g.
"""
import sys, json
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import build, gf_mul, encode

cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_deep.json')).items()}
exp, log = build(0x11d)
def inv(v): return exp[(255 - log[v]) % 255]
def mul(a,b): return gf_mul(a,b,exp,log)

Q = {}
for m in range(9):
    Q[m] = [mul(v, inv(0x80)) for v in cols[8*m]]
    print(f"Q_{m} =", [hex(v) for v in Q[m]])

def polysub_scale(vec, s):
    """vec * s as polynomial coefficients MSB-first (byte k = x^(13-k))."""
    return [mul(v, s) for v in vec]

def xk_mod_g(k, g_asc, exp, log):
    """x^k mod g via repeated multiplication; g ascending coeffs, monic deg 14."""
    # represent remainder as ascending coeffs length 14
    r = [0]*14
    if k < 14: r[k] = 1; return r
    r[0] = 1  # x^1? start with x^0 and step
    r = [1] + [0]*13  # polynomial "1"
    for _ in range(k):
        # multiply by x: shift ascending, then reduce top (x^14 = g_13 x^13+...+g_0)
        top = r[13]
        r = [0] + r[:13]
        if top:
            for i in range(14):
                r[i] ^= gf_mul(top, g_asc[i], exp, log)
    return r

def parity_from_rem(rem_asc):
    """remainder ascending coeffs -> MSB-first parity bytes (14)."""
    return [rem_asc[13-k] for k in range(14)]

import sys as _s
for H in (14, 15):
    print(f"--- H: Q_8 = x^{H} mod g ---")
    if H == 14:
        g_asc = [Q[8][13-k] for k in range(14)] + [1]
    else:
        # F = x^15 + Q_8; find roots c, g = F/(x+c)
        F_asc = [0]*16
        F_asc[15] = 1
        for k in range(14): F_asc[k] = Q[8][13-k]
        roots = []
        for c in range(1, 256):
            acc = 0
            for coef in F_asc:   # Horner MSB-first
                acc = gf_mul(acc, c, exp, log) ^ coef
            if acc == 0: roots.append(c)
        print("  roots:", [hex(c) for c in roots])
        found = False
        for c in roots:
            rem = list(F_asc)
            for i in range(15):
                rem[i+1] ^= gf_mul(rem[i], c, exp, log)
            if rem[15] != 0 or rem[14] != 1: continue
            g_asc = rem[:15]
            # verify all Q_m = x^(23-m) mod g
            ok = True
            for m in range(9):
                r = xk_mod_g(23-m, g_asc, exp, log)
                if parity_from_rem(r) != Q[m]: ok = False; break
            if ok:
                print(f"  *** H2 MATCH c={c:#x} g_asc={[hex(v) for v in g_asc]}")
                found = True
        if not found: print("  no root verified"); continue
    # verify H1 recurrence for all m
    ok = True
    for m in range(9):
        r = xk_mod_g(22-m if H==14 else 23-m, g_asc, exp, log)
        if parity_from_rem(r) != Q[m]: ok = False; print(f"  Q_{m} MISMATCH"); break
    if ok:
        print(f"*** H{H-13} FULL MATCH: g ascending = {[hex(v) for v in g_asc]}")
        break
else:
    print("neither hypothesis matched")
