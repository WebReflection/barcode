"""Derive the 144x144 ground truth without libdmtx (which fails on the
6x6 region grid): read the codeword stream from zint's matrix with the
validated placement, then find the RS block split by hypothesis testing.
A wrong (blocks, nn) cannot survive RS consistency over ~2000 codewords.
"""

import json
import sys

sys.path.insert(0, "tools")
exec(open("tools/dmtx_oracle.py").read().split("if __name__")[0])
import dmtx_truth as T
from dmtx_placement_solve import region_grid, sweep, read_back

# ---- GF(256), poly 0x12D, RS generator roots alpha^1..alpha^nn
EXP = [1] * 512
LOG = [0] * 256
for i in range(1, 512):
    EXP[i] = EXP[i - 1] * 2
    if EXP[i] & 0x100:
        EXP[i] ^= 0x12D
for i in range(1, 255):
    LOG[EXP[i]] = i


def gf_mul(a, b):
    if a == 0 or b == 0:
        return 0
    return EXP[LOG[a] + LOG[b]]


def rs_generator(nn):
    """g(x) = prod (x - alpha^i), i=1..nn; coefficients HIGHEST first."""
    g = [1]
    for i in range(1, nn + 1):
        a = EXP[i % 255]
        ng = [0] * (len(g) + 1)
        for j, c in enumerate(g):
            ng[j] ^= c                    # x * term (degree j)
            ng[j + 1] ^= gf_mul(c, a)     # alpha^i * term (degree j+1)
        g = ng
    return g


def rs_encode(data, nn):
    """nn ecc codewords; ecc = (data * x^nn) mod g(x). Highest-first."""
    g = rs_generator(nn)
    msg = list(data) + [0] * nn
    for i in range(len(data)):
        c = msg[i]
        if c:
            for j in range(1, len(g)):
                msg[i + j] ^= gf_mul(g[j], c)
    return msg[-nn:]


def main():
    truth = json.load(open("crates/barcode-dm/testdata/dm_truth.json"))
    t = truth["144x144"] if "144x144" in truth else None
    if t is None:
        zm, R, C = T.encode_matrix("A", 144, 144, 24)
        t = {"text": "A", "rows": R, "cols": C, "matrix": zm}
    m, R, C = t["matrix"], t["rows"], t["cols"]

    grids = region_grid(R - 2, C - 2, 0)  # dims only; bit count checked below
    # geometry says 6x6 of 22 -> map 132x132
    kr, kc, sr, sc = 6, 6, 22, 22
    mr, mc = kr * sr, kc * sc
    n_cells = mr * mc
    print(f"map {mr}x{mc} = {n_cells} cells = {n_cells // 8} codewords"
          f" (remainder {n_cells % 8})")

    def to_sym(r, c):
        return (1 + r + 2 * (r // sr), 1 + c + 2 * (c // sc))

    # placement with pruning disabled (n_bits=0): read whatever is there
    pos = sweep(mr, mc, m, to_sym, [], 0)
    assert len(pos) == n_cells, len(pos)
    stream = []
    for k in range(n_cells // 8):
        v = 0
        for j in range(8):
            r, c = pos[8 * k + j]
            if m[r][c]:
                v |= 1 << (7 - j)
        stream.append(v)
    N = len(stream)
    print(f"stream read: {N} codewords, first: {stream[:8]}")

    # hypothesis test over (blocks, nn)
    hits = []
    for B in range(1, 37):
        for nn in range(1, 120):
            E = B * nn
            D = N - E
            if D < 1 or D < B:
                continue
            data, ecc = stream[:D], stream[D:]
            # deal data round-robin
            blocks = [[] for _ in range(B)]
            for i, v in enumerate(data):
                blocks[i % B].append(v)
            # de-interleave ecc: position i -> block i % B, round i // B
            eblocks = [[] for _ in range(B)]
            for i, v in enumerate(ecc):
                eblocks[i % B].append(v)
            ok = True
            for b in range(B):
                if rs_encode(blocks[b], nn) != eblocks[b]:
                    ok = False
                    break
            if ok:
                hits.append((B, nn, D))
    print("RS-consistent splits:", hits)
    if hits:
        B, nn, D = hits[0]
        data = stream[:D]
        print(f"data starts {data[:6]} (66='A'), then pads: {data[1:13]} ...")
        print(f"total {N} cw = {D} data + {E} ecc, {B} blocks x nn {nn}")


if __name__ == "__main__":
    main()
