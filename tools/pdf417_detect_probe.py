"""PDF417 detector probe: scan-line start/stop matching -> bands ->
row extraction -> full decode. Works on padded canvases (quiet zone),
not exact matrices.

Symbol row geometry (modules): [start 17][left 17][data*nc][right 17]
[stop 18] = 69 + 17*nc total width. Start runs [8,1,1,1,1,1,1,3],
stop runs [7,1,1,3,1,1,1,2,1]. Codeword rows render as uniform bands.
"""

import math
import random
import sys
from collections import Counter

sys.path.insert(0, "/Users/ag/git/barcode/tools")
from pdf417_probe import raw_rows  # noqa: E402
from pdf417_rs_probe import rs_correct  # noqa: E402


def rs_valid(full, k):
    return all(s == 0 for s in __import__("pdf417_rs_probe").syndromes(full, k))
from pdf417_modes_probe import interpret  # noqa: E402
from pyzint import zint  # noqa: E402

TABLE = None  # lazy: {cluster: {pattern: value}}


def load_table():
    global TABLE
    if TABLE is None:
        import json
        t = json.load(open("/tmp/pdf417_table.json"))
        TABLE = {int(k): {p: int(v) for v, p in t[k].items()} for k in t}
    return TABLE


START_RUNS = [8, 1, 1, 1, 1, 1, 1, 3]
STOP_RUNS = [7, 1, 1, 3, 1, 1, 1, 2, 1]


# ---------------------------------------------------------------- image


def render(payload, level=1, cols=0):
    z = zint.Zint(payload, zint.BARCODE_PDF417, scale=1, show_text=False,
                  border_width=0, whitespace_width=0,
                  option_1=level, option_2=cols)
    w, h, get_row = raw_rows(z.render_bmp())
    mw, mh = w // 2, h // 2
    return [[1 - get_row(h - 1 - 2 * y)[x * 2] for x in range(mw)]
            for y in range(mh)]


def paste(matrix, qz=4, noise_px=0, seed=1, upscale=2):
    """Paste into a (2*qz) padded canvas; optionally flip random pixels.
    The matrix is upscaled `upscale`x (nearest neighbor) so module width
    matches a realistic camera render."""
    rng = random.Random(seed)
    m = matrix
    if upscale > 1:
        m = [[v for v in row for _ in range(upscale)] for row in matrix
             for _ in range(upscale)]
    H, W = len(m) + 2 * qz, len(m[0]) + 2 * qz
    canvas = [[0] * W for _ in range(H)]
    for y in range(len(m)):
        for x in range(len(m[0])):
            canvas[y + qz][x + qz] = m[y][x]
    for _ in range(noise_px):
        x, y = rng.randrange(W), rng.randrange(H)
        canvas[y][x] ^= 1
    return canvas


def runs_of(row):
    """[(is_dark, length)] starting with the first pixel's color."""
    out = []
    cur = row[0] == 1
    n = 0
    for d in row:
        dark = d == 1
        if dark == cur:
            n += 1
        else:
            out.append((cur, n))
            cur, n = dark, 1
    out.append((cur, n))
    return out


# ---------------------------------------------------------------- match


def match_runs(runs, offset, nominal, s, tol=0.5):
    """Do `len(nominal)` runs starting at runs[offset] match the nominal
    module pattern at scale s (each within tol·s)?"""
    if offset + len(nominal) > len(runs):
        return False
    for k, want in enumerate(nominal):
        dark, ln = runs[offset + k]
        if dark != (k % 2 == 0):
            return False
        lo, hi = want * s - tol * s, want * s + tol * s
        if not (lo <= ln <= hi):
            return False
    return True


