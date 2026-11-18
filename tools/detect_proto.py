"""Prototype L-finder corner detection on the viva field frame.

Question for this step: does dark-run clustering find the symbol's L
corner (bottom-right, ~(454,853), L at bottom+right) among a manageable
number of false candidates? Verified visually + numerically before
porting to Rust (crates/barcode-dm/src/locator.rs).

Algorithm:
1. Bradley-style local-mean binarization via integral image (window 49).
2. Per-row dark runs >= MIN_RUN px -> cluster into horizontal bars.
3. Per-column dark runs -> vertical bars.
4. Pair touching bar ends -> L corner candidates.
"""
import json
import struct
import zlib
from itertools import accumulate

D = json.load(open('packages/web/test-codes/field/viva-dm-luma.json'))
W, H, L = D['w'], D['h'], D['luma']

# ---- 1. binarize (local mean, Bradley k=0.15, window 49) ----------------
R = 24
# integral image: II[y][x] = sum of luma over rows<=y, cols<=x
II = [[0] * (W + 1)]
prev = [0] * (W + 1)
for y in range(H):
    row = L[y * W:(y + 1) * W]
    cur = [p + c for p, c in zip(prev, accumulate(row, initial=0))]
    II.append(cur)
    prev = cur

def rect_mean(x, y):
    x0, y0 = max(0, x - R), max(0, y - R)
    x1, y1 = min(W, x + R + 1), min(H, y + R + 1)
    s = II[y1][x1] - II[y0][x1] - II[y1][x0] + II[y0][x0]
    return s / ((x1 - x0) * (y1 - y0))

BIN = bytearray(W * H)
for y in range(H):
    base = y * W
    for x in range(W):
        if L[base + x] < rect_mean(x, y) * 0.85:
            BIN[base + x] = 1

# ---- 2. horizontal runs -> bars -----------------------------------------
MIN_RUN = 15
def runs_1d(get, n):
    """Maximal dark runs (start, end_exclusive) of length >= MIN_RUN."""
    out = []
    start = None
    for i in range(n + 1):
        d = get(i) if i < n else False
        if d and start is None:
            start = i
        elif not d and start is not None:
            if i - start >= MIN_RUN:
                out.append((start, i))
            start = None
    return out

hbars = []   # (x0, y0, x1, y1) bounding boxes of clustered runs
def cluster(rs, key_a, key_b):
    """Cluster runs: consecutive key_a rows whose key_b ranges overlap
    (with 2px tolerance) merge into one bar bounding box."""
    bars = []   # [a0, b0, a1, b1] in (key_a, key_b) coords
    cur = None
    prev_a = None
    open_ranges = []   # (b0, b1) of the previous row
    for a, r in rs:
        a0, a1, b0, b1 = a, a + 1, r[0], r[1]
        if cur is not None and a == prev_a + 1 and any(
                b0 < ob1 + 2 and ob0 < b1 + 2 for ob0, ob1 in open_ranges):
            cur[0] = min(cur[0], a0); cur[1] = min(cur[1], b0)
            cur[2] = max(cur[2], a1); cur[3] = max(cur[3], b1)
            open_ranges = [(b0, b1)]
        else:
            if cur: bars.append(tuple(cur))
            cur = [a0, b0, a1, b1]
            open_ranges = [(b0, b1)]
        prev_a = a
    if cur: bars.append(tuple(cur))
    return bars

hrs = []  # (y, (x0, x1))
for y in range(H):
    base = y * W
    for x0, x1 in runs_1d(lambda i, b=base: BIN[b + i], W):
        hrs.append((y, (x0, x1)))
hrs = []  # (y, (x0, x1))
for y in range(H):
    base = y * W
    for x0, x1 in runs_1d(lambda i, b=base: BIN[b + i], W):
        hrs.append((y, (x0, x1)))
