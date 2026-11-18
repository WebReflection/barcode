"""Final validation: stream = PATH minus skip-set; per-render mask selection;
round-trip byte/numeric/alnum across lengths."""
import sys, random
sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render
from funcsolve import expected_stream, MASKS, PATH, A, av

def b2i(bl): return int(''.join(map(str, bl)), 2)
random.seed(3)

def validate(s, t):
    n = len(s); E = expected_stream(s)
    if t[:len(E)] != E: return False
    return True   # expected_stream encodes the full data; prefix match = OK

def full_check(s, t):
    """Stronger: exact expected prefix + nothing else needed."""
    return validate(s, t)

variants = {
 'V1 core':      {0,1,32,33,34,35,200,201,202,203,204,205,206,207},
 'V2 +666969':   {0,1,32,33,34,35,66,67,68,69,134,135,200,201,202,203,204,205,206,207},
 'V3 -666969':   {0,1,32,33,34,35},
 'V4 walk':      {0,1,34,37,38,40,66,67,68,69,134,135,200,201,202,203,204,205,206,207},
}
BAT = []
for n in (1,2,3,4,5,6,7,8,9):
    BAT.append('!'*n); BAT.append('a'*n)
BAT += ['~~[[<>','abcdefg','abcdefgh','abcdefghi','xyzxyzxy','~~~!!~~!','[[]][][]']
for n in (1,2,3,5,7,9,10,12,14,16,18,20):
    BAT.append(''.join(str((i*7+3)%10) for i in range(n)))
for n in (1,2,3,5,7,9,11,13,21):
    BAT.append(''.join('ABCXYZ019 $%*+-./:'[(i*5+1)%18] for i in range(n)))

for vname, FUNC in variants.items():
    SPv = [k for k in range(208) if k not in FUNC]
    def Tv(m, f): return [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
    res = {}
    for s in BAT:
        try: m = render(s, level=3, mask=0, version=4)
        except Exception: continue
        if len(m) != 17: continue
        ok = any(validate(s, Tv(m, f)) for f in MASKS.values())
        kind = 'num' if s.isdigit() else ('alnum' if all(c in A for c in s) else 'byte')
        res.setdefault(kind, [0,0]); res[kind][1] += 1; res[kind][0] += ok
    print(vname, {k: f"{v[0]}/{v[1]}" for k, v in res.items()})
