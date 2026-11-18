"""M1 (11x11) first look: path hypothesis + brute-force digit encoding search."""
import sys
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

def _funct(x, y):
    return (x <= 7 and y <= 7) or (x == 8 and y <= 8) or (y == 8 and x <= 7)

def build_path(size):
    """2-col strips from right, alternating up/down; last col alone."""
    n = size - 1
    strips = []
    x = n
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

P11 = build_path(11)
print("M1 path len:", len(P11))
P13 = build_path(13); print("M2 path len:", len(P13))
P15 = build_path(15); print("M3 path len:", len(P15))

# unmask M1 '12345' under each of the 4 validated laws and look at the bits
m = render('12345', level=1, mask=0, version=1)
laws = {
    'row-even': lambda x, y: y % 2 == 0,
    '(xy)%6<3': lambda x, y: (x * y) % 6 < 3,
    'coarse': lambda x, y: (x // 3 + y // 2 + 1) % 2 == 1,
}
for name, f in laws.items():
    bits = [m[y][x] ^ (1 if f(x, y) else 0) for x, y in P11]
    print(f"{name:10}: {''.join(map(str, bits))}")

# reference encodings of 12345
def bl(v, w): return [(v >> (w-1-i)) & 1 for i in range(w)]
print("3+2 group:", bl(123,10)+bl(45,7))
print("pairs+rem:", bl(12,7)+bl(34,7)+bl(5,4))
