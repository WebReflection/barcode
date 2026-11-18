"""Micro QR probe helpers: zint renders -> module matrices.

Conventions (same as the pdf417 probe): zint BMPs are 2px/module,
rows bottom-up; matrices here are top-down, 1 = dark.
"""

import sys

sys.path.insert(0, "/Users/ag/git/barcode/tools")
from pdf417_probe import raw_rows  # noqa: E402
from pyzint import zint  # noqa: E402

BARCODE_MICROQR = zint.BARCODE_MICROQR


def render(payload, level=None, mask=None, version=None):
    """Top-down module matrix; option_1 = EC level, option_2 = version,
    option_3 = mask (zint Micro QR option semantics)."""
    kw = dict(scale=1, show_text=False, border_width=0, whitespace_width=0)
    if level is not None:
        kw["option_1"] = level
    if version is not None:
        kw["option_2"] = version
    if mask is not None:
        kw["option_3"] = mask
    z = zint.Zint(payload, BARCODE_MICROQR, **kw)
    w, h, get_row = raw_rows(z.render_bmp())
    mw, mh = w // 2, h // 2
    assert mw == mh, (mw, mh)
    return [[1 - get_row(h - 1 - 2 * y)[x * 2] for x in range(mw)] for y in range(mh)]


def dump(m):
    return "\n".join("".join("#" if v else "." for v in row) for row in m)


if __name__ == "__main__":
    for payload in ("A1", "0123456789", "A" * 10):
        m = render(payload)
        print(f"--- {payload!r}: {len(m)}x{len(m[0])} ---")
        print(dump(m))
