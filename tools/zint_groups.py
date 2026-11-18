"""zint renders with forced level/mask -> find G1-equivalent renders, validate
numeric (flag + triples + tail), byte, alnum across all groups."""
import sys
sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render, dump
import numpy as np

def funct(x, y):
    return (x <= 7 and y <= 7) or (x == 8 and y <= 8) or (y == 8 and x <= 7)

STRIPS = [(16,15),(14,13),(12,11),(10,9),(8,7),(6,5),(4,3),(2,1),(0,None)]
def path():
    p = []
    for si, (hi, lo) in enumerate(STRIPS):
        rows = range(16,-1,-1) if si % 2 == 0 else range(17)
        for y in rows:
            for x in ((hi,) if lo is None else (hi, lo)):
                if not funct(x, y):
                    p.append((x, y))
    return p

PATH = path()
CONST = {0,1,30,33,66,67,68,69,134,135} | set(range(200,208))
SP = [c for k,c in enumerate(PATH) if k not in CONST]

def sigA(m):
    return ''.join(str(m[y][8]) for y in range(9)) + ''.join(str(m[8][x]) for x in range(7,-1,-1))

def stream(m, f):
    return [m[y][x] ^ (1 if f(x, y) else 0) for x, y in SP]

A = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:"
def av(c): return A.index(c)
def b2i(bl): return int(''.join(map(str,bl)),2)

MASKS = {
    'r%2=0': lambda x,y: y%2==0, 'r%2=1': lambda x,y: y%2==1,
    'c%2=0': lambda x,y: x%2==0, 'rc%2=0': lambda x,y:(x+y)%2==0,
    'rc%2=1': lambda x,y:(x+y)%2==1, 'rc%3=0': lambda x,y:(x+y)%3==0,
    '(r+2c)%3': lambda x,y:(y+2*x)%3==0, '(2r+c)%3': lambda x,y:(2*y+x)%3==0,
}

def validate(s, t):
    """Full round-trip check; returns mode or None."""
    if all(c.isdigit() for c in s):
        n = len(s); cnt = b2i(t[1:6])
        if cnt != n//2: return None
        if t[6] != n % 2: return None
        pos = 7; d = s
        while len(d) >= 3:
            if b2i(t[pos:pos+10]) != int(d[:3]): return None
            pos += 10; d = d[3:]
        if len(d) == 2:
            if b2i(t[pos:pos+7]) != int(d): return None
            pos += 7
        elif len(d) == 1:
            if b2i(t[pos:pos+4]) != int(d): return None
            pos += 4
        return 'num'
    if all(c in A for c in s):
        cnt = b2i(t[1:6])
        if cnt != len(s) or t[0] != 1: return None
        pos = 6
        for k in range(cnt//2):
            if b2i(t[pos:pos+11]) != av(s[2*k])*45+av(s[2*k+1]): return None
            pos += 11
        if cnt % 2 and b2i(t[pos:pos+6]) != av(s[-1]): return None
        return 'alnum'
    cnt = b2i(t[1:6])
    if cnt != len(s) or t[0] != 0: return None
    data = bytes(b2i(t[6+8*i:6+8*i+8]) for i in range(cnt))
    return 'byte' if data == s.encode() else None

PAYLOADS = ['1','12','1234','12345','1234567','12345678','1234567890',
            '9','99','999','a','aa','abcdefgh','AB','ABC','ABCDEFG','A B','X Y']
found = {}
for s in PAYLOADS:
    for lv in (1,2,3,4):
        for mk in (0,1,2,3):
            try:
                m = render(s, level=lv, mask=mk, version=4)
            except Exception as e:
                continue
            if len(m) != 17: continue
            sg = sigA(m)
            for name, f in MASKS.items():
                t = stream(m, f)
                mode = validate(s, t)
                if mode:
                    found.setdefault(sg, []).append((s, lv, mk, name, mode))
for sg, items in sorted(found.items()):
    masks = {it[3] for it in items}
    modes = {it[4] for it in items}
    print(f"sig {sg}: masks={masks} modes={modes} n={len(items)}")
    for it in items[:4]:
        print("   ", it)
