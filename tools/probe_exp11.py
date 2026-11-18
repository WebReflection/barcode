"""DataBar Expanded probe 11: mechanical anatomy extraction.

Model: [guard (0,1)(1,1)][check: 8 el, 17 mods][start: 5 el, 15 mods,
(1,8,4,1,1)] then repeating [char 8el][char 8el][block 5el 15mods],
ending with [char 8el][guard (0,1)(1,1)] and/or a trailing block.

Verify on [21]+k digits for k=1..20; report char widths, block widths,
and the tail shape. Also dump c1/c2/check widths.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))
GL = (('0', 1), ('1', 1))
GR = (('0', 1), ('1', 1))


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def anatomy(p):
    s = m(p)
    r = runs(s)
    if tuple(r[:2]) != GL or tuple(r[-2:]) != GR or tuple(r[10:15]) != START:
        return None
    chk = [x for _, x in r[2:10]]
    if sum(chk) != 17:
        return ("chkwidth", sum(chk))
    i = 15
    end = len(r) - 2
    chars = []
    blocks = []
    while i < end:
        # char: 8 elements
        c = [x for _, x in r[i:i+8]]
        if sum(c) != 17:
            return ("badchar", i, c)
        chars.append(c)
        i += 8
        if i >= end:
            break
        # block: 5 elements 15 modules?
        b = [x for _, x in r[i:i+5]]
        if sum(b) == 15 and len(b) == 5:
            blocks.append(b)
            i += 5
    return dict(mods=len(s), chars=chars, blocks=blocks,
                tail=r[end:], n=len(chars))


for k in range(1, 21):
    p = f"[21]{'1'*k}"
    try:
        a = anatomy(p)
    except Exception as e:
        a = f"ERR {e}"
    if a is None:
        print(f"k={k:2d}: anatomy mismatch")
    elif isinstance(a, tuple):
        print(f"k={k:2d}: {a}")
    else:
        ws = [sum(c) for c in a['chars']]
        bs = [sum(b) for b in a['blocks']]
        print(f"k={k:2d} mods={a['mods']:3d} nchars={a['n']} charw={ws} blockw={bs} tail={[list(x) for x in a['tail']]}")
