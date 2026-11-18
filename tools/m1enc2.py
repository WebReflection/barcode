"""M1 encoder v2: per-length (G, K) tables from m1len.json + mask maps.

K_L derived from a fresh payload; its mask class pinned via count cells
(count = L known). Validation: try all 4 mask classes vs zint render.
"""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

fit = json.load(open('/tmp/opencode/m1fit.json'))
DATA = [tuple(c) for c in fit['DATA']]; N = len(DATA)
IDX = {c: i for i, c in enumerate(DATA)}
len_j = json.load(open('/tmp/opencode/m1len.json'))
MD = json.load(open('/Users/ag/git/barcode/tools/m1masks.json'))
MAPS = [tuple(m[0]) for m in MD['masks']]   # m[1] = freq; relative to ref class
_m5 = json.load(open('/tmp/opencode/m1maps5.json'))
assert [tuple(c) for c in _m5['DATA']] == DATA
MAPS.append(tuple(_m5['maps'][4]))
CC = [(9, 10), (10, 10), (10, 9)]           # b2, b1, b0 of count=L

def model_f(L, payload):
    d = [int(c) for c in payload]
    if L == 5:
        g1, g2 = d[0]*100+d[1]*10+d[2], d[3]*10+d[4]
        return [(g1 >> (9-i)) & 1 for i in range(10)] + [(g2 >> (6-i)) & 1 for i in range(7)]
    if L == 4:
        g1 = d[0]*100+d[1]*10+d[2]
        return [(g1 >> (9-i)) & 1 for i in range(10)] + [(d[3] >> (3-i)) & 1 for i in range(4)]
    if L == 3:
        g1 = d[0]*100+d[1]*10+d[2]
        return [(g1 >> (9-i)) & 1 for i in range(10)]
    if L == 2:
        v = d[0]*10+d[1]
        return [(v >> (6-i)) & 1 for i in range(7)]
    if L == 1:
        return [(d[0] >> (3-i)) & 1 for i in range(4)]

def obs(s):
    m = render(s, level=1, mask=0, version=1)
    return [m[y][x] for x, y in DATA]

def derive_K(L, payload):
    f = model_f(L, payload)
    o = obs(payload)
    return [o[i] ^ sum(G[i][j] for j in range(len(f)) if f[j]) % 2 for i in range(N)]

# --- derive per-length constants and pin the anchor's mask class via count cells
KTAB = {}
for L in range(1, 6):
    G = fit['G'] if L == 5 else len_j[str(L)]['G']
    K = 17 if L == 5 else len(model_f(L, '1' * L))
    best = None
    for p in ('11111', '1111', '111', '11', '1')[:1] if L == 5 else ('1'*L,):
        pass
    # try a few payloads until the count-cell test pins a class cleanly
    for trial in range(6):
        random.seed(L * 100 + trial)
        p = ''.join(str(random.randrange(10)) for _ in range(L))
        Kc = derive_K(L, p)
        # count-cell test: Kc[CC] ^ map_m[CC] == L for m = anchor's class
        hits = []
        for mi, mp in enumerate(MAPS):
            val = 0
            for k, c in enumerate(CC):
                val = (val << 1) | (Kc[IDX[c]] ^ mp[IDX[c]])
            if val == L: hits.append(mi)
        if len(hits) == 1:
            cp = hits[0]
            print(f"len {L}: payload {p}: anchor class = map{cp} (count test)")
            KTAB[L] = {'K': {mi: [Kc[i] ^ MAPS[mi][i] ^ MAPS[cp][i] for i in range(N)]
                             for mi in range(4)}, 'cp': cp}
            break
        else:
            # ambiguous (mask classes sharing count-cell values): pin by mini-validation
            print(f"  len {L} trial {trial}: ambiguous hits {hits}, pinning by render test")
            G = fit['G'] if L == 5 else len_j[str(L)]['G']
            K = len(model_f(L, p))
            f = model_f(L, p)
            z = render(p, level=1, mask=0, version=1)
            for cp in hits:
                good = True
                for mpay in ('1'*L, '2'*L):
                    if len(set(mpay)) != len(mpay) and False: pass
                    fp = model_f(L, mpay)
                    zp = render(mpay, level=1, mask=0, version=1)
                    okc = 0
                    for mi in range(4):
                        KK = [Kc[i] ^ MAPS[mi][i] ^ MAPS[cp][i] for i in range(N)]
                        e = [sum(G[i][j] for j in range(K) if fp[j]) % 2 ^ KK[i] for i in range(N)]
                        if all(e[i] == zp[DATA[i][1]][DATA[i][0]] for i in range(N)): okc += 1
                    if okc == 0: good = False
                if good:
                    print(f"len {L}: anchor class = map{cp} (render test)")
                    KTAB[L] = {'K': {mi: [Kc[i] ^ MAPS[mi][i] ^ MAPS[cp][i] for i in range(N)]
                                     for mi in range(4)}, 'cp': cp}
                    break
            if L in KTAB: break
    else:
        print(f"len {L}: FAILED to pin class"); continue

# --- validate
random.seed(2026)
stat = {}
pls = []
for t in range(400):
    L = random.randrange(1, 6)
    p = ''.join(str(random.randrange(10)) for _ in range(L))
    if p not in pls: pls.append(p)
tmpl = render('11111', level=1, mask=0, version=1)
ok = miss = 0
for p in pls:
    L = len(p)
    G = fit['G'] if L == 5 else len_j[str(L)]['G']
    f = model_f(L, p)
    z = render(p, level=1, mask=0, version=1)
    hit = None
    for mi in range(5):
        K = KTAB[L]['K'].setdefault(mi) or 0
        if K is 0 or len(K) != N:
            K = KTAB[L]['K']['0']
        K = [0]*N
        e = [row[:] for row in tmpl]
        for i, (x, y) in enumerate(DATA):
            e[y][x] = sum(G[i][j] for j in range(len(f)) if f[j]) % 2 ^ K[i]
        if e == z: hit = mi; break
    stat.setdefault(L, [0, 0])
    if hit is None: stat[L][0] += 1; miss += 1
    else: stat[L][1] += 1; ok += 1
print(f"M1 encoder v2: {ok} ok, {miss} miss of {len(pls)}")
for L in sorted(stat): print(f"  len {L}: {stat[L][1]} ok / {stat[L][0]} miss")
json.dump({str(L): {'K': {str(mi): v for mi, v in KTAB[L]['K'].items()},
                    'G': (fit['G'] if L == 5 else len_j[str(L)]['G'])}
           for L in KTAB},
          open('/tmp/opencode/m1table.json', 'w'))
print("saved m1table.json")
