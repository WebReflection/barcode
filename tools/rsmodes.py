"""Verify alnum+numeric end-to-end at all levels; probe count overflow."""
import sys, random, string
sys.path.insert(0, "/tmp/opencode")
from bprobe import cws_of, LAW, SPv
from funcsolve import expected_stream, PATH
from mqr_probe import render
from rssolve2 import build, gf_mul, encode

exp, log = build(0x11d)
def mul(a,b): return gf_mul(a,b,exp,log)
def inv(v): return exp[(255 - log[v]) % 255]
import json
cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_deep.json')).items()}
Q8 = [mul(v, inv(0x80)) for v in cols[64]]
g_asc = [Q8[13-k] for k in range(14)] + [1]
def gen(ne):
    g = [1]
    for i in range(ne):
        a = exp[i]; nxt = [0]*(len(g)+1)
        for k,c in enumerate(g): nxt[k+1] ^= c; nxt[k] ^= mul(c, a)
        g = nxt
    return g
GEN = {1: gen(8), 2: gen(10), 3: gen(14)}
NDATA = {1:15, 2:13, 3:9}; NECC = {1:8, 2:10, 3:14}

def check(s, lv):
    """returns (ok, skip, X_used)"""
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: return None, None, None
    if len(m) != 17: return None, None, None
    E = expected_stream(s)
    tl = None
    for name, f in LAW.items():
        tt = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
        if tt[:len(E)] == E: tl = tt; break
    if tl is None: return None, None, None
    nd, ne = NDATA[lv], NECC[lv]
    obs = cws_of(tl, 6+8*nd, ne)
    dcw = cws_of(tl, 6, nd)
    if s.isdigit(): X = 0x00 | (len(s)//2)
    elif all(c in AL for c in s): X = 0x20 | len(s)
    else: X = 0x40 | len(s)
    pred = encode([X] + dcw, ne, GEN[lv][::-1], exp, log)
    return pred == obs, False, X

AL = string.ascii_uppercase + string.digits + ' $%*+-./:'
random.seed(3)
print("mode  lv  len  result  X")
for mode, gen_s in (('alnum', lambda n: ''.join(random.choice(AL) for _ in range(n))),
                    ('num', lambda n: ''.join(random.choice('123456789') for _ in range(n)))):
    for lv in (1, 2, 3):
        caps = {1: (21, 35), 2: (18, 30), 3: (13, 21)}[lv]
        cap = caps[0] if mode == 'alnum' else caps[1]
        for n in {2, 5, cap-1, cap}:
            for trial in range(8):
                s = gen_s(n)
                r, skip, X = check(s, lv)
                if r is None: continue
                print(f"{mode:5} {lv}   {n:3}  {'OK' if r else 'BAD'}     {X:#04x}")
                break
            else:
                print(f"{mode:5} {lv}   {n:3}  no-render")
