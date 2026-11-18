"""Brute-force alignment search for the viva symbol: sample the 12x36
grid over (theta, pitch, x@midheight, y@midheight), RS-arbitrate.

The RS check is the ultimate objective - a wrong alignment cannot pass.
"""
import json
import math
import sys

sys.path.insert(0, "tools")
import importlib.util
s = importlib.util.spec_from_file_location("p", "tools/dmtx_mode_probe.py")
m = importlib.util.module_from_spec(s); s.loader.exec_module(m)

D = json.load(open('packages/web/test-codes/field/viva-dm-luma.json'))
W, L = D['w'], D['luma']
G = [L[y*W:(y+1)*W] for y in range(D['h'])]
THR = 150

spec = m.load_spec(12, 36)

def sample(theta, pitch, xm, ym):
    """Sample a 12x36 grid centered at (xm, ym) at mid-height, rotated theta."""
    ct, st = math.cos(theta), math.sin(theta)
    # symbol local axes: u right, v down
    M = []
    for r in range(36):
        row = []
        v = (r - 17.5) * pitch
        for c in range(12):
            u = (c - 5.5) * pitch
            # image = center + u*(cos,sin) + v*(-sin,cos)
            x = xm + u*ct - v*st
            y = ym + u*st + v*ct
            s = n = 0
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    s += G[int(y)+dy][int(x)+dx] < THR; n += 1
            row.append(1 if s*2 > n else 0)
        M.append(row)
    return M

def mirror_x(M): return [row[::-1] for row in M]
def mirror_y(M): return M[::-1]
def rot180(M): return [row[::-1] for row in M[::-1]]

# A is 36 rows x 12 cols (image orientation). The canonical "12x36"
# symbol is 12 rows x 36 cols -> the dims-preserving transforms can't
# work; the transpose family does:
def transposes(A):
    return {
        "T":      [[A[c][r] for c in range(36)] for r in range(12)],
        "rot90":  [[A[35-c][r] for c in range(36)] for r in range(12)],
        "rot270": [[A[c][11-r] for c in range(36)] for r in range(12)],
        "T180":   [[A[35-c][11-r] for c in range(36)] for r in range(12)],
    }

best = []
xm0, ym0 = 415.0, 705.0   # mid-height center estimate
count = 0
for theta_deg in (1.8, 2.2, 2.6, 2.73, 2.9, 3.3):
    th = math.radians(theta_deg)
    for pitch in (7.9, 8.0, 8.1, 8.17):
        for dx in range(-4, 5, 2):
            for dy in range(-6, 7, 2):
                A = sample(th, pitch, xm0+dx, ym0+dy)
                for name, Mp in transposes(A).items():
                    full = m.read_stream(Mp, 12, 36, spec['total'])
                    count += 1
                    if m.rs_validate(full, spec):
                        best.append((theta_deg, pitch, xm0+dx, ym0+dy, name, full))
print(f"tried {count} alignments, {len(best)} RS-valid")
for b in best[:6]:
    print(b[:5])
if best:
    json.dump(best[0][5], open('/tmp/viva_stream.json', 'w'))
    print("stream saved")
