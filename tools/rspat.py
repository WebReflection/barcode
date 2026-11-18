"""Solve the leading input CW value X per (mode, count) at lv3.
X = (obs xor parity([0]+dataCWs)) / P0  (linear in X)."""
import sys, random, json
sys.path.insert(0, "/tmp/opencode")
from bprobe import stream_law_b, cws_of
from rssolve2 import build, gf_mul, encode

cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_deep.json')).items()}
exp, log = build(0x11d)
def mul(a,b): return gf_mul(a,b,exp,log)
def inv(v): return exp[(255 - log[v]) % 255]
Q8 = [mul(v, inv(0x80)) for v in cols[64]]
g_asc = [Q8[13-k] for k in range(14)] + [1]
P0 = encode([1,0,0,0,0,0,0,0,0,0], 14, g_asc[::-1], exp, log)

def solve_X(data, lv=3):
    name, t = stream_law_b(data, lv)
    if name != 'row-even': return None
    obs = cws_of(t, 78, 14)
    dcw = cws_of(t, 6, 9)
    base = encode([0] + dcw, 14, g_asc[::-1], exp, log)
    d = [obs[k] ^ base[k] for k in range(14)]
    Hs = [mul(d[k], inv(P0[k])) for k in range(14) if P0[k] != 0]
    return Hs[0] if len(set(Hs)) == 1 else ('INCONSISTENT', Hs[:3])

random.seed(5)
print("=== byte mode, lengths 1..9 ===")
for n in range(1, 10):
    for trial in range(6):
        data = bytes([random.randrange(256) for _ in range(n)])
        X = solve_X(data)
        if X is not None:
            print(f"  len {n}: X = {X:#04x} = {X:08b}  (data {data.hex()})")
            break

print("=== alnum mode (uppercase+digits+space) ===")
import string
AL = string.ascii_uppercase + string.digits + ' $%*+-./:'
for n in (1, 2, 3, 5, 9, 13):
    for trial in range(8):
        data = ''.join(random.choice(AL) for _ in range(n)).encode()
        X = solve_X(data)
        if X is not None:
            print(f"  len {n}: X = {X:#04x} = {X:08b}")
            break

print("=== numeric mode ===")
for n in (1, 2, 3, 5, 9, 21):
    for trial in range(8):
        data = ''.join(random.choice(string.digits) for _ in range(n)).encode()
        X = solve_X(data)
        if X is not None:
            print(f"  len {n}: X = {X:#04x} = {X:08b}")
            break
