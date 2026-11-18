"""RS parameter solve for Micro QR M4 ECC.

CW grid starts at bit 6; L/M/Q = 15/13/9 data CW + 8/10/14 ecc CW.
For each render: unmask stream with its law, slice data CWs (input) and
ecc CWs (output), then brute-force (poly, fcr) so that
ecc = (data * x^n_ecc) mod g(x), g = prod_{i<n_ecc} (x - a^(fcr+i)).
"""
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

NDATA = {1:15, 2:13, 3:9}; NECC = {1:8, 2:10, 3:14}

def cw_stream(s, lv):
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: return None
    if len(m) != 17: return None
    E = expected_stream(s)
    for name, f in LAW.items():
        t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
        if t[:len(E)] == E:
            return t
    return None

def slice_cws(t):
    return [int(''.join(map(str, t[6+8*k:14+8*k])), 2) for k in range(23)]

# collect samples per level: diverse payloads (vary data widely)
SAMP = {
 1: ['a','aa','!!!','1234567890','abcdefghi','12345678901234567890123456789012345',
     '307418529630741852963074185296','ZZZZZ','$ $ $ $ $','[[]][][]','ABCDEFGH'],
 2: ['a','aa','!!!','1234567890','abcdefghi','3074185296307418529630741852',
     '0123456789','HELLO WORLD','!!!!','~~~~~~~~~'],
 3: ['a','aa','aaa','!!!!','1234567890','aaaaaaaaa','ABCDEFGH','[[]][][]',
     '307418529630','HELLO, WORLD!'],
}
samples = {}   # lv -> list of (data_cws, ecc_cws)
for lv, pay in SAMP.items():
    samples[lv] = []
    for s in pay:
        t = cw_stream(s, lv)
        if not t: continue
        cws = slice_cws(t)
        nd, ne = NDATA[lv], NECC[lv]
        samples[lv].append((s, cws[:nd], cws[nd:nd+ne]))
    print(f"lv{lv}: {len(samples[lv])} samples")

POLYS = [0x11d, 0x11b, 0x12b, 0x12d, 0x14d, 0x15f, 0x163, 0x165, 0x169, 0x171, 0x187, 0x18d]

def rs_ecc(data, ne, exp, fcr):
    # generator = prod (x - a^(fcr+i)); ecc = data*x^ne mod g
    # poly arithmetic in GF(256), MSB-first
    g = [1]
    for i in range(ne):
        root = exp[(fcr+i) % 255]
        ng = [0]*(len(g)+1)
        for j, c in enumerate(g):
            ng[j] ^= gf_mul(c, root, exp, log)
            ng[j+1] ^= c
        g = ng
    # g MSB-first with leading 1 at index ne; divide data*x^ne by g
    rem = list(data) + [0]*ne
    for i in range(len(data)):
        c = rem[i]
        if c == 0: continue
        for idx in range(ne):          # g[idx] aligns at position i+ne-idx
            rem[i+ne-idx] ^= gf_mul(c, g[idx], exp, log)
    return rem[len(data):]

def build_tables(poly):
    exp = [0]*512; log = [0]*256
    x = 1
    for i in range(255):
        exp[i] = x; log[x] = i
        x <<= 1
        if x & 0x100: x ^= poly
    for i in range(255, 512): exp[i] = exp[i-255]
    return exp, log

def gf_mul(a, b, exp, log):
    if a == 0 or b == 0: return 0
    return exp[log[a] + log[b]]

for poly in POLYS:
    exp, log = build_tables(poly)
    for fcr in range(0, 8):
        ok = True
        for lv in (1,2,3):
            for s, d, e in samples[lv]:
                calc = rs_ecc(d, NECC[lv], exp, fcr)
                if calc != e: ok = False; break
            if not ok: break
        if ok:
            print(f"\n*** MATCH: poly=0x{poly:x} fcr={fcr}")
            sys.exit(0)
# partial diagnostics: best per (poly,fcr) for lv3 only
print("\nno global match; per-sample detail for poly=0x11d:")
exp, log = build_tables(0x11d)
for fcr in range(4):
    hits = 0; tot = 0
    for lv in (1,2,3):
        for s, d, e in samples[lv]:
            tot += 1
            if rs_ecc(d, NECC[lv], exp, fcr) == e: hits += 1
    print(f"  fcr={fcr}: {hits}/{tot}")
