"""Generalized Micro QR frame extraction: any version/level/mode/length.

Per (version, level, mode, L): saturated varying scan -> single-field-bit
probes (majority vote, adaptive retry) -> K-class collection -> validation.

Usage: mqr_modes.py VERSION LEVEL MODE L0 L1 OUT.json
  MODE in {num, byte, alnum}; lengths L0..L1 inclusive (1-based).
"""
import sys, json, random, re
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

VERSION, LEVEL, MODE = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
L0, L1 = int(sys.argv[4]), int(sys.argv[5])
OUT = sys.argv[6]
SIZE = VERSION * 2 + 9
random.seed(11)

ALNUM = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:"

def fields_of(L):
    if MODE == 'byte':
        return [(1, 8)] * L
    if MODE == 'alnum':
        F = [(2, 11)] * (L // 2)
        if L % 2: F.append((1, 6))
        return F
    F, rem = [], L
    while rem >= 3:
        F.append((3, 10)); rem -= 3
    if rem == 2: F.append((2, 7))
    elif rem == 1: F.append((1, 4))
    return F

def fcap(n):
    """Max value + 1 for a field of n chars."""
    if MODE == 'byte': return 256
    if MODE == 'alnum': return 45 * 45 if n == 2 else 45
    return 10 ** n

def payload_from(vals, L):
    if MODE == 'byte':
        return bytes(vals)
    if MODE == 'alnum':
        s = ''
        for fi, (n, w) in enumerate(fields_of(L)):
            v = vals[fi]
            if n == 2: s += ALNUM[v // 45] + ALNUM[v % 45]
            else: s += ALNUM[v]
        return s
    s = ''
    for fi, (n, w) in enumerate(fields_of(L)):
        v = vals[fi]; ds = []
        for _ in range(n): ds.append(str(v % 10)); v //= 10
        s += ''.join(reversed(ds))
    return s

def model_f(vals, L):
    out = []
    for fi, (n, w) in enumerate(fields_of(L)):
        out += [(vals[fi] >> e) & 1 for e in range(w)]
    return out

def rand_vals(L):
    while True:
        vals = [random.randrange(fcap(n)) for n, w in fields_of(L)]
        p = payload_from(vals, L)
        # zint auto-mode: all-digit alnum payloads go numeric; 3+ digit
        # runs get numeric-segmented. Reject both for alnum frames.
        if MODE == 'alnum':
            if all(c in '0123456789' for c in p):
                continue  # would go numeric
            if re.search(r'\d{3}', p):
                continue  # 3+ digit run -> numeric-segmented
            return vals
        if MODE == 'byte':
            ALNUM_SET = set(b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:")
            if all(c in ALNUM_SET for c in p):
                continue  # would go numeric/alnum
            return vals
        return vals

_cache = {}
def vec(p):
    if p not in _cache:
        m = render(p, level=LEVEL, version=VERSION)
        _cache[p] = {(x, y): m[y][x] for y in range(SIZE) for x in range(SIZE)}
    return _cache[p]

ALL = [(x, y) for y in range(SIZE) for x in range(SIZE)]
res = {}
for L in range(L0, L1 + 1):
    ref = vec(payload_from(rand_vals(L), L))
    varying = set()
    for t in range(600):
        m = vec(payload_from(rand_vals(L), L))
        for c, v in m.items():
            if v != ref[c]: varying.add(c)
    DATA = [c for c in ALL if c in varying]; DSET = set(DATA); N = len(DATA)
    def T(p): return tuple(vec(p)[c] for c in DATA)
    F = fields_of(L); K = sum(w for _, w in F)
    def probe(fi, e, ntries=220, thr=52):
        pv = 1 << e; votes = {}
        for t in range(ntries):
            v = random.randrange(0, fcap(F[fi][0])); v &= ~((1 << (e + 1)) - 1)
            wv = v + pv
            if wv >= fcap(F[fi][0]): continue
            dA = rand_vals(L); dB = list(dA)
            dA[fi], dB[fi] = v, wv
            va, vb = T(payload_from(dA, L)), T(payload_from(dB, L))
            if sum(x != y for x, y in zip(va, vb)) <= thr:
                key = tuple(x ^ y for x, y in zip(va, vb))
                votes[key] = votes.get(key, 0) + 1
        if not votes: return None, 0
        return max(votes.items(), key=lambda kv: kv[1])
    G = {}
    weak = 0
    for fi in range(len(F)):
        for e in range(F[fi][1]):
            c, votes = probe(fi, e)
            if not (c and votes >= 6):
                c, votes = probe(fi, e, ntries=400, thr=95)
            if c and votes >= 6:
                G[(fi, e)] = [1 if c[i] else 0 for i in range(N)]
            else:
                weak += 1
                print(f"  v{VERSION} L{LEVEL} {MODE} len {L} WARNING weak col f{fi}b{e}", flush=True)
                G[(fi, e)] = [0] * N
    def gmat():
        cols = [G[(fi, e)] for fi in range(len(F)) for e in range(F[fi][1])]
        return [[cols[j][i] for j in range(K)] for i in range(N)]
    GM = gmat()
    def obs(p): return [vec(p)[c] for c in DATA]
    Ks = {}
    for t in range(600):
        vals = rand_vals(L)
        f = model_f(vals, L)
        o = obs(payload_from(vals, L))
        Kv = tuple(o[i] ^ sum(GM[i][j] for j in range(K) if f[j]) % 2 for i in range(N))
        Ks[Kv] = Ks.get(Kv, 0) + 1
    ok = miss = 0
    for t in range(90):
        vals = rand_vals(L)
        f = model_f(vals, L)
        z = obs(payload_from(vals, L))
        hit = any([sum(GM[i][j] for j in range(K) if f[j]) % 2 ^ Kv[i] for i in range(N)] == z
                  for Kv in Ks)
        if hit: ok += 1
        else: miss += 1
    print(f"v{VERSION} L{LEVEL} {MODE} len {L}: {N} cells, {K-weak}/{K} cols strong, "
          f"{len(Ks)} classes, valid {ok}/{ok+miss}", flush=True)
    res[L] = {'DATA': [list(c) for c in DATA], 'K': K, 'fields': F,
              'G': {f"{fi},{e}": G[(fi, e)] for (fi, e) in G},
              'Ks': [list(k) for k in Ks]}

json.dump(res, open(OUT, 'w'))
print(f"saved {OUT}")
