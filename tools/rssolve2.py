"""RS solve v2: faithful port of barcode-qr's proven rs_encode (LFSR style),
plus layout variants (bit-reversed CWs, reversed CW order)."""
import sys, itertools
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
NDATA = {1:15, 2:13, 3:9}; NECC = {1:8, 2:10, 3:14}

def cw_stream(s, lv):
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: return None
    if len(m) != 17: return None
    E = expected_stream(s)
    for name, f in LAW.items():
        t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
        if t[:len(E)] == E: return t
    return None

def slice_cws(t, revbits=False):
    out = []
    for k in range(23):
        bits = t[6+8*k:14+8*k]
        if revbits: bits = bits[::-1]
        out.append(int(''.join(map(str, bits)), 2))
    return out

SAMP = {
 1: ['a','!!!','1234567890','abcdefghi','12345678901234567890123456789012345',
     'ZZZZZ','$ $ $ $ $','[[]][][]'],
 2: ['a','!!!','1234567890','abcdefghi','3074185296307418529630741852','HELLO WORLD'],
 3: ['a','aaa','!!!!','1234567890','aaaaaaaaa','307418529630'],
}
samples = {}
for lv, pay in SAMP.items():
    samples[lv] = []
    for s in pay:
        t = cw_stream(s, lv)
        if not t: continue
        c = slice_cws(t)
        samples[lv].append((s, c[:NDATA[lv]], c[NDATA[lv]:NDATA[lv]+NECC[lv]]))
    print(f"lv{lv}: {len(samples[lv])} samples")

def build(poly):
    exp = [0]*512; log = [0]*256
    x = 1
    for i in range(255):
        exp[i] = x; log[x] = i
        x <<= 1
        if x & 0x100: x ^= poly
    for i in range(255,512): exp[i] = exp[i-255]
    return exp, log

def gf_mul(a,b,exp,log):
    if a==0 or b==0: return 0
    return exp[log[a]+log[b]]

def gen(exp,log,nn,fcr):
    g=[1]
    for j in range(nn):
        a=exp[(fcr+j)%255]
        nxt=[0]*(len(g)+1)
        for i,c in enumerate(g):
            nxt[i+1]^=c; nxt[i]^=gf_mul(c,a,exp,log)
        g=nxt
    return g[::-1]

def encode(data,nn,g,exp,log):
    rem=[0]*nn
    for d in data:
        f=d^rem[0]
        rem=rem[1:]+[0]
        for i,gc in enumerate(g[1:]):
            rem[i]^=gf_mul(f,gc,exp,log)
    return rem

POLYS=[0x11d,0x11b,0x12b,0x12d,0x14d,0x15f,0x163,0x165,0x169,0x171,0x187,0x18d]
VARIANTS = ['plain','revbits','rev-ecc-order','rev-data-order','revbits+rev-ecc']

for poly in POLYS:
    exp,log=build(poly)
    for fcr in range(8):
        gs = {ne: gen(exp,log,ne,fcr) for ne in (8,10,14)}
        for var in VARIANTS:
            ok=True
            for lv in (1,2,3):
                for s,d,e in samples[lv]:
                    dd,ee = d,e
                    if 'revbits' in var:
                        rb=lambda v: int(f"{v:08b}"[::-1],2)
                        dd=[rb(v) for v in dd]; ee=[rb(v) for v in e]
                    if 'rev-data' in var: dd=dd[::-1]
                    calc=encode(dd,NECC[lv],gs[NECC[lv]],exp,log)
                    if 'rev-ecc' in var: ee=ee[::-1]
                    if calc!=ee: ok=False; break
                if not ok: break
            if ok:
                print(f"\n*** MATCH poly=0x{poly:x} fcr={fcr} variant={var}"); sys.exit(0)
print("no match in searched space")
# diagnostic: show one example's data/ecc
s,d,e = samples[3][0]
print(f"example lv3 {s!r}: data={[hex(v) for v in d]}")
print(f"                  ecc ={[hex(v) for v in e]}")
