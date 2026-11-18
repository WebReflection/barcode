"""Wide-grid dump for the largest unmatched renders + cross-member
consistency check per family."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH, A
from mqr_probe import render

FUNC = {0,1,32,33,34,35,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]

TARGETS = [('307418529630741852',1), ('307418529630741852',2), ('aaaaaaaaa',3),
           ('abcdefghi',1), ('aaa',3), ('!!!!',3), ('3',2), ('30',3), ('3074',2), ('!',3)]

grids = {}
for s, lv in TARGETS:
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: continue
    if len(m) != 17: continue
    E = expected_stream(s)
    mb = [(PATH[SPv[i]][0], PATH[SPv[i]][1], m[PATH[SPv[i]][1]][PATH[SPv[i]][0]] ^ E[i])
          for i in range(len(E))]
    grids[(s,lv)] = mb

# consistency: for each pair of grids, cells in common must agree
items = list(grids.items())
for i in range(len(items)):
    for j in range(i+1, len(items)):
        (s1,lv1), g1 = items[i]; (s2,lv2), g2 = items[j]
        d1 = {(x,y):v for x,y,v in g1}; d2 = {(x,y):v for x,y,v in g2}
        common = set(d1) & set(d2)
        bad = [c for c in common if d1[c] != d2[c]]
        if bad:
            print(f"CONFLICT {s1[:12]!r}lv{lv1} vs {s2[:12]!r}lv{lv2}: {len(bad)}/{len(common)} cells, e.g. {bad[:5]}")

# widest grid
s, lv = ('307418529630741852', 1)
mb = grids[(s,lv)]
print(f"\n=== grid {s!r} lv{lv} len(E)={len(mb)} ===")
grid = {(x,y):v for x,y,v in mb}
xs = sorted(set(x for x,y in grid), reverse=True)
print("      " + "".join(f"x{x:<3}" for x in xs))
for y in range(17):
    if any((x,y) in grid for x in xs):
        print(f"  y={y:2}: " + " ".join(f"{grid.get((x,y),'?'):<3}" for x in xs))
