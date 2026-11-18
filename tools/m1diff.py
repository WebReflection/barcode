"""M1 data layer via payload diffs: '0000k' probes locate digit bit positions."""
import sys
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

P = None
def cells(s, v=1):
    global P
    m = render(s, level=1, mask=0, version=v)
    P = build_path(11) if v == 1 else P
    return {(x, y) for y in range(len(m)) for x in range(len(m)) if m[y][x] == 1}, m

def diff(s1, s2, v=1):
    c1, m1 = cells(s1, v); c2, m2 = cells(s2, v)
    d = c1 ^ c2
    pos = sorted(P.index((x, y)) for x, y in d if (x, y) in set(P))
    extra = [(x, y) for x, y in d if (x, y) not in set(P)]
    return pos, extra

if 'build_path' not in dir():
    def _funct(x, y):
        return (x <= 7 and y <= 7) or (x == 8 and y <= 8) or (y == 8 and x <= 7)
    def build_path(size):
        n = size - 1; strips = []; x = n
        while x > 0:
            strips.append((x, x - 1)); x -= 2
        if x == 0: strips.append((0, None))
        PATH = []
        for si, (hi, lo) in enumerate(strips):
            rows = range(size - 1, -1, -1) if si % 2 == 0 else range(size)
            for y in rows:
                for xx in ((hi,) if lo is None else (hi, lo)):
                    if not _funct(xx, y):
                        PATH.append((xx, y))
        return PATH

print("path:", len(build_path(11)))
for a, b in (('00000','00001'), ('00000','00002'), ('00000','00003'),
             ('00003','00001'), ('00000','00010'), ('00000','00100'),
             ('00000','01000'), ('00000','10000'),
             ('00000','0000'), ('0000','000'), ('000','00'), ('00','0')):
    try:
        pos, extra = diff(a, b)
        tag = f"+{extra} OUT" if extra else ""
        print(f"{a!r} vs {b!r}: path-pos {pos} {tag}")
    except Exception as ex:
        print(f"{a!r} vs {b!r}: FAIL {str(ex)[-50:]}")
