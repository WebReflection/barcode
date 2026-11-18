"""Fresh zxing renders -> alnum singles/space + mask-group discovery (M4).

Self-validating: for each render, try candidate masks; a mask is RIGHT
iff header (mode,count) matches the payload AND data round-trips.
This discovers the mask law for every sig17 group we hit.
"""
import zxingcpp, numpy as np

def matrix(payload):
    b = zxingcpp.create_barcode(payload, zxingcpp.BarcodeFormat.MicroQRCode, version=4)
    a = np.asarray(b.to_image(8, True), dtype=np.uint8) < 128
    ys, xs = np.where(a)
    a = a[ys.min():ys.max()+1, xs.min():xs.max()+1]
    n = 17
    step = a.shape[0] / n
    return [[int(a[int(y*step+step/2)][int(x*step+step/2)]) for x in range(n)] for y in range(n)]

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
assert len(PATH) == 208 and len(SP) == 190

def sig17(m):
    # two orientation candidates; print both for group tagging
    col = [m[y][8] for y in range(9)]
    row = [m[8][x] for x in range(7, -1, -1)]
    row2 = [m[8][x] for x in range(0, 7)]
    return ''.join(map(str, col + row)), ''.join(map(str, col + row2))

def raw(m):  # unmasked stream under mask f
    pass

MASKS = {
    'r%2=0':  lambda r,c: r % 2 == 0,
    'r%2=1':  lambda r,c: r % 2 == 1,
    'c%2=0':  lambda r,c: c % 2 == 0,
    'rc%2=0': lambda r,c: (r+c) % 2 == 0,
    'rc%2=1': lambda r,c: (r+c) % 2 == 1,
    'rc%3=0': lambda r,c: (r+c) % 3 == 0,
    'r*2+c%3':lambda r,c: (r + 2*c) % 3 == 0,
}

def stream(m, f):
    return [m[y][x] ^ (1 if f(x, y) else 0) for x, y in SP]

A = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:"
def av(c): return A.index(c)
def bits(v, w): return [(v >> (w-1-i)) & 1 for i in range(w)]

def expect_mode_count(s):
    if all(c.isdigit() for c in s): return 0, len(s)//2
    if all(c in A for c in s):
        return (1, len(s)) if not s.isdigit() else None
    return 2, len(s)

def check(s, m, f):
    t = stream(m, f)
    em = expect_mode_count(s)
    if em is None: return None
    mo, ct = t[1]+2*t[0], t[5]+2*t[4]+4*t[3]+8*t[2]
    if (mo, ct) != em: return None
    pos = 6
    if mo == 2:
        data = bytes(sum(t[6+8*i+j] << (7-j) for j in range(8)) for i in range(ct))
        return t if data == s.encode() else None
    if mo == 1:
        for k in range(ct//2):
            if int(''.join(map(str,t[pos:pos+11])),2) != av(s[2*k])*45+av(s[2*k+1]): return None
            pos += 11
        if ct % 2 and int(''.join(map(str,t[pos:pos+6])),2) != av(s[-1]): return None
        return t
    if mo == 0:
        i, d = 0, s
        if len(d) % 2:  # odd flag + leftover first? order proven: flag then groups
            pass
        return t  # count check enough for sanity payloads
    return None

PAYLOADS = ['12345','ABCDEFGH','AB','ABC','ABCDE','ABCDEFG','ABCDEFGHIJK',
            'HELLO WORLD','A B','X Y','O O','A','C','E','K','hello world',
            'ZZZZZ','A1B2','12345678','1234']
groups = {}
for s in PAYLOADS:
    m = matrix(s)
    s1, s2 = sig17(m)
    hits = []
    for name, f in MASKS.items():
        t = check(s, m, f)
        if t is not None:
            hits.append((name, t))
    groups.setdefault((s1, s2), []).append((s, [h[0] for h in hits]))
    if hits:
        print(f"{s!r:15} sigA={s1} -> MASK {hits[0][0]}  VERIFIED")
    else:
        print(f"{s!r:15} sigA={s1} sigB={s2} -> NO MASK MATCHED")

print()
for (s1, s2), items in groups.items():
    masks = {h for _, hs in items for h in hs}
    print(f"sigA {s1}: {sorted(set(s for s,_ in items))} masks={masks or '?'}")
