"""Per-render law assignment: fit every render's derived-mask triples against
the formula zoo; cluster renders by law. Renders fitting nothing -> dump
grid for manual inspection."""
import sys
sys.path.insert(0, "/tmp/opencode")
from funcsolve import expected_stream, PATH, A
from mqr_probe import render
from maskfit import gen_formulas  # reuse generator

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

forms = list(gen_formulas())
law_of = {}; unmatched = []
for s, lv in BAT:
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: continue
    if len(m) != 17: continue
    E = expected_stream(s)
    mb = [(PATH[SPv[i]][0], PATH[SPv[i]][1], m[PATH[SPv[i]][1]][PATH[SPv[i]][0]] ^ E[i])
          for i in range(len(E))]
    hit = [n for n, fn in forms if all(fn(x,y)==v for x,y,v in mb)]
    if hit:
        key = hit[0]
        law_of.setdefault(key, []).append((s, lv, len(hit)))
    else:
        unmatched.append((s, lv, mb))

print("=== LAW CLUSTERS ===")
for law in sorted(law_of):
    mem = law_of[law]
    print(f"{law:32} n={len(mem):3} ex={mem[0][0][:14]!r} maxalt={max(h[2] for h in mem)}")

print(f"\n=== UNMATCHED: {len(unmatched)} ===")
for s, lv, mb in unmatched:
    print(f"  {s[:18]!r} lv={lv} len(E)={len(mb)}")
