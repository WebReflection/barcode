"""Brute-force the DM placement rules against the banked libdmtx ground
truth — v3, with the CONCATENATED mapping matrix.

Model (validated empirically, see STATUS.md):
  - The inner area ((rows-2)x(cols-2)) is tiled with s x s data regions
    separated by 2-module lines (clock + solid L). Rectangles use 1x2
    region grids (8x32 = two 6x14, 12x36/16x36 = two Wx16, 16x48).
  - The PLACEMENT runs on the concatenation of the data regions
    (separators removed): dims mr = kr*s, mc = kc*s. This is forced by
    codeword alignment: a 14x14 region holds 24.5 codewords, which is
    impossible; the concatenated 28x28 holds exactly 98.
  - Rendering maps mapping-matrix (r,c) to symbol
    (1 + r + 2*(r//s), 1 + c + 2*(c//s)).
  - The annex M diagonal sweep (+ 4 corner cases + the fixed pattern)
    applies to the concatenated matrix with its full dims.
"""

import json
import itertools
import sys

TRUTH = "crates/barcode-dm/testdata/dm_truth.json"


def region_grid(ir, ic, n_bits):
    """Find (kr, kc, s) with kr x kc regions of s x s (squares) or
    (kr, kc, sr, sc) generally, leftover = cells - bits in 0..4."""
    sols = []
    for kr in range(1, 9):
        if (ir - 2 * (kr - 1)) <= 0 or (ir - 2 * (kr - 1)) % kr:
            continue
        sr = (ir - 2 * (kr - 1)) // kr
        for kc in range(1, 9):
            if (ic - 2 * (kc - 1)) <= 0 or (ic - 2 * (kc - 1)) % kc:
                continue
            sc = (ic - 2 * (kc - 1)) // kc
            cells = kr * kc * sr * sc
            if cells >= n_bits and cells - n_bits in (0, 2, 4):
                sols.append((kr, kc, sr, sc, cells - n_bits))
    return sols


def sweep(nrow, ncol, matrix, to_sym, bits, n_bits,
          shift_r=None, shift_c=None, corner_overrides=None):
    """Annex M diagonal sweep on the concatenated mapping matrix.
    to_sym(r, c) -> symbol coords. Returns positions or None."""
    if shift_r is None:
        shift_r = 4 - (nrow + 4) % 8
    if shift_c is None:
        shift_c = 4 - (ncol + 4) % 8
    co = corner_overrides or {}
    array = [0] * (nrow * ncol)
    positions = []

    def module(row, col):
        if row < 0:
            row += nrow
            col += shift_r
        if col < 0:
            col += ncol
            row += shift_c
        if not (0 <= row < nrow and 0 <= col < ncol):
            raise IndexError((row, col))
        array[row * ncol + col] = 1
        positions.append(to_sym(row, col))
        if len(positions) <= n_bits and \
                matrix[positions[-1][0]][positions[-1][1]] != bits[len(positions) - 1]:
            raise ValueError

    def utah(row, col):
        for dr, dc in [(-2, -2), (-2, -1), (-1, -2), (-1, -1),
                       (-1, 0), (0, -2), (0, -1), (0, 0)]:
            module(row + dr, col + dc)

    def corner1():
        for r, c in [(nrow - 1, 0), (nrow - 1, 1), (nrow - 1, 2),
                     (0, ncol - 2), (0, ncol - 1), (1, ncol - 1),
                     (2, ncol - 1), (3, ncol - 1)]:
            module(r, c)

    def corner2():
        for r, c in [(nrow - 3, 0), (nrow - 2, 0), (nrow - 1, 0),
                     (0, ncol - 4), (0, ncol - 3), (0, ncol - 2),
                     (0, ncol - 1), (1, ncol - 1)]:
            module(r, c)

    def corner3():
        for r, c in [(nrow - 3, 0), (nrow - 2, 0), (nrow - 1, 0),
                     (0, ncol - 2), (0, ncol - 1), (1, ncol - 1),
                     (2, ncol - 1), (3, ncol - 1)]:
            module(r, c)

    def corner4():
        for r, c in [(nrow - 1, 0), (nrow - 1, ncol - 1), (0, ncol - 3),
                     (0, ncol - 2), (0, ncol - 1), (1, ncol - 3),
                     (1, ncol - 2), (1, ncol - 1)]:
            module(r, c)

    corners = {"c1": corner1, "c2": corner2, "c3": corner3, "c4": corner4}

    row, col = 4, 0
    passes = 0
    while True:
        passes += 1
        if passes > 400:
            raise RuntimeError("no termination")
        if row == nrow and col == 0 and co.get("c1", True):
            corner1()
        if row == nrow - 2 and col == 0 and ncol % 4 != 0 and co.get("c2", True):
            corner2()
        if row == nrow - 2 and col == 0 and ncol % 8 == 4 and co.get("c3", True):
            corner3()
        if row == nrow + 4 and col == 2 and ncol % 8 == 0 and co.get("c4", True):
            corner4()
        while True:
            if row < nrow and col >= 0 and array[row * ncol + col] == 0:
                utah(row, col)
            row -= 2
            col += 2
            if not (row >= 0 and col < ncol):
                break
        row += 1
        col += 3
        while True:
            if row >= 0 and col < ncol and array[row * ncol + col] == 0:
                utah(row, col)
            row += 2
            col -= 2
            if not (row < nrow and col >= 0):
                break
        row += 3
        col += 1
        if not (row < nrow or col < ncol):
            break
    return positions


