"""Single-bit-flip probes: base 'aaaaaaaaa' vs base with byte i bit j flipped.
A clean probe shows exactly 1 diff cell in the data region -> that cell
carries data bit 6+8i+j. Also: does zint option_3 (mask) do anything?"""
import sys
sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

def _funct(x, y):
    return (x <= 7 and y <= 7) or (x == 8 and y <= 8) or (y == 8 and x <= 7)
_STRIPS = [(16,15),(14,13),(12,11),(10,9),(8,7),(6,5),(4,3),(2,1),(0,None)]
PATH = []
for _si, (_hi, _lo) in enumerate(_STRIPS):
    _rows = range(16, -1, -1) if _si % 2 == 0 else range(17)
    for _y in _rows:
        for _x in ((_hi,) if _lo is None else (_hi, _lo)):
            if not _funct(_x, _y):
                PATH.append((_x, _y))
OLD = {0,1,30,33,66,67,68,69,134,135} | set(range(200,208))
SP_OLD = [k for k in range(208) if k not in OLD]
MASKS = {
    'r%2=0':    lambda x, y: y % 2 == 0,
    '(y>>1)&1': lambda x, y: (y >> 1) & 1,
    '(x>>1)&1': lambda x, y: (x >> 1) & 1,
    '(x+y)%2=0': lambda x, y: (x + y) % 2 == 0,
    '(x+y)%3=0': lambda x, y: (x + y) % 3 == 0,
    'x%3=0':    lambda x, y: x % 3 == 0,
}
def stream(m, f):
    return [m[y][x] ^ (1 if f(x, y) else 0) for x, y in [PATH[k] for k in SP_OLD]]

# --- does option_3 change anything? ---
m0 = render('aaaaaaaaa', level=3, mask=0, version=4)
mats = [render('aaaaaaaaa', level=3, mask=mk, version=4) for mk in (1,2,3)]
print("option_3 changes matrix:", [m != m0 for m in mats])

base = render('aaaaaaaaa', level=3, mask=0, version=4)
mapping = {}
good = bad = 0
for b in range(6, 78):
    i, j = divmod(b - 6, 8)
    if j == 7: continue  # MSB: 'a'->'a'+128 not ascii; handle via alt base later
    s2 = 'a'*i + chr(0x61 ^ (1 << j)) + 'a'*(9-i-1)
    m2 = render(s2, level=3, mask=0, version=4)
    hit = None
    for name, f in MASKS.items():
        t0, t1 = stream(base, f), stream(m2, f)
        d = [p for p in range(6, 85) if t0[p] != t1[p]]
        if len(d) == 1:
            hit = (name, d[0]); break
    if hit: mapping[b] = (hit[1], hit[0]); good += 1
    else: bad += 1
print(f"\nprobes ok: {good}, failed: {bad}")
for b in sorted(mapping):
    c, mn = mapping[b]
    print(f"  bit {b:3} -> SP {c:3}  (mask {mn})")
