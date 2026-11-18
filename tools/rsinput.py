"""Try alternative RS INPUT conventions against trusted ecc prefix.
Inputs: grid-6 payload slices, grid-0 slices (header inside), etc."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import samples, build, gf_mul, encode, NDATA, NECC, LAW
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

def bits2cws(bits, off, n):
    return [int(''.join(map(str, bits[off+8*k:off+8*k+8])), 2) for k in range(n)]

def gen_u(exp,log,nn,fcr,u):
    g=[1]
    for i in range(nn):
        a=exp[(fcr+u*i)%255]
        nxt=[0]*(len(g)+1)
        for k,c in enumerate(g):
            nxt[k+1]^=c; nxt[k]^=gf_mul(c,a,exp,log)
        g=nxt
    return g[::-1]

TRUSTED = {3: 6, 2: 2}
results = []
for poly in (0x11d, 0x11b, 0x12b, 0x12d):
    exp,log = build(poly)
    for fcr in range(8):
        for u in range(1,9):
            score=0; tot=0
            for lv in (3,2):
                T = TRUSTED[lv]
                ne = NECC[lv]
                g = gen_u(exp,log,ne,fcr,u)
                for s in ('a','aa','aaa','!!!!','1234567890','aaaaaaaaa'):
                    t = raw_stream(s, lv)
                    if not t: continue
                    # input variants
                    inputs = {
                      'grid6': bits2cws(t, 6, NDATA[lv]),
                      'grid0': bits2cws(t, 0, NDATA[lv]),
                    }
                    ecc6 = bits2cws(t, 6 + 8*NDATA[lv], ne)   # grid-6 ecc
                    ecc0off = bits2cws(t, 8*NDATA[lv], ne)    # grid-0 ecc (78->cont)
                    for iname, inp in inputs.items():
                        for ename, ee in (('g6',ecc6), ('g0',ecc0off)):
                            calc = encode(inp, ne, g, exp, log)
                            score += sum(1 for i in range(T) if calc[i]==ee[i]); tot += T
                            if score/tot > 0.9:
                                print(f"*** HIT poly={poly:x} fcr={fcr} u={u} in={iname} ecc={ename} score={score}/{tot}")
                                sys.exit(0)
            results.append((score/tot if tot else 0, poly, fcr, u))
results.sort(reverse=True)
print(results[:8])
