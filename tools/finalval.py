"""FINAL validation: corrected FUNC (row 0 cols 9-16 + corners), 4 mask
laws, per-render law selection, round-trip over the full battery."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH, A
from mqr_probe import render

FUNC = {0,1,32,33,34,35,100,101,102,103,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]

def A_(y): return ((y//3) % 2) == 1
def B_(y): return (((y-1)//3) % 2) == 1
def C_(y): return (y % 2) == 1
LAW = {
 'row-even':  lambda x,y: y % 2 == 0,
 'xy%6<3':    lambda x,y: (x*y) % 6 < 3,
 'x//3+y//2': lambda x,y: (x//3 + y//2 + 1) % 2 == 1,
 'ABC-lut':   lambda x,y: [not A_(y), C_(y), B_(y), A_(y), not C_(y), not B_(y)][(16-x) % 6],
}

BAT = []
for n in range(1,10):
    BAT.append(('!'*n,3)); BAT.append(('a'*n,3))
BAT += [('~~[[<>',3),('abcdefg',3),('abcdefgh',3),('abcdefghi',3),
        ('xyzxyzxy',3),('~~~!!~~!',3),('[[]][][]',3),('~~~~~~',3),
        ('<<<<<<',3),('ZZZZZ',3),('$ $ $ $ $',3),('A1B2C3D4E',3),
        ('X Y Z Y X',3),('ABCDEFG',3),('ABCDEFGH',3),('ABCDEFGHIJK',3),
        ('ABCDEFGHIJKLM',3),('ABCDEFGHIJKLMN',3),('ABCDEFGHIJKLMNO',3),
        ('ABCDEFGHIJKLMNOP',3),('ABCDEFGHIJKLMNOPQ',3),('ABCDEFGHIJKLMNOPQRS',3),
        ('ABCDEFGHIJKLMNOPQRST',3),('ABCDEFGHIJKLMNOPQRSTU',3),
        ('0123456789ABCDEFG',3),('HELLO WORLD',3),('HELLO, WORLD!',3)]
for n in range(1,36):
    BAT.append((''.join(str((i*7+3)%10) for i in range(n)), 1 + n % 3))
for lv in (1,2,3):
    for s in ('abcdefghi','!!!!!!!!','1234567890','ABCDEFGH','$ $ $ $ $','!!!!',
              'aaaaaaaaa','12345678901234567890123456789012345'):
        BAT.append((s,lv))

res = {}; fails = []
for s, lv in BAT:
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: continue
    if len(m) != 17: continue
    E = expected_stream(s)
    oklaw = None
    for name, f in LAW.items():
        t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv[:len(E)]]]
        if t == E: oklaw = name; break
    kind = 'num' if s.isdigit() else ('alnum' if all(c in A for c in s) else 'byte')
    res.setdefault(kind, [0,0]); res[kind][1] += 1
    if oklaw: res[kind][0] += 1
    else: fails.append((s, lv, len(E)))
print("results:", {k: f"{v[0]}/{v[1]}" for k,v in res.items()})
print(f"failures: {len(fails)}")
for s, lv, L in fails[:25]: print(f"   {s[:20]!r} lv={lv} len(E)={L}")
