"""Extract Data Matrix ground truth (codewords + placement) from libdmtx
for every symbol size, using zint as the generator. This validates our own
spec-table transcription (sizes, data/ecc split, placement, RS) — the same
role segno's matrix diff played for QR. Test tooling only."""
import ctypes
import json
import sys

exec(open("tools/dmtx_oracle.py").read().split("if __name__")[0])

REGION_LAYOUT = None  # not needed; mapping dims come from the symbol size


# zint option_2 = symbol size index: 1-24 squares, 25-30 DMRE
def size_index(rows, cols):
    squares = [10, 12, 14, 16, 18, 20, 22, 24, 26, 32, 36, 40, 44, 48,
               52, 64, 72, 80, 88, 96, 104, 120, 132, 144]
    if rows == cols:
        return squares.index(rows) + 1
    return {(8, 18): 25, (8, 32): 26, (12, 26): 27, (12, 36): 28,
            (16, 36): 29, (16, 48): 30}[(rows, cols)]


def extract(matrix_text_pair):
    """Given (text, rows, cols): render via zint, read modules ourselves,
    get libdmtx's codeword array + mapping array. Returns dict."""
    text, rows, cols = matrix_text_pair
    zmatrix, mrows, mcols = encode_matrix(text, rows, cols, size_index(rows, cols))
    # flatten to gray buffer at 4px/module with a quiet margin
    S = 4
    Q = 3 * S
    W, H = mcols * S + 2 * Q, mrows * S + 2 * Q
    gray = bytearray([255]) * (W * H)
    for y in range(mrows):
        for x in range(mcols):
            if zmatrix[y][x]:
                for yy in range(S):
                    base = (Q + y * S + yy) * W + Q + x * S
                    for xx in range(S):
                        gray[base + xx] = 0
    # libdmtx decode with the codeword/array extraction
    buf = (ctypes.c_ubyte * (W * H)).from_buffer_copy(bytes(gray))
    img = _lib.dmtxImageCreate(buf, W, H, DMTX_PACK_8BPP_K)
    dec = _lib.dmtxDecodeCreate(img, 1)
    reg = _lib.dmtxRegionFindNext(dec, None)
    assert reg, "no region"
    msg = _lib.dmtxDecodeMatrixRegion(dec, reg, -1)
    assert msg, "no message"
    m = msg.contents
    code = bytes(ctypes.cast(m.code, ctypes.POINTER(ctypes.c_ubyte * m.codeSize)).contents[: m.codeSize])
    arr = bytes(ctypes.cast(m.array, ctypes.POINTER(ctypes.c_ubyte * m.arraySize)).contents[: m.arraySize])
    _lib.dmtxMessageDestroy(ctypes.byref(msg))
    _lib.dmtxRegionDestroy(ctypes.byref(ctypes.c_void_p(reg)))
    _lib.dmtxDecodeDestroy(ctypes.byref(ctypes.c_void_p(dec)))
    _lib.dmtxImageDestroy(ctypes.byref(ctypes.c_void_p(img)))
    return {
        "text": text, "rows": mrows, "cols": mcols,
        "matrix": zmatrix, "code": list(code), "array": list(arr),
    }


SIZES = [
    # (rows, cols) — squares as (n, n), rectangles as DMRE
    (8, 18), (8, 32),
    (10, 10), (12, 12), (12, 26), (14, 14), (16, 16), (16, 36),
    (18, 18), (20, 20), (22, 22), (24, 24), (26, 26),
    (12, 36), (16, 48),
    (32, 32), (36, 36), (40, 40), (44, 44), (48, 48), (52, 52),
    (64, 64), (72, 72), (80, 80), (88, 88), (96, 96), (104, 104),
    (120, 120), (132, 132), (144, 144),
]

if __name__ == "__main__":
    out = {}
    for rows, cols in SIZES:
        try:
            d = extract(("A", rows, cols))
            nz = sum(1 for v in d["array"] if v)
            print(f"{rows}x{cols}: codeSize={len(d['code'])} mapping={d['rows']-2}x{d['cols']-2} array_nz={nz} max={max(d['array'])}")
            out[f"{rows}x{cols}"] = d
        except Exception as e:
            print(f"{rows}x{cols}: FAILED {e}")
    json.dump(out, open("/tmp/dm_truth.json", "w"))
    print("saved /tmp/dm_truth.json")
