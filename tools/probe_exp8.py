"""DataBar Expanded probe 8: greedy segmentation.

Model so far:
  [left guard: light1+ink1]
  [check character: 8 elements, 17 modules, responds to all data]
  [START pattern: 15 modules FIXED (1,8,4,1,1) space-first]
  [data characters: 8 elements each, ink-first, variable width?]
  [optional mid-symbol finders?]
  [right guard: light1+ink1]

Greedy segmenter: after the start pattern, repeatedly take 8 runs as a
character, unless the next 5 runs form a finder-shaped window (15 modules,
ratio rule, bar-first parity = reversed start shape).
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

START = ('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1)


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def finderish(r5, expect_first):
    w = [x for _, x in r5]
    if sum(w) != 15:
        return False
    if not (10 <= w[1] + w[2] <= 12):
        return False
    if w[3] + w[4] != 2:
        return False
    return r5[0][0] == expect_first


def segment(p):
    s = m(p)
    r = runs(s)
    print(f"== {p}  mods={len(s)} runs={len(r)}")
    # guard
    if r[0] != ('0', 1) or r[1] != ('1', 1):
        print("   LEFT GUARD MISMATCH", r[:2])
        return
    i = 2
    # check char: 8 elements
    chk = r[i:i+8]
    print(f"   check  @2:     widths={[x for _, x in chk]} mods={sum(x for _, x in chk)}")
    i += 8
    # start pattern
    if tuple(r[i:i+5]) != START:
        print("   START MISMATCH", r[i:i+5])
        return
    i += 5
    # body until last 2 runs
    end = len(r) - 2
    if r[end] != ('0', 1) or r[end + 1] != ('1', 1):
        print("   RIGHT GUARD MISMATCH", r[end:end+2])
        return
    seq = []
    while i < end:
        if i + 5 <= end and finderish(r[i:i+5], '1'):
            seq.append(('F', [x for _, x in r[i:i+5]]))
            i += 5
            continue
        if i + 8 > end:
            seq.append(('?', [x for _, x in r[i:end]]))
            break
        seq.append(('C', [x for _, x in r[i:i+8]]))
        i += 8
    for t, w in seq:
        print(f"   {t} mods={sum(w):3d} {w}")
    print(f"   right guard ok; leftover check: i={i} end={end}")


for p in ["[21]1", "[21]12345", "[01]95012345678903", "[17]261231",
          "[10]ABC", "[10]ABCDEFGHIJ", "[01]95012345678903[10]ABC123",
          "[01]95012345678903[3103]000123"]:
    try:
        segment(p)
    except Exception as e:
        print(f"== {p} ERR {e}")
