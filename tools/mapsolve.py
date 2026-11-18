"""Definitive bit->cell mapping via differential pairs.

'aaaaaaaaa' vs (chr(0x61 ^ (1<<j)))*9 for j=0..6: same length/mode/count,
bitstreams differ at exactly the 9 data bits {6+8i+j}. The differing SP
cells (sorted) map 1:1 onto those bits, revealing the true data path and
therefore every skipped (function) cell.
"""
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
assert len(PATH) == 208

OLD = {0,1,30,33,66,67,68,69,134,135} | set(range(200,208))
SP_OLD = [k for k in range(208) if k not in OLD]          # session-6/7 stream
MASKS = {
    'r%2=0':    lambda x, y: y % 2 == 0,
    '(y>>1)&1': lambda x, y: (y >> 1) & 1,
    '(x>>1)&1': lambda x, y: (x >> 1) & 1,
    '(x+y)%2=0': lambda x, y: (x + y) % 2 == 0,
    '(x+y)%3=0': lambda x, y: (x + y) % 3 == 0,
    'x%3=0':    lambda x, y: x % 3 == 0,
}

def stream(m, f, sp_idx):
    return [m[y][x] ^ (1 if f(x, y) else 0) for x, y in
            [PATH[k] for k in sp_idx]]

DATA_END = 6 + 72          # hdr + 9 bytes
mapping = {}               # data bit -> old-SP cell
for j in range(7):
    pa = 'a' * 9
    pb = chr(0x61 ^ (1 << j)) * 9
    ma = render(pa, level=3, mask=0, version=4)
    mb = render(pb, level=3, mask=0, version=4)
    for name, f in MASKS.items():
        t0 = stream(ma, f, SP_OLD); t1 = stream(mb, f, SP_OLD)
        diffs = [i for i in range(len(t0)) if t0[i] != t1[i]]
        lo = [i for i in diffs if i < DATA_END + 6]
        if len(diffs) == 9 and lo == diffs:
            bits = [6 + 8*i + j for i in range(9)]
            if all(b not in mapping for b in bits):
                for b, c in zip(bits, diffs):
                    mapping[b] = c
            print(f"j={j} mask={name}: diffs {diffs} -> bits {bits}")
            break
    else:
        print(f"j={j}: no clean pair (diffs={len(diffs)})")

bits_sorted = sorted(mapping)
print("\nbit -> old-SP cell (data region):")
prev_bit, prev_cell = None, None
skips = []
for b in bits_sorted:
    c = mapping[b]
    if prev_bit is not None:
        gap = c - prev_cell - (b - prev_bit)
        if gap != 0:
            skips.append((prev_bit, b, prev_cell, c, gap))
    prev_bit, prev_cell = b, c
    print(f"  bit {b:3} -> SP {c:3}")
print("\ngaps (between bit a and b: cells a->b, gap):", skips)
