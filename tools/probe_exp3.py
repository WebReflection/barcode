"""DataBar Expanded probe 3: multiple finder occurrences + tails."""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

PAT = "111111110000101"


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


for p in ["[21]1", "[21]1234567890123456789012"[:8], "[01]95012345678903",
          "[01]95012345678903[10]ABC123", "[10]ABCDEFGHIJ", "[17]261231"]:
    try:
        s = m(p)
    except Exception as e:
        print(f"{p:34s} ERR {e}")
        continue
    occ = []
    i = s.find(PAT)
    while i >= 0:
        occ.append(i)
        i = s.find(PAT, i + 1)
    r = runs(s)
    w = [x for _, x in r]
    print(f"{p:34s} modules={len(s):3d} finder_occurrences={occ}")
    print(f"   tail15={s[-15:]}  tail_runs={w[-8:]}")
    print(f"   head20={s[:20]}")
