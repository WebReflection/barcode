"""Full identification pipeline:
1) affinity verification on held-out render pairs (trusted prefix)
2) GF(256)-consistency self-test per input byte (framing grid-0)
3) g = x^14 + Q_9 recovery, verify recurrence Q_{m} = x^(14+9-m) mod g
"""
import sys, json, random
sys.path.insert(0, "/tmp/opencode")
from bprobe import ecc_of, payload, stream_law_b, cws_of

PREF = 6
cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_full.json')).items()}

# ---------- 1) affinity on fresh renders ----------
random.seed(5)
ok = bad = 0
for trial in range(40):
    S = set(random.sample(range(72), random.randint(2, 30)))
    T = set(random.sample(range(72), random.randint(2, 30)))
    eS, eT = ecc_of(S), ecc_of(T)
    if eS is None or eT is None: continue
    diff = S ^ T
    if not diff <= set(cols): continue
    pred = [x ^ y for x, y in zip(eS[:PREF], eT[:PREF])]
    real = [0]*PREF
    for j in diff:
        for k in range(PREF): real[k] ^= cols[j][k]
    if pred == real: ok += 1
    else: bad += 1
print(f"affinity: {ok} ok, {bad} bad")

# ---------- 2) consistency per input byte (grid-0) ----------
POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
       0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
       0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]
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

best = None
for poly in POLYS:
    exp, log = build(poly)
    def inv(v): return exp[(255 - log[v]) % 255]
    consistent = 0; total = 0
    for m in range(10):
        qs = []
        for p in range(8):
            j = 8*m + p - 6
            if j in cols:
                fe = 1 << (7-p)
                qs.append([gf_mul(v, inv(fe), exp, log) for v in cols[j]])
        for i in range(1, len(qs)):
            total += 1
            if qs[i] == qs[0]: consistent += 1
    if best is None or consistent > best[0]: best = (consistent, total, poly)
    if total and consistent == total:
        print(f"*** CONSISTENT framing grid-0 with poly=0x{poly:x}: {consistent}/{total}")
        break
    else:
        print(f"poly 0x{poly:x}: consistency {consistent}/{total}")
print("best:", best)
