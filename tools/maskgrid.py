"""Full derived-mask grids for each unknown family's widest render."""
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
        ('X Y Z Y X',3),('ABCDEFG',3),('ABCDEFGH',3),
        ('ABCDEFGHIJK',3),('ABCDEFGHIJKLM',3),('ABCDEFGHIJKLMN',3),
        ('ABCDEFGHIJKLMNO',3),('ABCDEFGHIJKLMNOP',3),('ABCDEFGHIJKLMNOPQ',3),
        ('ABCDEFGHIJKLMNOPQRS',3),('ABCDEFGHIJKLMNOPQRST',3),('ABCDEFGHIJKLMNOPQRSTU',3)]
for n in range(1,21):
    BAT.append((''.join(str((i*7+3)%10) for i in range(n)),3))
for lv in (1,2,3):
    for s in ('abcdefghi','!!!!!!!!','1234567890','ABCDEFGH','$ $ $ $ $','!!!!'):
        BAT.append((s,lv))

sigs = {}
for s, lv in BAT:
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: continue
    if len(m) != 17: continue
    E = expected_stream(s)
    mb = [(PATH[SPv[i]][0], PATH[SPv[i]][1], m[PATH[SPv[i]][1]][PATH[SPv[i]][0]] ^ E[i])
          for i in range(len(E))]
    sig = tuple(v for _,_,v in mb[:24])
    sigs.setdefault(sig, []).append((s, lv, mb))

KNOWN = {'r%2=0', '((x-1)>>1)+(y>>1)+1 odd'}
import importlib
ZOO = importlib.import_module('maskzoo').ZOO

for sig, items in sigs.items():
    fits = [n for n,f in ZOO.items() if all(f(x,y)==v for s,lv,mb in items for x,y,v in mb)]
    if any(k in f for f in fits for k in KNOWN) or fits:
        continue
    widest = max(items, key=lambda t: len(t[2]))
    s, lv, mb = widest
    print(f"=== family n={len(items)} ex={s[:18]!r} widest len(E)={len(mb)} members={[t[0] for t in items][:12]}")
    grid = {}
    for x,y,v in mb: grid[(x,y)] = v
    xs = sorted(set(x for x,y in grid), reverse=True)
    print("      " + "".join(f"x{x:<2}" for x in xs))
    for y in range(17):
        if any((x,y) in grid for x in xs):
            print(f"  y={y:2}: " + " ".join(f"{grid.get((x,y),'?'):<2}" for x in xs))
    print()
