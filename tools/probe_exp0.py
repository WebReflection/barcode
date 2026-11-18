"""Which AI payload forms does zint BARCODE_RSS_EXP accept?"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

cands = [
    "(01)95012345678903",
    "(01)9501234567890310",  # wrong len on purpose
    "(10)ABC123",
    "(21)12345",
    "(17)261231(10)ABC123",
    "(01)95012345678903(10)ABC123",
    "(01)95012345678903(21)12345",
    "(3103)000123",
    "0195012345678903",
]
for p in cands:
    try:
        s = svg_module_string(p, zint.BARCODE_RSS_EXP)
        r = runs(s)
        print(f"OK   {p:36s} modules={len(s)} runs={len(r)}")
    except Exception as e:
        print(f"ERR  {p:36s} {e}")
