"""Recover Q_m (unit-CW parities) and identify the RS generator.
Framing: payload byte k = input CW (grid-6), header = extra CW (first or last).
Header-first: Q_m = x^(22-m) mod g  -> g = x^14 + Q_8 directly.
Header-last:  Q_m = x^(23-m) mod g  -> x^15 + Q_8 divisible by g.
"""
import sys, json
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import build, gf_mul, encode

cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_full.json')).items()}
exp, log = build(0x11d)
def inv(v): return exp[(255 - log[v]) % 255]
def mul(a,b): return gf_mul(a,b,exp,log)

Q = {}
for m in range(9):
    for p in range(8):
        j = 8*m + p
        fe = 1 << (7-p)
        q = [mul(v, inv(fe)) for v in cols[j]]
        if m in Q: assert Q[m] == q, (m, p)
        else: Q[m] = q
print("Q_8 =", [hex(v) for v in Q[8]])
print("Q_0 =", [hex(v) for v in Q[0]])

# ---- hypothesis header-FIRST: g = x^14 + Q_8 (Q_m = x^(22-m) mod g) ----
def polydivmod(num, den, exp, log):
    # num, den MSB-first coefficient lists; den monic
    rem = list(num)
    dn, dd = len(num)-1, len(den)-1
    for i in range(dn - dd + 1):
        c = rem[i]
        if c == 0: continue
        for k in range(1, dd+1):
            rem[i+k] ^= gf_mul(den[k], c, exp, log)
    return rem[dn-dd+1:]

for poly in (0x11d, 0x11b, 0x12b, 0x12d):
    e2, l2 = build(poly)
    g = [1] + [0]*13 + [0]  # placeholder
    # g = x^14 + Q_8: Q_8 are coefficients of x^13..x^0 (MSB-first parity bytes)
    # Q_8 as polynomial: coeff of x^(13-k) = Q_8[k]
    g_asc = [0]*15           # ascending: g_asc[i] = coeff of x^i
    g_asc[14] = 1
    for k in range(14): g_asc[13-k] = Q[8][k]
    # verify: Q_m should equal parity of unit at position m (n=10, header first)
    ok = True
    for m in range(9):
        up = encode([1 if i==m+1 else 0 for i in range(10)], 14, g_asc[::-1], e2, l2)
        if up[:6] != Q[m]: ok = False; break
    print(f"header-first poly {poly:#x}: {'MATCH' if ok else 'no'}")

# ---- hypothesis header-LAST: x^15 + Q_8 divisible by g ----
# F(x) = x^15 + Q_8(x); find roots c; g = F/(x+c); verify with Q_7 = x^16 mod g
e2, l2 = exp, log
F = [0]*16
F[15] = 1
for k in range(14): F[13-k] = Q[8][k]
roots = []
for c in range(1, 256):
    # evaluate F at alpha^? -- evaluate at field element c via Horner
    acc = 0
    for coef in F:
        acc = gf_mul(acc, c, e2, l2) ^ coef
    if acc == 0: roots.append(c)
print("roots of x^15+Q_8:", [hex(r) for r in roots])
for c in roots:
    # g = F / (x + c)
    quot = [0]*15
    rem = list(F)
    # synthetic division by (x + c)
    for i in range(15):
        rem[i+1] ^= gf_mul(rem[i], c, e2, l2)
    quot = rem[:15]
    if rem[15] != 0: continue
    g_asc = quot  # ascending coeffs, degree 14, monic?
    if g_asc[14] != 1: continue
    ok = True
    for m in range(9):
        up = encode([1 if i==m else 0 for i in range(10)], 14, g_asc[::-1], e2, l2)
        if up[:6] != Q[m]: ok = False; break
    if ok:
        print(f"*** header-LAST MATCH with c={c:#x}: g ascending = {[hex(v) for v in g_asc]}")
