"""Linearity among row-even renders only:
predicted ecc(v x9)[0:6] = v * inv(2) * sum_i p_i_obs[0:6]
Find row-even same-byte payloads and check."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of
from rssolve2 import build, gf_mul

def payload_bytes(vals):
    return ''.join(chr(v & 0xff) for v in vals)

exp, log = build(0x11d)

# unit parities (value v at pos i) -> p_i = inv(v) * obs
PROBES = [2, 1, 4, 8, 16, 32, 64, 128, 3, 5, 6, 9, 10, 12, 17, 0x41, 0x42, 0x7e]
p_i = {}
for i in range(9):
    for v in PROBES:
        vals = [v if j == i else 0 for j in range(9)]
        name, t = stream_law(payload_bytes(vals), 3)
        if name == 'row-even':
            e = cws_of(t, 78, 6)
            invV = exp[(255 - log[v]) % 255]
            p_i[i] = [gf_mul(x, invV, exp, log) for x in e]
            break
print("units complete:", sorted(p_i) == list(range(9)))

S = [0]*6
for i in range(9):
    for k in range(6):
        S[k] ^= p_i[i][k]

# candidate same-byte payloads, keep row-even ones
tested = 0
for v in [1, 2, 3, 4, 5, 6, 8, 0x10, 0x20, 0x40, 0x41, 0x7e, 0x21, 0x5a]:
    vals = [v]*9
    name, t = stream_law(payload_bytes(vals), 3)
    if name != 'row-even': continue
    obs = cws_of(t, 78, 6)
    vinv = exp[(255 - log[v]) % 255]
    pred = [gf_mul(S[k], v, exp, log) for k in range(6)]  # v * sum p_i
    tested += 1
    print(f"v={v:#04x}: pred={[hex(x) for x in pred]} obs={[hex(x) for x in obs]} match={pred==obs}")
print("tested", tested)
