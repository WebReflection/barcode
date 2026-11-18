"""ECC start per level via cross-level diff of UNMASKED streams.
Pad/terminator is level-independent; ECC differs -> first differing bit
between levels = min(ECC start). Also diff across n for terminator rule."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH
from mqr_probe import render

FUNC = {0,1,32,33,34,35,100,101,102,103,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]
def A_(y): return ((y//3)%2)==1
def B_(y): return (((y-1)//3)%2)==1
def C_(y): return (y%2)==1
LAW = {'row-even': lambda x,y: y%2==0, 'xy%6<3': lambda x,y: (x*y)%6<3,
       'coarse': lambda x,y: (x//3+y//2+1)%2==1,
       'ABC': lambda x,y: [not A_(y),C_(y),B_(y),A_(y),not C_(y),not B_(y)][(16-x)%6]}

def unmasked(s, lv):
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: return None, None
    if len(m) != 17: return None, None
    E = expected_stream(s)
    for name, f in LAW.items():
        t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
        if t[:len(E)] == E: return name, t
    return None, None

for n in (1, 4, 9):
    s = 'a'*n
    streams = {}
    for lv in (1,2,3):
        name, t = unmasked(s, lv)
        if t: streams[lv] = t
    print(f"=== n={n} ===")
    lvs = sorted(streams)
    for i in range(len(lvs)):
        for j in range(i+1, len(lvs)):
            a, b = streams[lvs[i]], streams[lvs[j]]
            d = [k for k in range(190) if a[k] != b[k]]
            print(f"  lv{lvs[i]} vs lv{lvs[j]}: first diff at {d[0] if d else 'NONE'}  "
                  f"ndiff={len(d)} (of 190)")
    # tail after data for lv1
    L = 6+8*n
    t = streams[lvs[0]]
    print(f"  lv{lvs[0]} tail[{L}:] = {''.join(map(str,t[L:]))}")
