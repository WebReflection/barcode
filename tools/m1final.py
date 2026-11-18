"""M1 FINAL validation: 5 mask classes, all lengths, 500 payloads."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

fit = json.load(open('/tmp/opencode/m1fit.json'))
DATA = [tuple(c) for c in fit['DATA']]; N = len(DATA)
tab = json.load(open('/tmp/opencode/m1table.json'))
m5 = json.load(open('/tmp/opencode/m1maps5.json'))
MAPS = [tuple(m) for m in m5['maps']]

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
    return [(d[0] >> (3-i)) & 1 for i in range(4)]

# per-length K table: classes 0-3 saved; class 4 = K1 ^ map4 (map1 = all-zeros)
KT = {}
for L in range(1, 6):
    t = tab[str(L)]
    Ks = {mi: t['K'][str(mi)] for mi in range(4)}
    Ks[4] = [t['K']['1'][i] ^ MAPS[4][i] for i in range(N)]
    KT[L] = {'G': t['G'], 'K': Ks}

tmpl = render('11111', level=1, mask=0, version=1)
random.seed(777)
pls = []
for t in range(600):
    L = random.randrange(1, 6)
    p = ''.join(str(random.randrange(10)) for _ in range(L))
    if p not in pls: pls.append(p)

ok = miss = 0
maskfreq = {}
stat = {}
golden = []
for p in pls:
    L = len(p); G = KT[L]['G']; f = model_f(L, p)
    z = render(p, level=1, mask=0, version=1)
    hit = None
    for mi in range(5):
        K = KT[L]['K'][mi]
        e = [sum(G[i][j] for j in range(len(f)) if f[j]) % 2 ^ K[i] for i in range(N)]
        if all(e[i] == z[DATA[i][1]][DATA[i][0]] for i in range(N)): hit = mi; break
    stat.setdefault(L, [0, 0])
    if hit is None: stat[L][0] += 1; miss += 1
    else:
        stat[L][1] += 1; ok += 1
        maskfreq[hit] = maskfreq.get(hit, 0) + 1
        golden.append({'payload': p, 'mask': hit})
print(f"M1 FINAL: {ok} ok, {miss} miss of {len(pls)}")
for L in sorted(stat): print(f"  len {L}: {stat[L][1]} ok / {stat[L][0]} miss")
print("mask freq:", dict(sorted(maskfreq.items())))
json.dump(golden, open('/tmp/opencode/m1golden.json', 'w'))
print("saved m1golden.json")
