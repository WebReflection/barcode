"""Unit-vector attack v2: only trust ROW-EVEN renders (law verified global).
Sweep probe byte values per unit position until a row-even render appears."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import build, gf_mul, NECC, LAW
from funcsolve import expected_stream, PATH
from mqr_probe import render

FUNC = {0,1,32,33,34,35,100,101,102,103,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]

def stream_law(s, lv):
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: return None, None
    if len(m) != 17: return None, None
    E = expected_stream(s)
    for name, f in LAW.items():
        t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
        if t[:len(E)] == E: return name, t
    return None, None

def cws_of(t, off, n):
    return [int(''.join(map(str, t[off+8*k:off+8*k+8])), 2) for k in range(n)]

PROBES = ['\x01', '\x02', '\x04', '\x08', '\x10', 'A', 'B', 'Q', '!', '\x7f']
units = {}
for i in range(9):
    for pb in PROBES:
        b = ['\x00']*9; b[i] = pb
        s = ''.join(b)
        name, t = stream_law(s, 3)
        if name == 'row-even':
            units[i] = (pb, t); break
print("unit renders (row-even):", {i: repr(v[0]) for i, v in units.items()})

if 0 in units:
    pb, t = units[0]
    obs = cws_of(t, 78, 6)
    print("e_0 observed parity top-6:", [hex(v) for v in obs])
    POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
           0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
           0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]
    hit = False
    for poly in POLYS:
        exp, log = build(poly)
        for fcr in range(16):
            for u in range(1, 17):
                g = [1]
                for k in range(14):
                    a = exp[(fcr+u*k) % 255]
                    nxt = [0]*(len(g)+1)
                    for jx, c in enumerate(g):
                        nxt[jx+1] ^= c; nxt[jx] ^= gf_mul(c, a, exp, log)
                    g = nxt
                for cand in (g[13:7:-1], g[6::-1][:0] or g[0:6], g[13:7:-1][::-1]):
                    if cand == obs:
                        print(f"*** GEN MATCH poly=0x{poly:x} fcr={fcr} u={u}")
                        hit = True
    if not hit: print("no generator match")
# also dump all unit parities for later linear algebra
for i in sorted(units):
    pb, t = units[i]
    e = cws_of(t, 78, 14)
    print(f"i={i} pb={pb!r} ecc={[hex(v) for v in e]}")
