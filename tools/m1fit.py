"""M1: model-free linear fit.
1. Collect renders of random 5-digit payloads.
2. Same-mask graph via small diffs -> largest component.
3. Fit cells = G * fields + c0 over GF(2) for candidate field models.
   The consistent model wins."""
import sys, random, json
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

CELLS = [(x, y) for y in range(11) for x in range(11)]
FIXED = {(x, y) for x in range(8) for y in range(8)} | {(8,0),(9,0),(10,0),(8,8)}
DATA = [c for c in CELLS if c not in FIXED]
N = len(DATA)

random.seed(1234)
samples = {}
pls = []
for t in range(160):
    p = ''.join(str(random.randrange(10)) for _ in range(5))
    if p in samples: continue
    try:
        m = render(p, level=1, mask=0, version=1)
    except Exception:
        continue
    samples[p] = tuple(m[y][x] for x, y in DATA)
    pls.append(p)
print(f"renders: {len(pls)}")

# same-mask graph
def dsz(a, b): return sum(x != y for x, y in zip(a, b))
parent = {p: p for p in pls}
def find(p):
    while parent[p] != p: parent[p] = parent[parent[p]]; p = parent[p]
    return p
edges = []
for i, a in enumerate(pls):
    for b in pls[i+1:]:
        d = dsz(samples[a], samples[b])
        if d <= 14:
            edges.append((a, b, d))
            parent[find(a)] = find(b)
from collections import defaultdict
comp = defaultdict(list)
for p in pls: comp[find(p)].append(p)
big = max(comp.values(), key=len)
print(f"largest same-mask component: {len(big)} payloads")

# --- field models ---
def f_pairs(s):
    d = [int(c) for c in s]
    v = (d[0]*10+d[1], d[2]*10+d[3], d[4])
    out = []
    for x in v[:2]: out += [(x >> (6-i)) & 1 for i in range(7)]
    out += [(v[2] >> (3-i)) & 1 for i in range(4)]
    return out
def f_pairs_rev(s):
    d = [int(c) for c in s]
    v = (d[1]*10+d[0], d[3]*10+d[2], d[4])
    return f_pairs(''.join(str(x) for x in [v[0]//10, v[0]%10, v[1]//10, v[1]%10, v[2]]))
def f_32(s):
    d = [int(c) for c in s]
    g1, g2 = d[0]*100+d[1]*10+d[2], d[3]*10+d[4]
    return [(g1 >> (9-i)) & 1 for i in range(10)] + [(g2 >> (6-i)) & 1 for i in range(7)]
def f_digits(s):
    out = []
    for c in s: out += [(int(c) >> (3-i)) & 1 for i in range(4)]
    return out
def f_pairs_lsb(s):
    d = [int(c) for c in s]
    v = (d[0]*10+d[1], d[2]*10+d[3], d[4])
    out = []
    for x in v[:2]: out += [(x >> i) & 1 for i in range(7)]
    out += [(v[2] >> i) & 1 for i in range(4)]
    return out

MODELS = {'pairs': f_pairs, 'pairs_rev': f_pairs_rev, '3+2': f_32,
          'digits': f_digits, 'pairs_lsb': f_pairs_lsb}

def fit(model, payloads):
    """Solve cells(p) = G*field(p) + c0 over GF(2) for the component."""
    K = len(model(payloads[0]))
    # unknowns: K*n + n; eqs from pairs (p0, p): cells(p0)^cells(p) = G*(f0^f)
    p0 = payloads[0]; f0 = model(p0); c0v = samples[p0]
    rows = []  # each: (coef list over unknowns G[i][j] (n*K) , rhs n bits) -- build per-cell eq
    # We'll do block elimination: for each cell i, unknowns G[i][:] and c0[i]
    # equation from pair (p0,p) cell i: sum_j G[i][j]*(f0[j]^f[j]) = c0v[i]^samples[p][i]
    eqs = []  # (df, dc) pairs
    used = [p0]
    for p in payloads[1:]:
        f = model(p)
        df = [x ^ y for x, y in zip(f0, f)]
        if not any(df): continue
        dc = [x ^ y for x, y in zip(c0v, samples[p])]
        eqs.append((df, dc))
    # gaussian elim per cell with shared pivots on df
    # unknowns per cell: K (+1 for constant? no: c0 is per-cell constant, but cells(p0) anchors it)
    # solve G[i]: sum_j df[j]*G[i][j] = dc[i] for each eq
    # build matrix once from df vectors
    mrows = []
    for df, dc in eqs: mrows.append((df, dc))
    # eliminate
    piv = {}
    for df, dc in mrows:
        r = df[:]; rhs = list(dc)  # rhs is n-bit vector: each cell's eq has same coefs
        # we need per-cell unknowns; do elimination on (df -> dc-vector) rows
        mrows2 = mrows
        break
    # proper: unknowns x[j][i] = G[i][j]. eq: sum_j df[j]*x[j][i] = dc[i]
    # treat each cell i independently: rows (df, dc[i])
    rank = 0
    Gsol = [[None]*K for _ in range(N)]
    # build combined elimination over all cells simultaneously using df pivots
    rows = [(df, dc) for df, dc in eqs]
    pivots = []
    mat = [list(df) + [None] for df, dc in []]
    # simple: augment each row with its dc as n extra columns
    aug = [list(df) + list(dc) for df, dc in rows]
    ncol = K
    r = 0
    for cidx in range(ncol):
        piv_r = None
        for rr in range(r, len(aug)):
            if aug[rr][cidx]: piv_r = rr; break
        if piv_r is None: continue
        aug[r], aug[piv_r] = aug[piv_r], aug[r]
        for rr in range(len(aug)):
            if rr != r and aug[rr][cidx]:
                aug[rr] = [x ^ y for x, y in zip(aug[rr], aug[r])]
        pivots.append(cidx); r += 1
    # consistency: rows with all-zero coefs must have zero dc
    bad = 0
    for rr in aug:
        if not any(rr[:ncol]) and any(rr[ncol:]): bad += 1
    # extract G for pivot columns; free vars = 0
    Gs = [[0]*K for _ in range(N)]
    for ri, cidx in enumerate(pivots):
        for i in range(N):
            Gs[i][cidx] = aug[ri][ncol + i]
    return Gs, len(pivots), bad, K

for name, mf in MODELS.items():
    Gs, rank, bad, K = fit(mf, big)
    print(f"{name:10}: K={K} rank={rank} inconsistent-rows={bad}")
    if bad == 0 and rank == K:
        json.dump({'model': name, 'G': Gs, 'DATA': DATA, 'component': big},
                  open('/tmp/opencode/m1fit.json', 'w'))
        print("  -> SAVED")
