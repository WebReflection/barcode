"""Aztec placement probe: CONFIRMED data spiral reading order.

Established empirically against zint (see docs/STATUS.md AZTEC STEP A4):

PATH (CW spiral, per 2-thick layer ring, outer ring first):
  left strip  rows lo..hi-2 x cols lo,lo+1    : rows T->B, (y,lo),(y,lo+1)
  bottom strip rows hi-1,hi x cols lo..hi-2   : cols L->R, (hi,x),(hi-1,x)
  right strip rows lo+2..hi x cols hi-1,hi    : rows B->T, (y,hi),(y,hi-1)
  top strip  rows lo,lo+1 x cols lo+2..hi     : cols R->L, (lo,x),(lo+1,x)
  (lo,hi = ring bounds; each strip owns its starting corner; every strip
  is the same length; within a 2-cell group the OUTER cell comes first;
  the path ends at (lo,lo+2),(lo+1,lo+2) = just above the TL bullseye
  corner; ring N+1 starts adjacent at its left strip top.)
Grid cells (rows/cols == center mod 16, full symbols only) are skipped.

STREAM: [skip zero cells][D stuffed data cw][K unstuffed check cw]
  D from the mode message; K = T_table - D (T = cap_bits // b);
  the data codewords + skip fill the cells before the check exactly:
  skip = cap - K*b - stuffed_len = cap%b - stuff_count.

Verified: RS-valid decode of zint symbols at vers 1,2,3,4,5,6,8,9,13,27.
"""

import sys

sys.path.insert(0, "tools")
from aztec_probe import encode_matrix


# ---------------------------------------------------------------- GF/RS
def gf_mul(a, b, bits, poly):
    r = 0
    while b:
        if b & 1:
            r ^= a
        b >>= 1
        a <<= 1
        if a & (1 << bits):
            a ^= poly
    return r


POLY = {4: 0x13, 6: 0x43, 8: 0x12D, 10: 0x409, 12: 0x1069}


def gen(ncheck, bits):
    g = [1]
    for i in range(1, ncheck + 1):
        a = 1
        for _ in range(i):
            a = gf_mul(a, 2, bits, POLY[bits])
        ng = [0] * (len(g) + 1)
        for j, c in enumerate(g):
            ng[j] ^= c
            ng[j + 1] ^= gf_mul(a, c, bits, POLY[bits])
        g = ng
    return g


def rs_encode(data, ncheck, bits):
    g = gen(ncheck, bits)
    msg = list(data) + [0] * ncheck
    for i in range(len(data)):
        fb = msg[i]
        if fb:
            for j in range(1, len(g)):
                msg[i + j] ^= gf_mul(g[j], fb, bits, POLY[bits])
    return msg[len(data):]


# ------------------------------------------------------------- mode word
G5, G6 = gen(5, 4), gen(6, 4)


def rs_ok(nibs, ncheck, g):
    msg = list(nibs) + [0] * ncheck
    for i in range(len(nibs) - ncheck):
        fb = msg[i]
        if fb:
            for j in range(1, len(g)):
                msg[i + j] ^= gf_mul(g[j], fb, 4, POLY[4])
    return all(x == 0 for x in msg[len(nibs) - ncheck:len(nibs)])


