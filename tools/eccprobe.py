"""ECC layout probe: for row-even M4 renders, dump stream bits after the
data region (terminator/pad/ECC). Compare payloads of different lengths
(same level) to separate constant pad from data-dependent ECC."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH
from mqr_probe import render

FUNC = {0,1,32,33,34,35,100,101,102,103,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]
f = lambda x,y: y % 2 == 0

def stream_of(s, lv):
    m = render(s, level=lv, mask=0, version=4)
    if len(m) != 17: return None
    E = expected_stream(s)
    t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
    if t[:len(E)] != E: return None          # must be row-even & correct
    return t

# lv3 byte payloads 1..9 chars, keep row-even ones
by_len = {}
for n in range(1,10):
    for base in ('a','!','~','['):
        s = base * n
        t = stream_of(s, 3)
        if t: by_len.setdefault(n, []).append(t)
for n in sorted(by_len):
    ts = by_len[n]
    print(f"n={n}: {len(ts)} row-even renders")
ts = {n: by_len[n][0] for n in sorted(by_len)}
n0 = min(ts)
# cells after data end for each length; mark agreement with the n=1 render
ref = ts[n0]
for n in sorted(ts):
    t = ts[n]; L = 6 + 8*n
    tail = t[L:]
    agree = ''.join('.' if i+L < len(ref) and ref[i+L]==v else ('#' if i+L < len(ref) else ' ') for i,v in enumerate(tail))
    print(f"n={n} data_end={L}: tail[0:40]={''.join(map(str,tail[:40]))}")
    print(f"        vs n={n0} ref  : {agree[:40]}")
# full dump for n=1: entire post-data stream
L = 6+8
print(f"\nfull stream n=1 (data_end={L}):")
print(''.join(map(str, ts[1][L:])))
