"""Complete the unit-parity table: sweep probe values per position until
row-even; then run the GF(256)-linearity cross-check."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of
from rssolve2 import build, gf_mul

def payload_bytes(vals):
    return ''.join(chr(v & 0xff) for v in vals)

PROBES = [2, 1, 4, 8, 16, 32, 64, 128, 3, 5, 6, 9, 10, 12, 17, 0x41, 0x42, 0x7e]
units = {}
for i in range(9):
    for v in PROBES:
        vals = [v if j == i else 0 for j in range(9)]
        name, t = stream_law(payload_bytes(vals), 3)
        if name == 'row-even':
            units[i] = (v, cws_of(t, 78, 6))
            break
print("units:", {i: v[0] for i, v in units.items()})
if len(units) == 9:
    exp, log = build(0x11d)
    tot = [0]*6
    for i, (v, e) in units.items():
        invV = exp[(255 - log[v]) % 255]
        for k in range(6):
            tot[k] ^= gf_mul(e[k], invV, exp, log)
    pred = [gf_mul(t, 0x61, exp, log) for t in tot]
    print("predicted:", [hex(v) for v in pred])
    name, t = stream_law(payload_bytes([0x61]*9), 3)
    print("61x9 law:", name)
    obs = cws_of(t, 78, 6)
    print("observed :", [hex(v) for v in obs])
    print("MATCH:", pred == obs)
