"""Finish columns; identify RS with n=10 grid-0 input CWs (bits[0:80], last 2 zero)."""
import sys, random, json
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of
from rssolve2 import build, gf_mul, encode

def payload(setbits):
    b = ['\x00']*9
    for j in setbits: b[j//8] = chr(ord(b[j//8]) | (1 << (7 - j%8)))
    return ''.join(b)

def ecc_of(setbits):
    name, t = stream_law(payload(setbits), 3)
    if name != 'row-even': return None
    return cws_of(t, 78, 14)

cols = {int(k): v for k,v in json.load(open('/tmp/opencode/cols4.json')).items()}
random.seed(101)
for trial in range(400):
    if len(cols) == 72: break
    base = set(random.sample(range(72), random.randint(6, 40)))
    eb = ecc_of(base)
    if eb is None: continue
    for j in range(72):
        if j in cols: continue
        e = ecc_of(base ^ {j})
        if e: cols[j] = [x^y for x,y in zip(eb, e)]
print(f"columns: {len(cols)}/72")
json.dump({str(k): v for k,v in cols.items()}, open('/tmp/opencode/cols5.json','w'))

# q_m for n=10 grid-0 input: payload bit j -> stream 6+j -> CW (6+j)//8
q = {}
for m in range(10):
    qq = [None]*8
    for p in range(8):
        j = 8*m + p - 6
        if j in cols: qq[p] = cols[j]
    q[m] = qq

POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
       0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
       0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]
for poly in POLYS:
    exp, log = build(poly)
    for fcr in range(16):
        for u in range(1, 17):
            g_asc = [1]
            for i in range(14):
                a = exp[(fcr+u*i) % 255]
                nxt = [0]*(len(g_asc)+1)
                for k,c in enumerate(g_asc):
                    nxt[k+1] ^= c; nxt[k] ^= gf_mul(c, a, exp, log)
                g = nxt[::-1]
            ok = True; checked = 0
            for m in range(10):
                up = encode([1 if i==m else 0 for i in range(10)], 14, g, exp, log)
                for p in range(8):
                    if q[m][p] is not None:
                        checked += 1
                        if up[p] != q[m][p]: ok = False
            if ok and checked > 20:
                print(f"*** MATCH poly=0x{poly:x} fcr={fcr} u={u} checked={checked}")
                sys.exit(0)
print("no RS match (n=10 grid-0)")
