"""Complete columns, verify affinity, zero-test j>=66, reconstruct q_m,
and identify RS params by matching unit-CW parities."""
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

cols = {int(k): v for k,v in json.load(open('/tmp/opencode/cols3.json')).items()}
random.seed(23)
for trial in range(200):
    if len(cols) == 72: break
    base = set(random.sample(range(72), random.randint(6, 30)))
    eb = ecc_of(base)
    if eb is None: continue
    for j in range(72):
        if j in cols: continue
        e = ecc_of(base ^ {j})
        if e: cols[j] = [x^y for x,y in zip(eb, e)]
print(f"columns: {len(cols)}/72")
json.dump({str(k): v for k,v in cols.items()}, open('/tmp/opencode/cols4.json','w'))

zero_tail = all(all(v == 0 for v in cols[j]) for j in cols if j >= 66)
print("cols[66..71] all zero (9-CW grid-0 input):", zero_tail,
      [j for j in cols if j >= 66])

# reconstruct q_m (parity of unit input CW m), bits p=0..7 (MSB first)
q = {}
for m in range(0, 9):
    qq = [None]*8
    for p in range(8):
        j = 8*m + p - 6
        if j in cols: qq[p] = cols[j]
    q[m] = qq
# check q_1..q_8 complete
complete = all(all(x is not None for x in q[m]) for m in range(1,9))
print("q_1..q_8 complete:", complete)

# identify: for candidates, unit parity of CW m = x^(ne+8-m) mod g -> MSB-first CWs
POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
       0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
       0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]
def unit_parity(exp, log, ne, g, m):
    # parity of single symbol 1 at data index m (0=first=highest degree)
    return encode([1 if i==m else 0 for i in range(9)], ne, g, exp, log)

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
            ok = True
            for m in range(1, 9):
                up = unit_parity(exp, log, 14, g, m)
                for p in range(8):
                    if q[m][p] is not None and up[p] != q[m][p]:
                        ok = False; break
                if not ok: break
            if ok:
                print(f"*** MATCH poly=0x{poly:x} fcr={fcr} u={u}")
                sys.exit(0)
print("no RS match from unit parities")