hbars = [(b[1], b[0], b[3], b[2]) for b in cluster(hrs, 0, 1)]   # -> (x0,y0,x1,y1)
vrs = []
for x in range(W):
    for y0, y1 in runs_1d(lambda i, x=x: BIN[i * W + x], H):
        vrs.append((x, (y0, y1)))
vbars = cluster(vrs, 0, 1)   # (x0, y0, x1, y1) already

def filt_len(bars, horiz):
    out = []
    for x0, y0, x1, y1 in bars:
        ln = (x1 - x0) if horiz else (y1 - y0)
        th = (y1 - y0) if horiz else (x1 - x0)
        if ln >= 7 * th and ln >= 40 and th <= 30:
            out.append((x0, y0, x1, y1))
    return out

hbars = filt_len(hbars, True)
vbars = filt_len(vbars, False)
print(f"hbars={len(hbars)} vbars={len(vbars)}")

# ---- 4. corner candidates ----------------------------------------------
# An L corner = the end of one stroke meeting the other stroke's band.
# 4 orientations: corner at BL (canonical), BR (rot90ccw), TR (rot180),
# TL (rot270). vdir/hdir record which end of each stroke forms the L.
corners = []
for vx0, vy0, vx1, vy1 in vbars:
    for hx0, hy0, hx1, hy1 in hbars:
        TOL = 4
        for vend in (vy0, vy1):
            if not (hy0 - TOL <= vend <= hy1 + TOL):
                continue
            for hend in (hx0, hx1):
                if not (vx0 - TOL <= hend <= vx1 + TOL):
                    continue
                corners.append((hend, vend, hx1 - hx0, vy1 - vy0))
# dedup: merge corners within 6px
corners.sort()
dedup = []
for c in corners:
    if dedup and abs(c[0] - dedup[-1][0]) <= 6 and abs(c[1] - dedup[-1][1]) <= 6:
        dedup[-1] = (min(dedup[-1][0], c[0]), min(dedup[-1][1], c[1]),
                     max(dedup[-1][2], c[2]), max(dedup[-1][3], c[3]))
    else:
        dedup.append(list(c))
print(f"corners raw={len(corners)} dedup={len(dedup)}")
for c in dedup[:40]:
    print(f"   corner ({c[0]},{c[1]}) hstroke={c[2]}px vstroke={c[3]}px")

# ---- visualize -----------------------------------------------------------
def png(path, w, h, gray):
    def chunk(t, data):
        c = t + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)
    rows = b''.join(b'\x00' + bytes(r) for r in gray)
    open(path, 'wb').write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 0, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))

X0, Y0, X1, Y1 = 330, 530, 520, 900
g = [[255] * (X1 - X0) for _ in range(Y1 - Y0)]
for y in range(Y0, Y1):
    for x in range(X0, X1):
        g[y - Y0][x - X0] = 235 if BIN[y * W + x] else 255
def mark(x, y, v):
    if X0 <= x < X1 and Y0 <= y < Y1:
        g[y - Y0][x - X0] = v
for (vx0, vy0, vx1, vy1) in vbars:
    if vx1 < X0 or vx0 > X1 or vy1 < Y0 or vy0 > Y1: continue
    for y in range(max(vy0, Y0), min(vy1, Y1)):
        mark(vx0, y, 160); mark(vx1 - 1, y, 160)
for (hx0, hy0, hx1, hy1) in hbars:
    if hx1 < X0 or hx0 > X1 or hy1 < Y0 or hy0 > Y1: continue
    for x in range(max(hx0, X0), min(hx1, X1)):
        mark(x, hy0, 100); mark(x, hy1 - 1, 100)
for cx, cy, _hl, _vl in corners:
    if X0 <= cx < X1 and Y0 <= cy < Y1:
        for dx in range(-4, 5):
            mark(cx + dx, cy, 0); mark(cx, cy + dx, 0)
png('/tmp/detect_proto.png', X1 - X0, Y1 - Y0, g)
print("wrote /tmp/detect_proto.png (gray=bar outlines, black=corners)")
