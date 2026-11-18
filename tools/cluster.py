"""Cluster ALL renders by mask identity via pairwise agreement
(union-find on zero-conflict overlaps >= 8 cells), then derive a 6x6 LUT
per cluster and verify."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH
from mqr_probe import render
from law4 import derive  # reuse (module runs its own analysis too, fine)

# re-derive fresh to be safe
BAT = []
for n in range(1,10):
    BAT.append(('!'*n,3)); BAT.append(('a'*n,3))
BAT += [('~~[[<>',3),('abcdefg',3),('abcdefgh',3),('abcdefghi',3),
        ('xyzxyzxy',3),('~~~!!~~!',3),('[[]][][]',3),('~~~~~~',3),
        ('<<<<<<',3),('ZZZZZ',3),('$ $ $ $ $',3),('A1B2C3D4E',3),
        ('X Y Z Y X',3),('ABCDEFG',3),('ABCDEFGH',3),('ABCDEFGHIJK',3),
        ('ABCDEFGHIJKLM',3),('ABCDEFGHIJKLMN',3),('ABCDEFGHIJKLMNO',3),
        ('ABCDEFGHIJKLMNOP',3),('ABCDEFGHIJKLMNOPQ',3),('ABCDEFGHIJKLMNOPQRS',3),
        ('ABCDEFGHIJKLMNOPQRST',3),('ABCDEFGHIJKLMNOPQRSTU',3)]
for n in range(1,36):
    BAT.append((''.join(str((i*7+3)%10) for i in range(n)), 1 + n % 3))
for lv in (1,2,3):
    for s in ('abcdefghi','!!!!!!!!','1234567890','ABCDEFGH','$ $ $ $ $','!!!!',
              'aaaaaaaaa','12345678901234567890123456789012345'):
        BAT.append((s,lv))

renders = []
for s, lv in BAT:
    mb = derive(s, lv)
    if mb: renders.append((s, lv, mb))
print(f"{len(renders)} renders")

n = len(renders)
parent = list(range(n))
def find(i):
    while parent[i] != i: parent[i] = parent[parent[i]]; i = parent[i]
    return i
def union(i,j): parent[find(i)] = find(j)

dicts = [{(x,y):v for x,y,v in mb} for _,_,mb in renders]
for i in range(n):
    for j in range(i+1, n):
        common = set(dicts[i]) & set(dicts[j])
        if len(common) < 8: continue
        if all(dicts[i][c] == dicts[j][c] for c in common):
            union(i,j)

groups = {}
for i in range(n): groups.setdefault(find(i), []).append(i)
print(f"{len(groups)} clusters\n")
for root, mem in sorted(groups.items(), key=lambda kv: -len(kv[1])):
    s, lv, mb = renders[mem[0]]
    # merged triples + LUT fit
    T = set()
    for i in mem: T |= set(renders[i][2])
    xs = sorted(set(x for x,_,_ in T), reverse=True)
    print(f"cluster n={len(mem)} ex={s[:16]!r} lv={lv} cells={len(T)} xdepth={min(xs)}")
    print("   members:", [renders[i][0][:12] for i in mem][:10])
    if len(mem) == 1 and len(T) < 15: continue
    # try simple laws first
    tests = {
      'y%2=0': lambda x,y: y%2==0,
      'y%2=1': lambda x,y: y%2==1,
      '(x*y)%6<3': lambda x,y: (x*y)%6<3,
      '(x*y)%6>=3': lambda x,y: (x*y)%6>=3,
      '(x//3+y//2+1)%2': lambda x,y: (x//3+y//2+1)%2==1,
      '(x//2+y//2+1)%2': lambda x,y: (x//2+y//2+1)%2==1,
      '(x//2+y//2)%2': lambda x,y: (x//2+y//2)%2==1,
      '(x//2+y//3+1)%2': lambda x,y: (x//2+y//3+1)%2==1,
      '(x//3+y//3+1)%2': lambda x,y: (x//3+y//3+1)%2==1,
      '(x//2+y//1+1)%2': lambda x,y: (x//2+y+1)%2==1,
      '(x//1+y//2+1)%2': lambda x,y: (x+y//2+1)%2==1,
      '(x//1+y//3+1)%2': lambda x,y: (x+y//3+1)%2==1,
    }
    hits = [k for k,f in tests.items() if all(f(x,y)==v for x,y,v in T)]
    print("   laws:", hits if hits else "NONE")
