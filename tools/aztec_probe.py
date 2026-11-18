"""zint Aztec generator probe — structure reconnaissance.

Banks: layer/size options, bullseye dimensions, reference grid spacing,
mode-message location. Ground truth by construction (payload known).
"""

import struct


def encode_matrix(text, option_1=0, option_2=0, option_3=0):
    """Return (matrix, rows, cols) of the Aztec bitmap zint produced.

    option_1: ECC percent mode (10/23/36/50; 0 = auto)
    option_2: size index 1-36 (1-4 compact 15/19/23/27, 5-36 full 19..151)
    option_3: 1 = full-range only for auto sizing

    Matrix via render_svg (explicit ink rects — no BMP polarity traps):
    '#' = ink. Note zint groups horizontal runs into wide rects.
    """
    from pyzint import zint
    z = zint.Zint(
        text,
        zint.BARCODE_AZTEC,
        scale=1,
        show_text=False,
        border_width=0,
        whitespace_width=0,
        option_1=option_1,
        option_2=option_2,
        option_3=option_3,
    )
    return svg_to_matrix(z.render_svg().decode())


def svg_to_matrix(svg):
    import re
    ink = set()
    for m in re.finditer(
        r'<rect x="([-\d.]+)" y="([-\d.]+)" width="([-\d.]+)" height="([-\d.]+)"', svg
    ):
        x, y, w, h = (float(m.group(i)) for i in (1, 2, 3, 4))
        if x == 0 and y == 0:
            continue  # background
        for xi in range(int(x // 2), int((x + w) // 2)):
            for yi in range(int(y // 2), int((y + h) // 2)):
                ink.add((xi, yi))
    W = max(p[0] for p in ink) + 1
    H = max(p[1] for p in ink) + 1
    return [[0 if (x, y) in ink else 1 for x in range(W)] for y in range(H)], H, W


def bmp_to_matrix(data):
    off = struct.unpack("<I", data[10:14])[0]
    w = struct.unpack("<i", data[18:22])[0]
    h = struct.unpack("<i", data[22:26])[0]
    bpp = struct.unpack("<H", data[28:30])[0]
    assert bpp == 1, bpp
    row_size = ((w + 31) // 32) * 4
    assert w % 2 == 0 and abs(h) % 2 == 0, (w, h)
    mw, mh = w // 2, abs(h) // 2
    matrix = [[0] * mw for _ in range(mh)]
    for yy in range(abs(h)):
        my = yy // 2
        y_img = mh - 1 - my if h > 0 else my
        row = data[off + yy * row_size: off + (yy + 1) * row_size]
        for x in range(mw):
            bit = (row[(x * 2) >> 3] >> (7 - ((x * 2) & 7))) & 1
            matrix[y_img][x] = 0 if bit else 1  # 0 = black
    return matrix, mh, mw


def show(matrix, rows, cols):
    print(f"{rows}x{cols}")
    for r in matrix:
        print("".join("#" if v == 0 else "." for v in r))


if __name__ == "__main__":
    # auto-size, auto-ECC
    m, R, C = encode_matrix(b"AZTEC-TEST-1")
    print("== auto ==")
    show(m, R, C)
