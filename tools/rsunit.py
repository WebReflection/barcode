"""Unit-vector attack: 9-byte lv3 payloads map exactly to 9 data CWs.
Input e_i (single 0x01 at position i) -> parity = x^(14+i) mod g.
For e_0: parity = g's low coefficients (g monic deg 14) -> compare the
TRUSTED top-6 parity CWs against candidate generators directly."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import build, gf_mul, encode, gen, NECC, NDATA, LAW
from funcsolve import expected_stream, PATH
from mqr_probe import render

FUNC = {0,1,32,33,34,35,100,101,102,103,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]

def raw_stream(s, lv):
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: return None
    if len(m) != 17: return None
    E = expected_stream(s)
    for name, f in LAW.items():
        t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
        if t[:len(E)] == E: return t
    return None

def cws_of(t, off, n):
    return [int(''.join(map(str, t[off+8*k:off+8*k+8])), 2) for k in range(n)]

# unit inputs; try \x01 and printable alternatives
units = []
for i in range(9):
    for probe in ('\x01', 'A'):
        b = ['\x00']*9
        b[i] = probe
        s = ''.join(b)
        t = raw_stream(s, 3)
        if t:
            units.append((i, probe, s, t))
            break
print(f"{len(units)} unit renders")
for i, probe, s, t in units[:3]:
    d = cws_of(t, 6, 9)
    e = cws_of(t, 78, 14)
    print(f"  i={i} probe={probe!r}: data={[hex(v) for v in d]}")
    print(f"                     ecc ={[hex(v) for v in e]}")

# expected generator low coeffs for candidates
POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
       0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
       0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]
if units:
    i, probe, s, t = units[0]
    if i == 0:
        obs = cws_of(t, 78, 6)   # trusted top-6 parity CWs = g13..g8
        print("observed g13..g8:", [hex(v) for v in obs])
        for poly in POLYS:
            exp, log = build(poly)
            for fcr in range(16):
                for u in range(1, 17):
                    # generator ascending coefficients: prod (x - a^(fcr+u i))
                    g = [1]
                    for k in range(14):
                        a = exp[(fcr+u*k) % 255]
                        nxt = [0]*(len(g)+1)
                        for jx, c in enumerate(g):
                            nxt[jx+1] ^= c; nxt[jx] ^= gf_mul(c, a, exp, log)
                        g = nxt
                    # g ascending: g[14]=1 (x^14), g[13..0] low coeffs
                    # parity of e_0 = remainder of x^14 = g_13..g_0 -> first 6 CWs = g13..g8
                    if g[13:7:-1] == obs or g[13:6:-1][:6] == obs:
                        pass
                    # ascending list: index j = coeff of x^j; g[13] = coeff x^13
                    cand = [g[13-j] if False else g[13-j] for j in range(6)]
                    # first parity CW = coeff of x^13, then x^12, ... x^8
                    cand = g[13:7:-1]
                    if cand == obs:
                        print(f"*** GEN MATCH poly=0x{poly:x} fcr={fcr} u={u}")
                        sys.exit(0)
        print("no generator match on unit e_0")
