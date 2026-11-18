"""zint PDF417 generator probe — structure reconnaissance.

option_1: EC level 1-8? (0 = ?)  option_2/3: rows/cols? — to be pinned.
Matrix via render_svg (explicit ink rects): '#' = ink, 1 module/cell.
"""
import re


def encode_matrix(text, option_1=0, option_2=0, option_3=0):
    from pyzint import zint
    z = zint.Zint(
        text,
        zint.BARCODE_PDF417,
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
    ink = set()
    rects = re.findall(
        r'<rect x="([-\d.]+)" y="([-\d.]+)" width="([-\d.]+)" height="([-\d.]+)"', svg
    )
    for m in rects[1:]:  # rect 0 is the background
        x, y, w, h = (float(v) for v in m)
        for xi in range(int(x // 2), int((x + w) // 2)):
            for yi in range(int(y // 2), int((y + h) // 2)):
                ink.add((xi, yi))
    W = max(p[0] for p in ink) + 1
    H = max(p[1] for p in ink) + 1
    return [[0 if (x, y) in ink else 1 for x in range(W)] for y in range(H)], H, W


if __name__ == "__main__":
    # default geometry for a few payloads
    for text in ["A", "hello world", "https://example.com/pdf417"]:
        m, h, w = encode_matrix(text)
        print(f"{text!r}: {w}x{h}  cols={(w-18)//17-3} rows_band: h%?:", w, h)
