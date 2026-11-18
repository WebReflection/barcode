"""Validate the DM placement (ISO 16022 annex M algorithm, transcribed
from the spec's description) against libdmtx codeword ground truth."""

import json

POLY = 0x12D


def placement_positions(nrow, ncol):
    """Return a list of (row, col) — the data-region module for each
    successive BIT of the codeword stream (MSB first per codeword).

    Transcribed from the ECC200 module placement (annex M): the codewords
    fill the mapping matrix in upward/downward diagonal sweeps of an
    8-module corner shape, with four special corner cases and the fixed
    two-module pattern in the lower right when it is left untouched.
    """
    array = [0] * (nrow * ncol)  # 0 = unset; marks assigned cells
    positions = []  # positions[bit_index] = (row, col)
    fixed = []  # cells of the fixed '1' pattern, appended at the end

    def module(row, col):
        # wrapping for the shapes that reach past the edges
        if row < 0:
            row += nrow
            col += 4 - (nrow + 4) % 8
        if col < 0:
            col += ncol
            row += 4 - (ncol + 4) % 8
        array[row * ncol + col] = 1
        positions.append((row, col))

    def utah(row, col):
        # the 8-bit corner shape anchored at (row, col)
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

    row, col = 4, 0
    while True:
        if row == nrow and col == 0:
            corner1()
        if row == nrow - 2 and col == 0 and ncol % 4 != 0:
            corner2()
        if row == nrow - 2 and col == 0 and ncol % 8 == 4:
            corner3()
        if row == nrow + 4 and col == 2 and ncol % 8 == 0:
            corner4()
        # sweep upward diagonally
        while True:
            if row < nrow and col >= 0 and array[row * ncol + col] == 0:
                utah(row, col)
            row -= 2
            col += 2
            if not (row >= 0 and col < ncol):
                break
        row += 1
        col += 3
        # sweep downward diagonally
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

    # the fixed pattern: the two '1' modules in the lower right corner
    if array[nrow * ncol - 1] == 0:
        fixed.append((nrow - 1, ncol - 1))
        fixed.append((nrow - 2, ncol - 2))
    return positions, fixed


def read_codewords(matrix, rows, cols):
    """Sample the codeword stream from a symbol matrix using the placement."""
    mr, mc = rows - 2, cols - 2
    positions, fixed = placement_positions(mr, mc)
    n_cw = (mr * mc) // 8
    out = []
    for k in range(n_cw):
        v = 0
        for j in range(8):
            idx = 8 * k + j
            if idx < len(positions):
                r, c = positions[idx]
                b = matrix[1 + r][1 + c]
            elif idx - len(positions) < len(fixed):
                b = 1  # the fixed '1' pattern
            else:
                b = 0
            if b:
                v |= 1 << (7 - j)
        out.append(v)
    return out


if __name__ == "__main__":
    d = json.load(open("/tmp/dm_truth.json"))
    SIZES = ["8x18", "8x32", "10x10", "12x12", "12x26", "14x14", "16x16",
             "16x36", "18x18", "20x20", "22x22", "24x24", "26x26", "12x36",
             "16x48", "32x32", "36x36", "40x40", "44x44", "48x48", "52x52",
             "64x64", "72x72", "80x80", "88x88", "96x96", "104x104",
             "120x120", "132x132"]
    ok_all = True
    for s in SIZES:
        t = d[s]
        got = read_codewords(t["matrix"], t["rows"], t["cols"])
        match = got == t["code"]
        ok_all &= match
        if not match:
            first = next((i for i, (a, b) in enumerate(zip(got, t["code"])) if a != b), None)
            print(f"{s}: MISMATCH at cw {first}: got {got[first]:02x} want {t['code'][first]:02x}"
                  if first is not None else f"{s}: length {len(got)} vs {len(t['code'])}")
        else:
            print(f"{s}: OK")
    print("ALL MATCH" if ok_all else "FAILURES")
