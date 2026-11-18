"""1) Which cluster-1 members deviate from row-even, and where?
2) Test A/B/C features at deep x (period-6 hypothesis) on long cluster-2 renders."""
import sys
sys.path.insert(0, "/tmp/opencode")
from cluster import renders, dicts, groups, find

def feats(y):
    A = ((y//3) % 2) == 1
    B = (((y-1)//3) % 2) == 1
    C = (y % 2) == 1
    return A, B, C

# predicted feature per j=(16-x)%6: j0=notA, j1=C, j2=B, j3=A, j4=notC, j5=notB
def F4(x,y):
    A,B,C = feats(y)
    j = (16-x) % 6
    return [not A, C, B, A, not C, not B][j]

root1 = [mem for mem in groups.values()
         if renders[mem[0]][0] == 'a'][0]
print("=== cluster-1 members vs row-even ===")
for i in root1:
    s, lv, mb = renders[i]
    dev = [(x,y,v) for x,y,v in mb if (y%2==0) != v]
    if dev: print(f"  {s[:24]!r} lv={lv} len(E)={len(mb)} deviations={len(dev)} first={dev[:4]}")

root2 = [mem for mem in groups.values()
         if renders[mem[0]][0] == '!'][0]
print("\n=== cluster-2 members vs A/B/C period-6 law ===")
for i in root2:
    s, lv, mb = renders[i]
    dev = [(x,y,v) for x,y,v in mb if F4(x,y) != v]
    ndeep = sum(1 for x,y,v in mb if x <= 10)
    print(f"  {s[:24]!r} lv={lv} len(E)={len(mb)} dev={len(dev)} deep_x<=10={ndeep}"
          + (f" first_dev={dev[:3]}" if dev else ""))
