"""Ground-truth extraction from the viva field frame: edge-fit -> quad
sample -> orientation arbitration via RS -> mode decode.

Establishes: exact symbol quad, orientation transform, and the payload.
Findings feed the L-finder detector design (docs/STATUS.md).
"""
import json
import sys

sys.path.insert(0, "tools")
import importlib.util
s = importlib.util.spec_from_file_location("p", "tools/dmtx_mode_probe.py")
m = importlib.util.module_from_spec(s); s.loader.exec_module(m)

D = json.load(open('packages/web/test-codes/field/viva-dm-luma.json'))
W, L = D['w'], D['luma']
px = lambda x, y: L[y*W + x]
THR = 150
dark = lambda x, y: px(int(x), int(y)) < THR

# ---- 1. track the solid edges as lines (subpixel, iterative) ----------
# For each row: find the dark solid stroke near the current line estimate,
# then its subpixel OUTER boundary (stroke center = boundary - pitch/2).
def gray(x, y): return px(int(x), int(y))

def track_edge(along, get_val, seed_a, seed_b, cross_axis, lo, hi,
               stroke_sign, win_back, win_fwd):
    """Generic edge tracker. along: coordinate iterated (y for vertical
    edges). get_val(x, y) -> gray. Returns [(along, edge_pos)]."""
    pts = []
    for t in range(lo, hi):
        seed = seed_a + seed_b * t
        # search window for the stroke
        if cross_axis == 'x':
            vals = [(get_val(x, t), x) for x in range(int(seed - win_back), int(seed + win_fwd))]
        else:
            vals = [(get_val(t, y), y) for y in range(int(seed - win_back), int(seed + win_fwd))]
        darkest_v, darkest_at = min(vals, key=lambda p: p[0])
        # background level just beyond the window on the outer side
        if stroke_sign > 0:
            bg_v = max(v for v, _ in vals[-4:])
        else:
            bg_v = max(v for v, _ in vals[:4])
        mid = (darkest_v + bg_v) / 2
        # walk from the darkest point toward the outer side to the midpoint crossing
        i = [v for v, _ in vals].index(darkest_v)
        step = 1 if stroke_sign > 0 else -1
        j = i
        while 0 <= j + step < len(vals) and vals[j][0] < mid:
            j += step
        # subpixel lerp between j-step and j
        v0, p0 = vals[j - step]; v1, p1 = vals[j]
        frac = (mid - v0) / (v1 - v0) if v1 != v0 else 0
        edge = p0 + frac * (p1 - p0)
        pts.append((t, edge - stroke_sign * 4.0))  # center of ~8px stroke
    return pts

# pass 1: coarse run-based centers (previous approach, zone-limited)
def right_edge_y(y):
    run = 0
    for x in range(478, 380, -1):
        if dark(x, y):
            run += 1
            if run >= 6: return x + 2
        else: run = 0
    return None

def bottom_edge_x(x):
    run = 0
    for y in range(865, 570, -1):
        if dark(x, y):
            run += 1
            if run >= 6: return y + 2
        else: run = 0
    return None

rys = [(y, right_edge_y(y)) for y in range(565, 862)]
rys = [(y, x) for y, x in rys if x and 440 <= x <= 478]
bxs = [(x, bottom_edge_x(x)) for x in range(375, 465)]
bxs = [(x, y) for x, y in bxs if y and 830 <= y <= 865]

def fit(pts, c1, c2):
    n = len(pts)
    sx = sum(p[c1] for p in pts); sy = sum(p[c2] for p in pts)
    sxx = sum(p[c1]**2 for p in pts); sxy = sum(p[c1]*p[c2] for p in pts)
    b = (n*sxy - sx*sy) / (n*sxx - sx*sx)
    a = (sy - b*sx) / n
    return a, b

ra, rb = fit(rys, 0, 1)
ba, bb = fit(bxs, 0, 1)

# pass 2+3: subpixel re-track seeded by the fit, then refit
for _ in range(2):
    rys = track_edge(None, gray, ra, rb, 'x', 575, 850, +1, 8, 4)
    bxs = track_edge(None, gray, ba, bb, 'y', 380, 460, +1, 8, 4)
    ra, rb = fit(rys, 0, 1)
    ba, bb = fit(bxs, 0, 1)

print(f"right edge: x = {ra:.2f} + {rb:.4f}*y  (drift {rb*290:.1f}px over height, {len(rys)} pts)")
print(f"bottom edge: y = {ba:.2f} + {bb:.4f}*x (drift {bb*97:.1f}px over width, {len(bxs)} pts)")

# ---- 2. quad corners (rigid rotation model) ---------------------------
# Both edges tilt consistently -> rigid rotation theta. Symbol axes:
#   u_hat (right)  = (cos t, sin t)   bottom edge goes down to the right
#   v_hat (down)   = (-sin t, cos t)  right edge leans left going down
import math
theta = math.atan2((abs(rb) + abs(bb)) / 2, 1.0)
u_hat = (math.cos(theta), math.sin(theta))
v_hat = (-math.sin(theta), math.cos(theta))
x_i = (ra + rb*ba) / (1 - rb*bb)          # right-edge x at BR corner
y_i = ba + bb*x_i                          # BR corner
PITCH = 8.0                                # clock autocorrelation: 16px period
HMOD, WMOD = 36, 12
BR = (x_i, y_i)
TR = (BR[0] - HMOD*PITCH*v_hat[0], BR[1] - HMOD*PITCH*v_hat[1])
TL = (TR[0] - WMOD*PITCH*u_hat[0], TR[1] - WMOD*PITCH*u_hat[1])
BL = (BR[0] - WMOD*PITCH*u_hat[0], BR[1] - WMOD*PITCH*u_hat[1])
print(f"theta={math.degrees(theta):.2f} deg  pitch={PITCH:.2f}")
print(f"corners: TL=({TL[0]:.1f},{TL[1]:.1f}) TR=({TR[0]:.1f},{TR[1]:.1f}) "
      f"BL=({BL[0]:.1f},{BL[1]:.1f}) BR=({BR[0]:.1f},{BR[1]:.1f})")

