"""libdmtx decode oracle + zint DM generator, via ctypes.

Test tooling only — never shipped. The clean-room rule is about decoder
CODE: third-party generators/decoders are exactly what we use to
cross-validate ours (same role segno played for QR).

Usage:
    python3 tools/dmtx_oracle.py encode "TEXT" out.png [--rows R] [--cols C] [--scale S]
    python3 tools/dmtx_oracle.py decode image.png [image2.png ...]
"""
import ctypes
import sys
import struct
import zlib


# ---------------------------------------------------------------- libdmtx

_lib = ctypes.CDLL("/opt/homebrew/lib/libdmtx.dylib")

DMTX_SUCCESS = 0
DMTX_PACK_8BPP_K = 300


class DmtxMessage(ctypes.Structure):
    _fields_ = [
        ("arraySize", ctypes.c_size_t),
        ("codeSize", ctypes.c_size_t),
        ("outputSize", ctypes.c_size_t),
        ("outputIdx", ctypes.c_int),
        ("padCount", ctypes.c_int),
        ("fnc1", ctypes.c_int),
        ("array", ctypes.POINTER(ctypes.c_ubyte)),
        ("code", ctypes.POINTER(ctypes.c_ubyte)),
        ("output", ctypes.POINTER(ctypes.c_ubyte)),
    ]


_lib.dmtxImageCreate.restype = ctypes.c_void_p
_lib.dmtxImageCreate.argtypes = [ctypes.POINTER(ctypes.c_ubyte), ctypes.c_int, ctypes.c_int, ctypes.c_int]
_lib.dmtxDecodeCreate.restype = ctypes.c_void_p
_lib.dmtxDecodeCreate.argtypes = [ctypes.c_void_p, ctypes.c_int]
_lib.dmtxRegionFindNext.restype = ctypes.c_void_p
_lib.dmtxRegionFindNext.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
_lib.dmtxDecodeMatrixRegion.restype = ctypes.POINTER(DmtxMessage)
_lib.dmtxDecodeMatrixRegion.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int]
_lib.dmtxRegionDestroy.restype = ctypes.c_int
_lib.dmtxRegionDestroy.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
_lib.dmtxDecodeDestroy.restype = ctypes.c_int
_lib.dmtxDecodeDestroy.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
_lib.dmtxImageDestroy.restype = ctypes.c_int
_lib.dmtxImageDestroy.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
_lib.dmtxMessageDestroy.restype = ctypes.c_int
_lib.dmtxMessageDestroy.argtypes = [ctypes.POINTER(ctypes.POINTER(DmtxMessage))]


def decode_gray(gray, width, height, scale=1):
    """Decode a 8-bit grayscale buffer with libdmtx. Returns list of bytes."""
    buf = (ctypes.c_ubyte * (width * height)).from_buffer_copy(bytes(gray))
    img = _lib.dmtxImageCreate(buf, width, height, DMTX_PACK_8BPP_K)
    assert img, "dmtxImageCreate failed"
    dec = _lib.dmtxDecodeCreate(img, scale)
    assert dec, "dmtxDecodeCreate failed"
    dec_ref = ctypes.c_void_p(dec)
    reg_ref = ctypes.c_void_p()
    out = []
    try:
        while True:
            reg = _lib.dmtxRegionFindNext(dec, None)
            if not reg:
                break
            msg = _lib.dmtxDecodeMatrixRegion(dec, reg, -1)
            if msg:
                m = msg.contents
                out.append(bytes(ctypes.cast(m.output, ctypes.POINTER(ctypes.c_ubyte * m.outputSize)).contents[: m.outputSize]))
                _lib.dmtxMessageDestroy(ctypes.byref(msg))
            _lib.dmtxRegionDestroy(ctypes.byref(ctypes.c_void_p(reg)))
    finally:
        _lib.dmtxDecodeDestroy(ctypes.byref(dec_ref))
        _lib.dmtxImageDestroy(ctypes.byref(ctypes.c_void_p(img)))
    return out


# ------------------------------------------------------------ PNG helpers

def read_gray_png(path):
    """Minimal PNG reader for the grayscale PNGs we write (8-bit, no interlace)."""
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    pos, idat, w, h, depth, ctype = 8, b"", 0, 0, 0, 0
    while pos < len(data):
        (ln,) = struct.unpack(">I", data[pos:pos + 4])
        typ = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + ln]
        if typ == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", body[:10])
            assert depth == 8 and ctype == 0, f"need 8-bit gray, got {depth}/{ctype}"
        elif typ == b"IDAT":
            idat += body
        pos += 12 + ln
    raw = zlib.decompress(idat)
    stride = w
    px = bytearray(w * h)
    prev = bytearray(w)
    pos = 0
    for y in range(h):
        filt = raw[pos]
        pos += 1
        line = bytearray(raw[pos:pos + w])
        pos += w
        if filt == 1:  # sub
            for x in range(1, w):
                line[x] = (line[x] + line[x - 1]) & 0xFF
        elif filt == 2:  # up
            for x in range(w):
                line[x] = (line[x] + prev[x]) & 0xFF
        elif filt == 3:  # avg
            for x in range(w):
                line[x] = (line[x] + ((line[x - 1] if x else 0) + prev[x]) // 2) & 0xFF
        elif filt == 4:  # paeth
            for x in range(w):
                a = line[x - 1] if x else 0
                b = prev[x]
                c = prev[x - 1] if x else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[x] = (line[x] + pr) & 0xFF
        px[y * w:(y + 1) * w] = line
        prev = line
    return px, w, h


def write_gray_png(path, gray, w, h):
    def chunk(typ, body):
        return struct.pack(">I", len(body)) + typ + body + struct.pack(">I", zlib.crc32(typ + body) & 0xFFFFFFFF)

    raw = bytearray()
    for y in range(h):
        raw.append(0)
        raw += gray[y * w:(y + 1) * w]
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 0, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw)))
    png += chunk(b"IEND", b"")
    open(path, "wb").write(png)