def scan_candidates(canvas):
    """(x0, x1, s, y) candidate codeword rows: a start pattern paired
    with a stop pattern on one scan line at consistent scale."""
    hits = []
    for y, row in enumerate(canvas):
        runs = runs_of(row)
        # prefix sums -> absolute x of each run start
        xs = []
        acc = 0
        for _, ln in runs:
            xs.append(acc)
            acc += ln
        starts = []
        for i in range(len(runs) - 7):
            # scale estimate: total start width is exactly 17 modules
            w17 = sum(ln for _, ln in runs[i:i + 8])
            if w17 % 17:
                continue
            s = w17 // 17
            if not (2 <= s <= 40) or not match_runs(runs, i, START_RUNS, s):
                continue
            starts.append((i, s, xs[i]))
        if not starts:
            continue
        stops = []
        for i in range(len(runs) - 8):
            w18 = sum(ln for _, ln in runs[i:i + 9])
            if w18 % 18:
                continue
            s = w18 // 18
            if not (2 <= s <= 40) or not match_runs(runs, i, STOP_RUNS, s):
                continue
            stops.append((i, s, xs[i] + w18))
        for si, ss, x0 in starts:
            for ti, ts, x1 in stops:
                if abs(ss - ts) > max(1, ss // 8):
                    continue
                width_m = (x1 - x0) / ss
                ncols = int(math.floor((width_m - 69) / 17 + 0.5))
                if ncols < 1 or ncols > 30:
                    continue
                if abs(width_m - (69 + 17 * ncols)) > 1.0:
                    continue
                hits.append((x0, x1, ss, y, ncols))
    return hits


def cluster_bands(hits):
    """Group CONSECUTIVE scan lines with identical (x0, x1, s, sig) into
    bands; merge same-key bands split by stray scan lines; drop
    single-line specks. Returns [(x0, x1, s, y0, y1, ncols)] by y0."""
    bands = []
    for x0, x1, s, y, ncols, sig in hits:
        key = (x0, x1, s, sig)
        if bands and bands[-1][0] == key and y - bands[-1][3] <= 2:
            bands[-1][3] = y
        else:
            bands.append([key, ncols, y, y])
    merged = []
    for key, ncols, y0, y1 in bands:
        if merged and merged[-1][0] == key and y0 - merged[-1][3] <= 3:
            merged[-1][3] = y1
        else:
            merged.append([key, ncols, y0, y1])
    return [(k[0], k[1], k[2], y0, y1, n)
            for k, n, y0, y1 in merged if y1 - y0 + 1 >= 2]


# ---------------------------------------------------------------- extract


def value_at(cluster, elems):
    return load_table()[cluster].get("".join(map(str, elems)))


def parse_line(canvas, y, x0, x1, s, ncols):
    """Strict cell parse of one scan line, or None.

    Run boundaries are snapped to the module grid first, so ±s/2 px of
    edge error (blur, thresholding) does not corrupt the elements.
    """
    row = canvas[y][x0:x1]
    runs = runs_of(row)
    mods = []  # (dark, width in modules), boundaries snapped
    pos = 0
    prev_b = 0
    for dark, ln in runs:
        pos += ln
        b = int(math.floor(pos / s + 0.5))  # round half up (matches Rust)
        w = b - prev_b
        prev_b = b
        if w > 0:
            if mods and mods[-1][0] == dark:
                mods[-1] = (dark, mods[-1][1] + w)
            else:
                mods.append((dark, w))
    widths = [w for _, w in mods]
    if not widths or widths[0] == 0:
        return None
    if widths[:8] != START_RUNS or widths[-9:] != STOP_RUNS:
        return None
    cells = []
    idx = 0
    bounds = [17, 34] + [34 + 17 * (k + 1) for k in range(ncols)] \
        + [34 + 17 * ncols + 17, 34 + 17 * ncols + 35]
    pos = 0
    for ci, end_m in enumerate(bounds):
        elems = []
        while pos < end_m:
            take = min(end_m - pos, widths[idx])
            elems.append(take)
            pos += take
            if take == widths[idx]:
                idx += 1
            else:
                widths[idx] -= take
        # codeword cells: exactly 8 elements (4 bars + 4 spaces) summing
        # to 17 modules; stop: 9 elements / 18. Corrupt lines fail here
        # and never vote.
        want_n, want_sum = (9, 18) if ci == len(bounds) - 1 else (8, 17)
        if len(elems) != want_n or sum(elems) != want_sum or min(elems) < 1:
            return None
        cells.append(elems)
    return cells


def detect(canvas, verbose=False):
    """Full decode: payload bytes or None.

    Each scan-line hit identifies its own symbol row via the left row
    indicator; the cluster comes from trial-decoding the first data
    cell (the three cluster tables are disjoint). Cell values are
    majority-voted per row across scan lines; RS is the final arbiter.
    """
    hits = scan_candidates(canvas)
    symbols = {}
    for x0, x1, s, y, ncols in hits:
        symbols.setdefault((x0, x1, s, ncols), []).append(y)
    # largest symbol first
    for (x0, x1, s, ncols), ys in sorted(symbols.items(),
                                         key=lambda kv: -len(kv[1])):
        votes = {}  # symbol row -> {cell index: Counter of element tuples}
        for y in ys:
            cells = parse_line(canvas, y, x0, x1, s, ncols)
            if cells is None:
                continue
            for c in (0, 3, 6):
                if value_at(c, cells[2]) is not None:
                    cluster = c
                    break
            else:
                continue
            left = value_at(cluster, cells[1])
            if left is None:
                continue
            x_ind, _ = divmod(left, 30)
            right = value_at(cluster, cells[2 + ncols])
            if right is None:
                continue
            if divmod(right, 30)[0] != x_ind:
                continue
            row = 3 * x_ind + cluster // 3 + 1
            per = votes.setdefault(row, {})
            for i, cell in enumerate(cells):
                per.setdefault(i, Counter())[tuple(cell)] += 1
        if not votes:
            continue
        nrows = max(votes)
        if set(votes) != set(range(1, nrows + 1)):
            if verbose:
                print(f"  rows incomplete: {sorted(votes)}")
            continue
        stream = []
        ok = True
        for row in range(1, nrows + 1):
            cluster = ((row - 1) % 3) * 3
            cells = [list(votes[row][i].most_common(1)[0][0])
                     for i in range(4 + ncols)]
            for cell in cells[2:2 + ncols]:
                v = value_at(cluster, cell)
                if v is None:
                    ok = False
                    break
                stream.append(v)
            if not ok:
                break
        if not ok:
            continue
        desc = stream[0]
        if not (0 < desc < len(stream)):
            continue
        k = len(stream) - desc
        if k < 2:
            continue
        if not rs_valid(stream, k):
            fixed = rs_correct(stream, k)
            if fixed is None:
                if verbose:
                    print("  RS correction failed")
                continue
            stream = fixed[0]
        end = desc
        while end > 1 and stream[end - 1] == 900:
            end -= 1
        try:
            return interpret(stream[1:end])
        except Exception:
            continue
    return None