# ---- 3. sample the grid (bilinear quad) -------------------------------
def sample_quad(NC, NR):
    M = []
    for r in range(NR):
        row = []
        v = (r + 0.5) / NR
        for c in range(NC):
            u = (c + 0.5) / NC
            # bilinear on quad TL,TR,BL,BR
            x = (TL[0]*(1-u)*(1-v) + TR[0]*u*(1-v) + BL[0]*(1-u)*v + BR[0]*u*v)
            y = (TL[1]*(1-u)*(1-v) + TR[1]*u*(1-v) + BL[1]*(1-u)*v + BR[1]*u*v)
            s = n = 0
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    s += dark(x+dx, y+dy); n += 1
            row.append(1 if s*2 > n else 0)
        M.append(row)
    return M

M = sample_quad(12, 36)

# ---- 4. orientation arbitration + decode ------------------------------
def mirror_x(M): return [row[::-1] for row in M]
def mirror_y(M): return M[::-1]
def rot180(M): return [row[::-1] for row in M[::-1]]

def edges(M):
    R, C = len(M), len(M[0])
    return dict(top=sum(M[0])/C, bottom=sum(M[R-1])/C,
                left=sum(M[r][0] for r in range(R))/R,
                right=sum(M[r][C-1] for r in range(R))/R)

def ref(data):
    """Reference mode decoder (banked rules, docs/STATUS.md)."""
    out = bytearray(); i = 0
    def unr(c, p): return (c - ((149*p) % 255) - 1) % 256
    while i < len(data):
        cw = data[i]; i += 1
        if cw <= 128: out.append(cw-1)
        elif cw == 129: break
        elif cw <= 229:
            v = cw-130; out += bytes((48+v//10, 48+v % 10))
        elif cw == 230 or cw == 239:
            kind = 'C40' if cw == 230 else 'TEXT'
            S2 = list(range(33, 48)) + list(range(58, 65)) + list(range(91, 96))
            shift = 0; upper = False
            while i < len(data):
                if data[i] == 254: i += 1; break
                v = data[i]*256 + data[i+1] - 1; i += 2
                for val in (v//1600, v % 1600//40, v % 40):
                    if shift == 0:
                        if val <= 2: shift = val+1; continue
                        if val == 3: ch = 32
                        elif val <= 13: ch = 48+val-4
                        elif val <= 39: ch = (65 if kind == 'C40' else 97)+val-14
                        else: raise ValueError('basic')
                    elif shift == 1: ch = val; shift = 0
                    elif shift == 2:
                        shift = 0
                        if val == 30: upper = True; continue
                        if val == 27: ch = 29
                        elif val < 27: ch = S2[val]
                        else: raise ValueError('s2')
                    else:
                        shift = 0
                        if kind == 'C40' or val == 0 or val >= 27: ch = 96+val
                        else: ch = 64+val
                    if upper: ch += 128; upper = False
                    out.append(ch)
        elif cw == 231:
            ln = unr(data[i], i+1)
            for j in range(ln): out.append(unr(data[i+1+j], i+2+j))
            i += 2 + ln
        elif cw == 235: out.append(data[i]+127); i += 1
        elif cw == 238:
            T = {0: 13, 1: 42, 2: 62, 3: 32}
            while i < len(data):
                if data[i] == 254: i += 1; break
                v = data[i]*256 + data[i+1] - 1; i += 2
                for val in (v//1600, v % 1600//40, v % 40):
                    if val in T: out.append(T[val])
                    elif val <= 13: out.append(48+val-4)
                    elif val <= 39: out.append(65+val-14)
                    else: raise ValueError('x12')
        elif cw == 240:
            while i + 3 <= len(data):
                bits = data[i] << 16 | data[i+1] << 8 | data[i+2]; i += 3
                brk = False
                for k in range(4):
                    val = (bits >> (18-6*k)) & 63
                    if val == 31: brk = True; break
                    out.append(val if val >= 32 else val + 64)
                if brk: break
        else: raise ValueError(f'cw {cw}')
    return bytes(out)

spec = m.load_spec(12, 36)   # structure keys are "cols x rows"
print(f"spec: data={spec['data']} total={spec['total']} blocks={spec['blocks']}")

for name, T in [("identity", lambda M: M), ("mirror-x", mirror_x),
                ("mirror-y", mirror_y), ("rot180", rot180)]:
    Mp = T(M)
    try:
        full = m.read_stream(Mp, 36, 12, spec['total'])
    except Exception as e:
        print(f"{name}: read failed {e}"); continue
    ok = m.rs_validate(full, spec)
    line = f"{name:9s} RS={'OK ' if ok else 'FAIL'}"
    if ok:
        try:
            payload = ref(full[:spec['data']])
            line += f"  payload={payload!r}"
        except Exception as e:
            line += f"  mode-err {e}"
    print(line)

json.dump(M, open('/tmp/viva_matrix.json', 'w'))
