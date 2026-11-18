"""Derive the true mask bit at every data cell for each render (mapping now
known: stream idx i <-> SPv[i]), cluster renders by mask signature, and
auto-fit each cluster against a zoo of candidate laws."""
import sys, random
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH, A
from mqr_probe import render

FUNC = {0,1,32,33,34,35,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]

ZOO = {
 'r%2=0':        lambda x,y: y%2==0,
 'r%2=1':        lambda x,y: y%2==1,
 'y&1':          lambda x,y: y&1==1,
 '(y>>1)&1':     lambda x,y: (y>>1)&1==1,
 '1-(y>>1)&1':   lambda x,y: (y>>1)&1==0,
 'y%3=0':        lambda x,y: y%3==0,
 'x%3=0':        lambda x,y: x%3==0,
 '(x+y)%2=0':    lambda x,y: (x+y)%2==0,
 '(x+y)%2=1':    lambda x,y: (x+y)%2==1,
 '(x+y)%3=0':    lambda x,y: (x+y)%3==0,
 '(xy)%2+(xy)%3=0': lambda x,y: (x*y)%2+(x*y)%3==0,
 '((x-1)>>1)+(y>>1)+1 odd': lambda x,y: (((x-1)>>1)+(y>>1)+1)&1==1,
 '((x-1)>>1)+(y>>1) odd':   lambda x,y: (((x-1)>>1)+(y>>1))&1==1,
 '(x>>1)+(y>>1) odd':       lambda x,y: ((x>>1)+(y>>1))&1==1,
 '(x>>1)+(y>>1)+1 odd':     lambda x,y: ((x>>1)+(y>>1)+1)&1==1,
 '(x+y)>>1 odd':  lambda x,y: ((x+y)>>1)&1==1,
 '(x+y)>>1 even': lambda x,y: ((x+y)>>1)&1==0,
 'x>>1 odd':      lambda x,y: (x>>1)&1==1,
 'x-1>>1 odd':    lambda x,y: ((x-1)>>1)&1==1,
 '(x-1+y)>>1 odd':lambda x,y: (((x-1)+y)>>1)&1==1,
}

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
# level sweep: does the law depend on level?
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

print(f"{len(sigs)} distinct mask signatures across {len(BAT)} renders\n")
for sig, items in sigs.items():
    # fit zoo
    fits = []
    for name, f in ZOO.items():
        if all(f(x,y)==v for s,lv,mb in items for x,y,v in mb):
            fits.append(name)
    ex = items[0][0]
    print(f"sig#{sig[:8]}... n={len(items)} ex={ex[:16]!r} lv={items[0][1]}")
    print(f"   fits: {fits if fits else 'NONE'}")
    if not fits:
        # show pattern by y for widest payload
        widest = max(items, key=lambda t: len(t[2]))
        rows = {}
        for x,y,v in widest[2]:
            rows.setdefault(y, {})[x] = v
        for y in sorted(rows):
            print(f"    y={y:2}: " + " ".join(f"x{x}:{rows[y][x]}" for x in sorted(rows[y], reverse=True)))
