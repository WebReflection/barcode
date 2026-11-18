"""DataBar Expanded probe 1: length sweep + head/tail fingerprints.

Sweep payload lengths and record (modules, runs). Find the minimum symbol,
the per-character increment, and common head/tail patterns.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


print("== length sweep: [21]x k digits ==")
for k in range(1, 25):
    try:
        s = m(f"[21]{'1'*k}")
        r = runs(s)
        # fingerprint: first 8 and last 8 run widths
        w = [x for _, x in r]
        print(f"k={k:2d} modules={len(s):3d} runs={len(r):2d} "
              f"head={w[:6]} tail={w[-6:]}")
    except Exception as e:
        print(f"k={k:2d} ERR {e}")

print("\n== same-length different-digit stability ==")
base = m("[21]12345")
for d in range(10):
    s = m(f"[21]{d}2345" if d else "[21]02345")
    print(f"digit0={d} same_len={len(s)==len(base)}")

print("\n== payload zoo ==")
for p in ["[01]95012345678903", "[17]261231", "[10]ABC", "[10]ABCDEFGHIJ",
          "[01]95012345678903[10]ABC123", "[21]1234567890123456789012"]:
    try:
        s = m(p)
        r = runs(s)
        w = [x for _, x in r]
        print(f"{p:36s} modules={len(s):3d} runs={len(r):2d}")
    except Exception as e:
        print(f"{p:36s} ERR {e}")
