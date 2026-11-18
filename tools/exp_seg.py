"""Reusable DataBar Expanded segmenter.

Symbol layout: [guard 2][CHECK 17][START 15][c1 17][c2 17][B 15][c3 17][c4 17][B 15]...
Segment into: check widths, then list of (kind, widths) where kind in {'c','B'}.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))
GL = (('0', 1), ('1', 1))


def segments(payload):
    """-> (check_widths, [(kind, widths), ...]) or None on parse failure."""
    s = svg_module_string(payload, zint.BARCODE_RSS_EXP)
    r = runs(s)
    if tuple(r[:2]) != GL or tuple(r[10:15]) != START:
        return None
    chk = [x for _, x in r[2:10]]
    i = 15
    end = len(r)
    out = []
    while i < end:
        rem = end - i
        # a 15-module finder block: 5 runs summing to 15
        if rem >= 5 and sum(x for _, x in r[i:i + 5]) == 15 and (
                rem == 5 or (end - i - 5) % 17 != 0):
            out.append(('B', [x for _, x in r[i:i + 5]]))
            i += 5
            continue
        if rem < 8:
            break
        c = [x for _, x in r[i:i + 8]]
        if sum(c) != 17:
            return None
        out.append(('c', c))
        i += 8
    return chk, out


def chars_of(payload):
    """-> (check_widths, [data char widths...]) with blocks stripped."""
    seg = segments(payload)
    if seg is None:
        return None
    return seg[0], [w for k, w in seg[1] if k == 'c']
