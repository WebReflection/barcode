"""Brute-force the mask law per family over a huge formula space.

Triple DB: (family_sig, x, y, mask_bit) from derived masks of many renders
(incl. long lv1/lv2 numeric for wide x coverage). Then test generated
formulas: linear-mod-M thresholds and division-sum parities."""
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

fams = {}
for s, lv in BAT:
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: continue
    if len(m) != 17: continue
    E = expected_stream(s)
    mb = [(PATH[SPv[i]][0], PATH[SPv[i]][1], m[PATH[SPv[i]][1]][PATH[SPv[i]][0]] ^ E[i])
          for i in range(len(E))]
    sig = tuple(v for _,_,v in mb[:24])
    fams.setdefault(sig, {'members': [], 'triples': set()})
    fams[sig]['members'].append((s, lv))
    for x,y,v in mb: fams[sig]['triples'].add((x,y,v))

print("families:", [(len(f['members']), f['members'][0][0][:12], len(f['triples']))
                    for f in fams.values()])

def gen_formulas():
    # linear (A*x + B*y + C) mod M  REL T
    for M in (2,3,4,5,6,7,8):
        for a in range(0,9):
            for b in range(0,9):
                if a==0 and b==0: continue
                for c in range(M):
                    for t in range(1,M):
                        yield (f"({a}x+{b}y+{c})%M<{t}".replace('M',str(M)),
                               lambda x,y,a=a,b=b,c=c,M=M,t=t: ((a*x+b*y+c)%M) < t)
    # division sums parity
    for da in range(1,7):
        for db in range(1,7):
            for c in (0,1):
                yield (f"(x//{da}+y//{db}+{c})%2",
                       lambda x,y,da=da,db=db,c=c: ((x//da)+(y//db)+c)%2)
    # products
    for da in range(1,5):
        for db in range(1,5):
            for m in (2,3,4,5,6):
                for t in range(1,m):
                    yield (f"(x//{da}*y//{db})%{m}<{t}",
                           lambda x,y,da=da,db=db,m=m,t=t: ((x//da)*(y//db))%m < t)

forms = list(gen_formulas())
print(f"{len(forms)} candidate formulas\n")
for sig, f in fams.items():
    T = f['triples']
    hit = [n for n, fn in forms if all(fn(x,y)==v for x,y,v in T)]
    ex, lv = f['members'][0]
    print(f"family n={len(f['members'])} ex={ex[:16]!r} lv={lv} triples={len(T)}")
    if hit: print("   LAWS:", hit[:6])
    else: print("   NO FIT; x-range:", sorted(set(x for x,_,_ in T)))
