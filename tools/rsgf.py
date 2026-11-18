"""GF(256)-linearity cross-check: parity(d) = sum_i d_i * p_i where
p_i = parity(unit e_i). Unit renders used value v -> observed = v*p_i.
So parity(61 x9) = 61 * inv(2) * sum_i p_i^obs  (trusted top-6 only)."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of
from rssolve2 import build, gf_mul

def payload_bytes(vals):
    return ''.join(chr(v) for v in vals)

def collect(units_vals, lv=3):
    out = {}
    for i, vals in units_vals.items():
        name, t = stream_law(payload_bytes(vals), lv)
        if name == 'row-even':
            out[i] = cws_of(t, 78, 6)
    return out

# unit renders: value 2 at position i (0..8), others 0
units = {i: [2 if j == i else 0 for j in range(9)] for i in range(9)}
P = collect(units)
print("row-even unit renders:", sorted(P))
if len(P) == 9:
    exp, log = build(0x11d)
    inv2 = exp[(255 - 1) % 255]
    tot = [0]*6
    for i in sorted(P):
        for k in range(6):
            tot[k] ^= gf_mul(P[i][k], inv2, exp, log)   # sum of p_i
    pred = [gf_mul(t, 61, exp, log) for t in tot]
    print("predicted parity(61x9)[0:6]:", [hex(v) for v in pred])
    # observed
    name, t = stream_law(payload_bytes([0x61]*9), 3)
    print("aaaaa law:", name)
    if t:
        obs = cws_of(t, 78, 6)
        print("observed                 :", [hex(v) for v in obs])
        print("MATCH:", pred == obs)
