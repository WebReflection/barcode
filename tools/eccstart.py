"""Find ECC start per level: EC11 pad run ends where ECC begins.
For each render: stream (any law), find data end, then locate the last
position where the EC11 pattern (aligned) still holds."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH, A
from mqr_probe import render

FUNC = {0,1,32,33,34,35,100,101,102,103,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]
def A_(y): return ((y//3)%2)==1
def B_(y): return (((y-1)//3)%2)==1
def C_(y): return (y%2)==1
LAW = {'row-even': lambda x,y: y%2==0, 'xy%6<3': lambda x,y: (x*y)%6<3,
       'coarse': lambda x,y: (x//3+y//2+1)%2==1,
       'ABC': lambda x,y: [not A_(y),C_(y),B_(y),A_(y),not C_(y),not B_(y)][(16-x)%6]}

def analyze(s, lv):
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: return None
    if len(m) != 17: return None
    E = expected_stream(s)
    for name, f in LAW.items():
        t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
        if t[:len(E)] == E:
            return name, t
    return None

# lv sweep with byte payloads (short data -> long pad region)
for lv in (1,2,3):
    print(f"--- level {lv} ---")
    for n in (1,2,3,4,5,6,7,8,9):
        r = analyze('a'*n, lv)
        if not r: continue
        name, t = r
        L = 6+8*n
        # find EC11 pattern: check t[6+8k : 6+8k+8] == EC or 11 for k
        # report tail from L, and first index where EC11-alignment breaks
        tail = t[L:]
        # EC = 11101100, 11 = 00010001; try both phases & offsets 0..7
        best = None
        for off in range(0,8):
            for ph in (0,1):
                ok = 0
                i = off
                while i+8 <= 190:
                    cw = t[i:i+8]
                    if cw == [1,1,1,0,1,1,0,0] or cw == [0,0,0,1,0,0,0,1]:
                        ok += 8; i += 8
                    else: break
                if best is None or ok > best[0]: best = (ok, off, ph)
        print(f"  n={n} law={name:9} data_end={L:3} padEC11bits={best[0]:3} (from abs {L+ (best[1] if best else 0)}) tail[0:24]={''.join(map(str,tail[:24]))}")
