"""End-to-end verify lv1/lv2: input = [mode_count CW] + data CWs, parity 8/10."""
import sys, random, json
sys.path.insert(0, "/tmp/opencode")
from bprobe import stream_law_b, cws_of
from rssolve2 import build, gf_mul, encode, LAW
from funcsolve import expected_stream, PATH
from mqr_probe import render

cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_deep.json')).items()}
exp, log = build(0x11d)
def mul(a,b): return gf_mul(a,b,exp,log)
def inv(v): return exp[(255 - log[v]) % 255]
Q8 = [mul(v, inv(0x80)) for v in cols[64]]
g_asc = [Q8[13-k] for k in range(14)] + [1]
def gen(ne):
    g = [1]
    for i in range(ne):
        a = exp[(0+i) % 255]
        nxt = [0]*(len(g)+1)
        for k,c in enumerate(g):
            nxt[k+1] ^= c; nxt[k] ^= mul(c, a)
        g = nxt
    return g
GEN = {1: gen(8), 2: gen(10), 3: g_asc}

NDATA = {1: 15, 2: 13, 3: 9}; NECC = {1: 8, 2: 10, 3: 14}

def verify(data: bytes, lv: int, trials=15):
    """data: raw payload bytes (byte mode)."""
    ok = bad = skip = 0
    random.seed(trials)
    for t in range(trials):
        d = bytes(random.randrange(256) for _ in range(len(data)))
        try: m = render(d, level=lv, mask=0, version=4)
        except Exception: skip += 1; continue
        if len(m) != 17: skip += 1; continue
        from bprobe import expected_stream_b; E = expected_stream_b(d)
        tl = None
        for name, f in LAW.items():
            tt = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv_]]
            if tt[:len(E)] == E: tl = tt; break
        if tl is None: skip += 1; continue
        obs = cws_of(tl, 6 + 8*NDATA[lv], NECC[lv])
        dcw = cws_of(tl, 6, NDATA[lv])
        pred = encode([0x40 | len(d)] + dcw, NECC[lv], GEN[lv][::-1], exp, log)
        if pred == obs: ok += 1
        else: bad += 1
    return ok, bad, skip

# SPv needs the same FUNC set
FUNC = {0,1,32,33,34,35,100,101,102,103,200,201,202,203,204,205,206,207}
SPv_ = [k for k in range(208) if k not in FUNC]

for lv in (1, 2):
    for n in (1, 5, NDATA[lv]-1, NDATA[lv]):
        ok, bad, skip = verify(bytes(n), lv)
        print(f"lv{lv} byte len {n}: {ok} ok, {bad} bad, {skip} skip")
