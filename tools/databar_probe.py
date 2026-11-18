"""GS1 DataBar oracle probe helpers (zint + zxing-cpp).

Reusable extraction toolkit for the DataBar family. All rendering goes
through zint's SVG (explicit ink rects — the BMP path has a 2px/module
scale plus a polarity trap and burned a whole session; don't use it).

Run with the oracle venv:
    /tmp/opencode/zxenv/bin/python tools/databar_probe.py
"""
import re
import sys

from pyzint import zint

# zint symbology constants for the family:
#   BARCODE_RSS14            -> DataBar Omnidirectional
#   BARCODE_RSS14STACK       -> DataBar Stacked
#   BARCODE_RSS14STACK_OMNI  -> DataBar Stacked Omnidirectional
#   BARCODE_RSS_LTD          -> DataBar Limited
#   BARCODE_RSS_EXP          -> DataBar Expanded
# zxing-cpp formats: DataBarOmni / DataBarStk / DataBarStkOmni /
#   DataBarLtd / DataBarExp / DataBarExpStk


def gtin13_check(d12: str) -> str:
    """Mod-10 check digit for a 12-digit GTIN-13 body (weights 1,3 from left)."""
    s = sum(int(c) * (1 if i % 2 == 0 else 3) for i, c in enumerate(d12))
    return str((10 - s % 10) % 10)


def gtin13(d12: str) -> str:
    """13-digit DataBar input value: 12 data digits + mod-10 check.

    zint validates this check (error 388 'Invalid check digit' otherwise).
    The AI(01) output is this 13-digit value + a freshly computed GTIN-14
    check digit (14 digits total).
    """
    return d12 + gtin13_check(d12)


def svg_module_string(payload: str, mode=zint.BARCODE_RSS14) -> str:
    """Full-width module string (1=ink) at 2px/module, untrimmed."""
    z = zint.Zint(payload, mode, scale=1, show_text=False,
                  border_width=0, whitespace_width=0)
    svg = z.render_svg().decode()
    rects = []
    for attrs in re.findall(r'<rect ([^/]+)/>', svg):
        d = dict(re.findall(r'(\w+)="([^"]*)"', attrs))
        if d.get('fill') == '#FFFFFF':
            continue
        rects.append((float(d['x']), float(d['width'])))
    width = int(max(x + w for x, w in rects))
    mods = []
    for xm in range((width + 1) // 2):
        lo, hi = 2 * xm, 2 * xm + 2
        ink = any(x < hi and x + w > lo for x, w in rects)
        mods.append('1' if ink else '0')
    return ''.join(mods)


def runs(s: str):
    """[(color, width)] runs of a module string; no phantom first run."""
    out = []
    cur, n = s[0], 0
    for c in s:
        if c == cur:
            n += 1
        else:
            out.append((cur, n))
            cur, n = c, 1
    out.append((cur, n))
    return out


def trim(s: str) -> str:
    return s[s.find('1'):s.rfind('1') + 1]


def widths(payload: str, mode=zint.BARCODE_RSS14):
    """Just the run widths (colors strictly alternate, leading paper first)."""
    return tuple(n for _, n in runs(svg_module_string(payload, mode)))


def zxing_decode(payload: str, mode=zint.BARCODE_RSS14):
    """Render with zint, decode with zxing-cpp; returns [(format, text)]."""
    import zxingcpp
    s = svg_module_string(payload, mode)
    h = 16
    buf = bytes((0 if ch == '1' else 255) for _ in range(h) for ch in s)
    img = zxingcpp.ImageView(buf, len(s), h, zxingcpp.ImageFormat.Lum)
    return [(r.format.name, r.text) for r in zxingcpp.read_barcodes(img)]


if __name__ == "__main__":
    for d12 in ("034531200000", "123456789012", "000000000000"):
        g = gtin13(d12)
        w = widths(g)
        print(g, "mods:", sum(w), "runs:", len(w), "widths:", w)
        print("  zxing:", zxing_decode(g))
