"""M1: per-mask-class values at the non-stream cells (format word + fixed)."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

fit = json.load(open('/tmp/opencode/m1fit.json'))
G = fit['G']; DATA = [tuple(c) for c in fit['DATA']]; N = len(DATA)
anchor = fit['component'][0]
_memo = {}
def vec(s):
    if s not in _memo:
        _memo[s] = tuple(render(s, level=1, mask=0, version=1)[y][x] for x, y in DATA)
    return _memo[s]
def f_32(s):
    d = [int(c) for c in s]
    g1, g2 = d[0]*100+d[1]*10+d[2], d[3]*10+d[4]
    return [(g1 >> (9-i)) & 1 for i in range(10)] + [(g2 >> (6-i)) & 1 for i in range(7)]
def predict(s):
    f0, c0 = f_32(anchor), vec(anchor)
    f = f_32(s); out = list(c0)
    for j in range(17):
        if f[j] != f0[j]:
            for i in range(N): out[i] ^= G[i][j]
    return out

random.seed(9)
classes = {}
for t in range(150):
    s = ''.join(str(random.randrange(10)) for _ in range(5))
    try: v = vec(s)
    except Exception: continue
    mm = tuple(a ^ b for a, b in zip(v, predict(s)))
    classes.setdefault(mm, []).append(s)
maps = sorted(classes, key=lambda mm: -len(classes[mm]))

# zero-row cells (non-stream within class): G row all zero
zerorows = [i for i in range(N) if not any(G[i])]
print(f"zero-row cells: {len(zerorows)}")
# per class, observed values there (constant within class)
vals = {}
for mi, mm in enumerate(maps):
    s = classes[mm][0]
    v = vec(s)
    vals[mi] = [v[i] for i in zerorows]

fixed_ix, format_ix = [], []
for k in range(len(zerorows)):
    col = [vals[mi][k] for mi in range(len(maps))]
    (fixed_ix if len(set(col)) == 1 else format_ix).append(k)
print(f"truly fixed: {len(fixed_ix)}, mask-dependent (format): {len(format_ix)}")
print("\nfixed cells:", [(DATA[zerorows[k]], vals[0][k]) for k in fixed_ix])
print("\nformat cells (cell, per-class bit m0..m3):")
fmt = {}
for k in format_ix:
    c = DATA[zerorows[k]]
    bits = [vals[mi][k] for mi in range(len(maps))]
    fmt[c] = bits
    print(f"  {c}: {bits}")
json.dump({'fixed': {f"{x},{y}": vals[0][zerorows[k]] for k in fixed_ix
                     for (x, y) in [DATA[zerorows[k]]]},
           'format': {f"{x},{y}": bits for (x, y), bits in fmt.items()},
           'class_of_map_index_order': 'sorted by frequency'},
          open('/tmp/opencode/m1fmt.json', 'w'), indent=1)
print("\nsaved /tmp/opencode/m1fmt.json")
