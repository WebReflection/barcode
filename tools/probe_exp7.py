"""DataBar Expanded probe 7: exact anatomy dump.

Hypothesis: [guard 2][check 17][start 15][data chars 12 mod each][right guard]
Dump runs with polarity+positions for payloads with clean pair counts and
test the 12-module split mechanically.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def dump(p):
    s = m(p)
    r = runs(s)
    print(f"== {p}  modules={len(s)} runs={len(r)}")
    off = 0
    parts = []
    for pol, w in r:
        parts.append(f"{'1' if pol == '1' else '0'}{w}@{off}")
        off += w
    print("   " + " ".join(parts))
    return r


dump("[01]95012345678903")     # 8 pairs, no FNC1
dump("[21]12345")              # 4 pairs + FNC1?
dump("[21]1")                  # 2 pairs
dump("[17]261231")             # "17"+"261231" = 8 items -> 4 pairs