def read_mode(m, full):
    """(layers, data_codewords) from the core ring. See STATUS.md A3."""
    R = len(m)
    c = R // 2
    core_half = 7 if full else 5
    lo, hi = c - core_half, c + core_half
    ring = [(lo, x) for x in range(lo, hi + 1)] + [(y, hi) for y in range(lo + 1, hi + 1)] + \
           [(hi, x) for x in range(hi - 1, lo - 1, -1)] + [(y, lo) for y in range(hi - 1, lo, -1)]
    if full:
        sides = [list(range(k + 2, k + 7)) + list(range(k + 8, k + 13)) for k in (0, 14, 28, 42)]
        ncheck, g, lw, ld = 6, G6, 4, 11
    else:
        sides = [list(range(k + 2, k + 9)) for k in (0, 10, 20, 30)]
        ncheck, g, lw, ld = 5, G5, 2, 6
    bits = []
    for side in sides:
        bits += [1 - m[y][x] for (y, x) in (ring[i] for i in side)]
    nibs = [int(''.join(map(str, bits[i * 4:(i + 1) * 4])), 2) for i in range(len(bits) // 4)]
    assert rs_ok(nibs, ncheck, g), f"mode RS invalid {nibs}"
    mw = 0
    for nib in nibs[:lw]:
        mw = (mw << 4) | nib
    return (mw >> ld) + 1, (mw & ((1 << ld) - 1)) + 1


# ------------------------------------------------------------------ path
def data_path(R, compact, layers):
    """Confirmed cell visit order (see module docstring).

    Rings live in the UNSHIFTED (15+4L)-square frame; grid rows/cols
    (final coords congruent to the center mod 16, except the center) are
    inserted between unshifted rows at distance 16k and 16k-1, pushing
    outer layers outward. Cells are mapped to final coords; those landing
    on grid rows/cols are skipped (all dashes).
    """
    u_size = (11 if compact else 15) + 4 * layers
    u_c = u_size // 2
    c = R // 2
    inserts = sorted(set(range(c % 16, R, 16)) - {c}) if not compact else []

    def keep(y, x):
        return compact or (y % 16 != c % 16 and x % 16 != c % 16)

    def M(u):
        s = u
        for ins in inserts:
            if s >= ins:
                s += 1
        return s

    cells = []
    for i in range(layers):
        half = u_c - 2 * i
        lo, hi = u_c - half, u_c + half
        for y in range(lo, hi - 1):          # left strip, rows T->B
            for x in (lo, lo + 1):
                if keep(M(y), M(x)):
                    cells.append((M(y), M(x)))
        for x in range(lo, hi - 1):          # bottom strip, cols L->R
            for y in (hi, hi - 1):
                if keep(M(y), M(x)):
                    cells.append((M(y), M(x)))
        for y in range(hi, lo + 1, -1):      # right strip, rows B->T
            for x in (hi, hi - 1):
                if keep(M(y), M(x)):
                    cells.append((M(y), M(x)))
        for x in range(hi, lo + 1, -1):      # top strip, cols R->L
            for y in (lo, lo + 1):
                if keep(M(y), M(x)):
                    cells.append((M(y), M(x)))
    return cells


def bits_by_layers(layers):
    return 6 if layers <= 2 else 8 if layers <= 8 else 10 if layers <= 22 else 12


def cap_bits(layers, compact):
    l = layers
    return (88 + 16 * l) * l if compact else (112 + 16 * l) * l


# --------------------------------------------------------------- decoding
def parse_data(bits, pos, b, n_cw):
    """Parse n_cw stuffed codewords from bits[pos:] -> (values, end_pos) or None.

    Codewords 0..n-2 are bit-stuffed (a complementary bit inserted after
    b-1 identical leading bits). The FINAL codeword carries the padding
    and is written raw (never stuffed) — see STATUS.md A4.
    """
    out = []
    for i in range(n_cw):
        if pos + b > len(bits):
            return None
        run = bits[pos:pos + b - 1]
        pos += b - 1
        if i < n_cw - 1 and len(set(run)) == 1:
            if pos >= len(bits) or bits[pos] == run[0]:
                return None  # stuff bit must be complementary
            pos += 1
        if pos >= len(bits):
            return None
        out.append(int(''.join(map(str, run + [bits[pos]])), 2))
        pos += 1
    return out, pos


def decode_stream(m, compact, layers):
    """Full stream decode -> (data, check, skip, K) or None."""
    R = len(m)
    b = bits_by_layers(layers)
    cap = cap_bits(layers, compact)
    path = data_path(R, compact, layers)
    assert len(path) == cap, f"path {len(path)} != cap {cap}"
    bits = [1 - m[y][x] for (y, x) in path]
    L, D = read_mode(m, not compact)
    assert L == layers
    T = cap // b
    K = T - D
    check_start = cap - K * b
    for skip in range(cap % b + 1):
        parsed = parse_data(bits, skip, b, D)
        if parsed and parsed[1] == check_start:
            data, pos = parsed
            check = [int(''.join(map(str, bits[pos + i * b:pos + (i + 1) * b])), 2)
                     for i in range(K)]
            if rs_encode(data, K, b) == check:
                return data, check, skip, K
    return None


CASES = [
    # (vers, compact, layers, payloads)
    (1, True, 1, [b"x", b"AZTEC", b"hello world", b"B" * 16, b"Hello, world!"]),
    (2, True, 2, [b"x", b"hello world", b"B" * 16]),
    (3, True, 3, [b"hello world", b"B" * 30]),
    (4, True, 4, [b"hello world"]),
    (5, False, 1, [b"x", b"hello world", b"B" * 16]),
    (6, False, 2, [b"hello world"]),
    (8, False, 4, [b"hello world", b"B" * 30]),
    (9, False, 5, [b"hello world"]),   # first size with grid in the data region
    (13, False, 9, [b"hello world"]),  # b=10
    (27, False, 23, [b"hello world"]), # b=12
]


def main():
    for vers, compact, layers, payloads in CASES:
        line = []
        for p in payloads:
            m, R, C = encode_matrix(p, option_2=vers)
            r = decode_stream(m, compact, layers)
            if r is None:
                line.append(f"{p[:6]!r}:FAIL")
            else:
                data, check, skip, K = r
                line.append(f"{p[:6]!r}:OK skip={skip} K={K}")
        print(f"vers {vers:2d} ({'compact' if compact else 'full   '} L{layers:2d}): "
              + "  ".join(line))


if __name__ == "__main__":
    main()
