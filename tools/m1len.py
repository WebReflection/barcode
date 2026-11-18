"""M1 per-length field-layout solver: cluster same-mask renders, fit candidate
field models over GF(2), report rank/bad-rows + structure + count bits."""
import sys, json, random, itertools
from collections import defaultdict
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]
N = len(DATA)

random.seed(2026)
_rend = {}
def vec(s):
    if s not in _rend:
        _rend[s] = tuple(render(s, level=1, mask=0, version=1)[y][x] for x, y in DATA)
    return _rend[s]

def component(payloads, thr=10):
    parent = {p: p for p in payloads}
    def find(p):
        while parent[p] != p: parent[p] = parent[parent[p]]; p = parent[p]
        return p
    for a, b in itertools.combinations(payloads, 2):
        if sum(x != y for x, y in zip(vec(a), vec(b))) <= thr:
            parent[find(a)] = find(b)
    comp = defaultdict(list)
    for p in payloads: comp[find(p)].append(p)
    return max(comp.values(), key=len)

def fit(model, comp):
    """cells = G*f + c0 over GF(2); returns Gs, rank, bad, K."""
    K = len(model(digits_of(comp[0])))
    p0 = comp[0]; f0 = model(digits_of(p0)); c0v = vec(p0)
    eqs = []
    for p in comp[1:]:
        f = model(digits_of(p))
        df = [x ^ y for x, y in zip(f0, f)]
        if not any(df): continue
        dc = [x ^ y for x, y in zip(c0v, vec(p))]
        eqs.append((df, dc))
    aug = [list(df) + list(dc) for df, dc in eqs]
    r = 0; pivots = []
    for cidx in range(K):
        pr = next((rr for rr in range(r, len(aug)) if aug[rr][cidx]), None)
        if pr is None: continue
        aug[r], aug[pr] = aug[pr], aug[r]
        for rr in range(len(aug)):
            if rr != r and aug[rr][cidx]:
                aug[rr] = [x ^ y for x, y in zip(aug[rr], aug[r])]
        pivots.append(cidx); r += 1
    bad = sum(1 for rr in aug if not any(rr[:K]) and any(rr[K:]))
    Gs = [[0]*K for _ in range(N)]
    for ri, cidx in enumerate(pivots):
        for i in range(N): Gs[i][cidx] = aug[r and ri][K + i] if False else aug[ri][K + i]
    return Gs, len(pivots), bad, K

def digits_of(s): return [int(c) for c in s]

# ---- field models per length ----
def M(f):
    return f

MODELS = {
 4: {
  '3+1': M(lambda d: [( (d[0]*100+d[1]*10+d[2]) >> (9-i)) & 1 for i in range(10)]
                     + [(d[3] >> (3-i)) & 1 for i in range(4)]),
  '2+2': M(lambda d: [( (d[0]*10+d[1]) >> (6-i)) & 1 for i in range(7)]
                     + [( (d[2]*10+d[3]) >> (6-i)) & 1 for i in range(7)]),
  '4dig': M(lambda d: [(d[i] >> (3-i//1)) & 1 for i in [0]] + [] ),  # placeholder
 },
}

def m_3groups(d):
    out = []
    for c in d: out += [(int(c) >> (3-i)) & 1 for i in range(4)]
    return out

MODELS[4]['digits4'] = M(lambda d: sum([[(d[i] >> (3-j)) & 1 for j in range(4)] for i in range(4)], []))
MODELS[3] = {
  '3': M(lambda d: [((d[0]*100+d[1]*10+d[2]) >> (9-i)) & 1 for i in range(10)]),
  '2+1': M(lambda d: [((d[0]*10+d[1]) >> (6-i)) & 1 for i in range(7)] + [(d[2] >> (3-i)) & 1 for i in range(4)]),
  'digits3': M(lambda d: sum([[(d[i] >> (3-j)) & 1 for j in range(4)] for i in range(3)], [])),
}
MODELS[2] = {
  '2': M(lambda d: [((d[0]*10+d[1]) >> (6-i)) & 1 for i in range(7)]),
  'digits2': M(lambda d: sum([[(d[i] >> (3-j)) & 1 for j in range(4)] for i in range(2)], [])),
}
MODELS[1] = {
  '1': M(lambda d: [(d[0] >> (3-i)) & 1 for i in range(4)]),
}

CC = [(10,10),(9,10),(10,9)]
CCI = [DATA.index(c) for c in CC]

results = {}
for L in (4, 3, 2, 1):
    pls = []
    for t in range(110):
        p = ''.join(str(random.randrange(10)) for _ in range(L))
        if p not in pls: pls.append(p)
    pls = [p for p in pls if len(p) == L]
    try:
        vec(pls[0])
    except Exception as e:
        print(f"len {L}: render fail {e}"); continue
    comp = component(pls)
    print(f"\n=== len {L}: {len(pls)} renders, largest component {len(comp)} ===")
    best = None
    for name, mf in MODELS[L].items():
        Gs, rank, bad, K = fit(mf, comp)
        print(f"  {name:8}: K={K} rank={rank} bad={bad}")
        if bad == 0 and (best is None or rank > best[2]):
            best = (name, Gs, rank, K)
    if best:
        name, Gs, rank, K = best
        # count bits from unmasked prediction of any member
        p0 = comp[0]; f0 = __import__('operator')  # noqa
        # unmasked cells: predicted = c0 ^ G*(f^f0); mask map = obs^pred (const in class)
        # count = pred[CCI] (pred is the UNMASKED stream+const cells? no: pred reproduces
        # the MASKED cells of members. Unmasked = pred ^ maskmap. Get maskmap first:
        # maskmap = obs ^ (c0 ^ G*(f^f0)) for each member -> constant per class.
        mm = None
        for p in comp[:5]:
            f = MODELS[L][name](digits_of(p))
            pred = list(vec(comp[0]))
            df = [x ^ y for x, y in zip(MODELS[L][name](digits_of(comp[0])), f)]
            for j in range(K):
                if df[j]:
                    for i in range(N): pred[i] ^= Gs[i][j]
            m = tuple(a ^ b for a, b in zip(vec(p), pred))
            if mm is None: mm = m
            else: assert mm == m, "mask map not constant?!"
        cnt = None  # count read separately (see m1count check)
        # varying cells in path order
        varcells = [DATA[i] for i in range(N) if any(Gs[i])]
        print(f"  WINNER {name}: count bits {cnt}, varying cells {len(varcells)}")
        results[L] = {'model': name, 'G': Gs, 'count': cnt,
                      'maskmap': list(mm),
                      'varcells': [list(c) for c in varcells]}
json.dump(results, open('/tmp/opencode/m1len.json', 'w'))
print("\nsaved /tmp/opencode/m1len.json")