def bmp_to_gray(path):
    """Read a zint BMP (1-bit or 24-bit) into (gray, w, h)."""
    data = open(path, "rb").read()
    off = struct.unpack("<I", data[10:14])[0]
    w = struct.unpack("<i", data[18:22])[0]
    h = struct.unpack("<i", data[22:26])[0]
    bpp = struct.unpack("<H", data[28:30])[0]
    gray = bytearray(w * abs(h))
    if bpp == 1:
        row_size = ((w + 31) // 32) * 4
        for yy in range(abs(h)):
            # BMP rows are stored bottom-up when h > 0
            y_img = abs(h) - 1 - yy if h > 0 else yy
            row = data[off + yy * row_size: off + yy * row_size + row_size]
            for x in range(w):
                bit = (row[x >> 3] >> (7 - (x & 7))) & 1
                gray[y_img * w + x] = 0 if bit else 255
        return gray, w, abs(h)
    assert bpp == 24, bpp
    row_size = ((w * 3 + 3) // 4) * 4
    gray = bytearray(w * abs(h))
    for yy in range(abs(h)):
        y_img = yy if h > 0 else abs(h) - 1 - yy
        row = data[off + yy * row_size: off + yy * row_size + w * 3]
        for x in range(w):
            b, g, r = row[x * 3: x * 3 + 3]
            gray[y_img * w + x] = (r * 299 + g * 587 + b * 114) // 1000
    return gray, w, abs(h)


# ------------------------------------------------------------- generation

def encode_matrix(text, rows=0, cols=0, opt2=None):
    """Return (matrix, n_rows, n_cols) — the raw DM bitmap zint produced.
    opt2: explicit zint symbol size index (squares 1-24, DMRE 25-30)."""
    from pyzint import zint
    z = zint.Zint(
        text,
        zint.BARCODE_DATAMATRIX,
        scale=1,
        show_text=False,
        border_width=0,
        whitespace_width=0,
        option_2=opt2 or 0,
    )
    data = z.render_bmp()
    off = struct.unpack("<I", data[10:14])[0]
    w = struct.unpack("<i", data[18:22])[0]
    h = struct.unpack("<i", data[22:26])[0]
    bpp = struct.unpack("<H", data[28:30])[0]
    assert bpp == 1, bpp
    row_size = ((w + 31) // 32) * 4
    # zint draws every module as 2x2 px even at scale 1
    assert w % 2 == 0 and abs(h) % 2 == 0, (w, h)
    mw, mh = w // 2, abs(h) // 2
    matrix = [[0] * mw for _ in range(mh)]
    for yy in range(abs(h)):
        my = yy // 2
        y_img = mh - 1 - my if h > 0 else my
        row = data[off + yy * row_size: off + (yy + 1) * row_size]
        for x in range(mw):
            bit = (row[(x * 2) >> 3] >> (7 - ((x * 2) & 7))) & 1
            # palette: 0 = black
            matrix[y_img][x] = 0 if bit else 1
    return matrix, mh, mw


def render_matrix_png(matrix, out_path, scale=8, quiet=6):
    """Upscale a DM matrix with nearest-neighbour + quiet zone, write PNG."""
    rows, cols = len(matrix), len(matrix[0])
    W = (cols + 2 * quiet) * scale
    H = (rows + 2 * quiet) * scale
    gray = bytearray([255]) * (W * H)
    for y in range(H):
        base = y * W
        my = y // scale - quiet
        if 0 <= my < rows:
            row = matrix[my]
            for x in range(W):
                mx = x // scale - quiet
                if 0 <= mx < cols and row[mx]:
                    gray[base + x] = 0
    write_gray_png(out_path, gray, W, H)
    return gray, W, H


def encode_zint(text, out_path, rows=0, cols=0, scale=8, quiet=6):
    matrix, rows_, cols_ = encode_matrix(text, rows, cols)
    return render_matrix_png(matrix, out_path, scale, quiet)


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "encode":
        text = sys.argv[2]
        out = sys.argv[3]
        rows = cols = 0
        scale = 10
        args = sys.argv[4:]
        for i, a in enumerate(args):
            if a == "--rows":
                rows = int(args[i + 1])
            if a == "--cols":
                cols = int(args[i + 1])
            if a == "--scale":
                scale = int(args[i + 1])
        gray, w, h = encode_zint(text, out, rows, cols, scale)
        print(f"wrote {out} {w}x{h}")
        got = decode_gray(gray, w, h)
        print("oracle decode:", [g.decode() for g in got])
    elif cmd == "decode":
        for path in sys.argv[2:]:
            gray, w, h = read_gray_png(path)
            got = decode_gray(gray, w, h)
            print(f"{path} {w}x{h}:", [g.decode() for g in got])
    else:
        print(__doc__)
