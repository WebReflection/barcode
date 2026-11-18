"""Verify affine constant C; derive header CW value H = C_k / P_k where
P = x^23 mod g (unit parity of input position 0); end-to-end verify."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode")
from bprobe import ecc_of, payload, stream_law_b, cws_of
from rssolve2 import build, gf_mul, encode

cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_deep.json')).items()}
exp, log = build(0x11d)
def inv(v): return exp[(255 - log[v]) % 255]
def mul(a,b): return gf_mul(a,b,exp,log)

# rebuild g from Q_8
Q8 = [mul(v, inv(0x80)) for v in cols[64]]
g_asc = [Q8[13-k] for k in range(14)] + [1]
def xk_mod_g(k):
    r = [1] + [0]*13
    for _ in range(k):
        top = r[13]
        r = [0] + r[:13]
        if top:
            for i in range(14): r[i] ^= mul(top, g_asc[i])
    return r
def unit_pos(i, n=10):
    return encode([1 if t==i else 0 for t in range(n)], 14, g_asc[::-1], exp, log)

P0 = unit_pos(0)   # x^23 mod g: unit parity of input position 0 (header CW)

# measure C = ecc(S) xor sum(col_j, j in S) for random valid renders
Cs = []
random.seed(9)
for trial in range(30):
    S = set(random.sample(range(72), random.randint(0, 30)))
    e = ecc_of(S)
    if e is None: continue
    C = list(e)
    for j in S:
        for k in range(14): C[k] ^= cols[j][k]
    Cs.append(tuple(C))
uniq = set(Cs)
print(f"C constant across {len(Cs)} renders: {len(uniq) == 1}")
C = Cs[0]
print("C =", [hex(v) for v in C])

# H = C_k / P0_k
Hs = []
for k in range(14):
    if P0[k] != 0: Hs.append(mul(C[k], inv(P0[k])))
print("H estimates:", [hex(v) for v in Hs], "consistent:", len(set(Hs)) == 1)
H = Hs[0]
print(f"header CW value: {H:#04x} = {H:08b}")

# END-TO-END: predict ecc for fresh random payloads
ok = bad = 0
random.seed(77)
for trial in range(40):
    data = bytes(random.randrange(256) for _ in range(9))
    name, t = stream_law_b(data, 3)
    if name != 'row-even': continue
    obs = cws_of(t, 78, 14)
    inp = [0x09] + list(data)          # header CW (guess: right-aligned 6 bits) + 9 payload CWs
    pred = encode(inp, 14, g_asc[::-1], exp, log)
    if pred == obs: ok += 1
    else:
        bad += 1
        if bad == 1:
            print("MISMATCH: pred", [hex(v) for v in pred[:6]], "obs", [hex(v) for v in obs[:6]])
print(f"end-to-end: {ok} ok, {bad} bad")
