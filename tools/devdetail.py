"""Where exactly do long-numeric renders deviate from row-even?
Print deviation stream indices + cells for each long numeric, plus derived
mask values around the first deviation."""
import sys
sys.path.insert(0, "/tmp/opencode")
from cluster import renders, dicts, groups

root1 = [mem for mem in groups.values() if renders[mem[0]][0] == 'a'][0]
rows = []
for i in root1:
    s, lv, mb = renders[i]
    if len(mb) < 90: continue
    dev = [(j, x, y, v) for j,(x,y,v) in enumerate(mb) if (y%2==0) != v]
    rows.append((len(mb), s, lv, dev))
rows.sort()
for L, s, lv, dev in rows:
    print(f"len(E)={L} lv={lv} ndev={len(dev)}")
    print("   dev idx:", [d[0] for d in dev])
    print("   dev cells:", [(d[1],d[2]) for d in dev])
    # derived mask values around first deviation
    if not dev: continue
    j0 = dev[0][0]
    s_, lv_, mb = renders[[i for i in root1 if renders[i][0]==s and renders[i][1]==lv][0]]
    print("   stream", j0-3, "..", min(j0+8, L-1), ":",
          [(j, mb[j][0], mb[j][1], mb[j][2]) for j in range(max(0,j0-3), min(j0+8, L))])
