"""Structural RS test from trusted unit parities only.
p_0 = x^14 mod g = g13..g0 (obs top-6 = g13..g8)
p_1 = x^15 mod g = (x + g13)*p_0 mod g -> top coeffs predictable from g13..g8:
  p1[0] = g12 + g13^2
  p1[1] = g11 + g13*g12
  p1[2] = g10 + g13*g11
  p1[3] = g9  + g13*g10
  p1[4] = g8  + g13*g9
"""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import build, gf_mul

p0 = [0x6a, 0xca, 0xea, 0x00, 0xea, 0x11]   # g13..g8 (row-even unit e_0)
p1 = [0xb8, 0x98, 0xa1, 0x2f, 0xac, 0xfa]   # observed top-6 of e_1 parity
g13, g12, g11, g10, g9, g8 = p0

exp, log = build(0x11d)
def mul(a, b): return gf_mul(a, b, exp, log)

pred = [g12 ^ mul(g13, g13), g11 ^ mul(g13, g12), g10 ^ mul(g13, g11),
        g9 ^ mul(g13, g10), g8 ^ mul(g13, g9)]
print("predicted p1[0:5]:", [hex(v) for v in pred])
print("observed  p1[0:5]:", [hex(v) for v in p1])
print("match:", pred == p1[:5])
# also try reversed parity convention
print("rev match:", pred == p1[:5][::-1])