def read_back(matrix, positions, n_cw):
    out = []
    for k in range(n_cw):
        v = 0
        for j in range(8):
            r, c = positions[8 * k + j]
            if matrix[r][c]:
                v |= 1 << (7 - j)
        out.append(v)
    return out


def main():
    only = sys.argv[1:] or None
    d = json.load(open(TRUTH))
    ok_all = True
    for key in sorted(d, key=lambda k: (d[k]["rows"], d[k]["cols"])):
        if only and key not in only:
            continue
        t = d[key]
        R, C, code = t["rows"], t["cols"], t["code"]
        m = t["matrix"]
        n_bits = 8 * len(code)
        grids = region_grid(R - 2, C - 2, n_bits)
        if len(grids) != 1:
            print(f"{key}: region grids {grids}")
            ok_all = False
            continue
        kr, kc, sr, sc, leftover = grids[0]
        mr, mc = kr * sr, kc * sc

        def to_sym(r, c, kr=kr, kc=kc, sr=sr, sc=sc):
            return (1 + r + 2 * (r // sr), 1 + c + 2 * (c // sc))

        bits = []
        for v in code:
            bits += [(v >> (7 - j)) & 1 for j in range(8)]

        found = None
        try:
            pos = sweep(mr, mc, m, to_sym, bits, n_bits)
            if read_back(m, pos, len(code)) == code:
                found = "formula"
        except (ValueError, IndexError, RuntimeError):
            pass
        if not found:
            # fallback: enumerate wrap shifts
            for shr, shc in itertools.product(range(-8, 9), repeat=2):
                try:
                    pos = sweep(mr, mc, m, to_sym, bits, n_bits,
                                shift_r=shr, shift_c=shc)
                except (ValueError, IndexError, RuntimeError):
                    continue
                if read_back(m, pos, len(code)) == code:
                    found = f"shifts r{shr:+d} c{shc:+d}"
                    break
        if found:
            print(f"{key}: map {mr}x{mc} (grid {kr}x{kc} of {sr}x{sc},"
                  f" leftover {leftover}) -> OK [{found}]")
        else:
            print(f"{key}: map {mr}x{mc} -> NO SOLUTION")
            ok_all = False
    print("ALL MATCH" if ok_all else "FAILURES")


if __name__ == "__main__":
    main()
