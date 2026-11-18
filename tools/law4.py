"""Assign every render to one of 4 laws; for F4 build the 6x6 LUT
indexed by ((16-x)%6, y%6) and verify against ALL F4 triples.
Then report which renders still fit nothing."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH, A
from mqr_probe import render

FUNC = {0,1,32,33,34,35,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]

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

def F1(x,y): return y % 2 == 0
def F2(x,y): return (x*y) % 6 < 3
def F3(x,y): return (x//3 + y//2 + 1) % 2 == 1

LUT = {}
def F4(x,y):
    return LUT[((16-x) % 6, y % 6)]

def derive(s, lv):
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: return None
    if len(m) != 17: return None
    E = expected_stream(s)
    return [(PATH[SPv[i]][0], PATH[SPv[i]][1], m[PATH[SPv[i]][1]][PATH[SPv[i]][0]] ^ E[i])
            for i in range(len(E))]

renders = []
for s, lv in BAT:
    mb = derive(s, lv)
    if mb: renders.append((s, lv, mb))

# pass 1: assign F1/F2/F3; collect F4 candidates (fit nothing else)
f4 = []
counts = {'F1':0,'F2':0,'F3':0,'F4':0,'NONE':0}
members = {'F1':[], 'F2':[], 'F3':[], 'F4':[], 'NONE':[]}
for s, lv, mb in renders:
    if all(F1(x,y)==v for x,y,v in mb): counts['F1']+=1; members['F1'].append(s); continue
    if all(F2(x,y)==v for x,y,v in mb): counts['F2']+=1; members['F2'].append(s); continue
    if all(F3(x,y)==v for x,y,v in mb): counts['F3']+=1; members['F3'].append(s); continue
    f4.append((s,lv,mb)); members['F4'].append(s)

# pass 2: F4 LUT from triples (majority vote per (j,t))
from collections import Counter
votes = {}
for s, lv, mb in f4:
    for x,y,v in mb:
        votes.setdefault(((16-x)%6, y%6), Counter())[v] += 1
for k, c in votes.items():
    LUT[k] = c.most_common(1)[0][0]
conf = {k: c for k,c in votes.items() if len(c) > 1}
print("F4 LUT (j=(16-x)%6 rows, t=y%6 cols):")
for j in range(6):
    print("   ", [LUT[(j,t)] for t in range(6)])
print("vote conflicts:", conf if conf else "NONE")

# pass 3: verify F4 law on all f4 renders
bad = 0
for s, lv, mb in f4:
    if not all(F4(x,y)==v for x,y,v in mb): bad += 1; print("F4 MISMATCH:", s, lv)
print(f"F4 verified: {len(f4)-bad}/{len(f4)}")
for k in members: print(k, counts[k], members[k][:8])
