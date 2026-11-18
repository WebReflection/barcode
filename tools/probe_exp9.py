"""DataBar Expanded probe 9: backtracking segmentation.

Known-good prefix: [guard ('0',1),('1',1)][check: 8 elements][start:
('0',1),('1',8),('0',4),('1',1),('0',1)]. After that: characters (8
elements, variable width), finder-shaped 5-element 15-module blocks
(either parity), and a right guard ('0',1),('1',1).

Enumerate ALL parses consistent with: every char has exactly 8 elements;
every 5-element block has 15 modules; the parse consumes all runs and ends
with the right guard. Report the multiset of parses.
"""
import sys
from functools import lru_cache

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))
GUARD_L = (('0', 1), ('1', 1))
GUARD_R = (('0', 1), ('1', 1))


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def parses(r):
    """All ways to split r[2:-2] (between guards) into 8-el chars and
    5-el 15-module blocks, given the check+start consumed first."""
    body = r[10:-2]  # after guard(2) + check(8) + start(5)
    n = len(body)

    @lru_cache(maxsize=None)
    def go(i):
        if i == n:
            return [()]
        out = []
        # char: 8 elements
        if i + 8 <= n:
            w = tuple(x for _, x in body[i:i+8])
            for rest in go(i + 8):
                out.append((('C', w),) + rest)
        # finder-shaped block: 5 elements, 15 modules
        if i + 5 <= n:
            w = tuple(x for _, x in body[i:i+5])
            if sum(w) == 15 and 10 <= w[1] + w[2] <= 12 and w[3] + w[4] == 2:
                for rest in go(i + 5):
                    out.append((('F', w),) + rest)
        return out

    return go(0)


def analyze(p):
    s = m(p)
    r = runs(s)
    ok_prefix = tuple(r[:2]) == GUARD_L and tuple(r[-2:]) == GUARD_R \
        and tuple(r[10:15]) == START
    ps = parses(r)
    print(f"== {p:36s} mods={len(s):3d} runs={len(r):2d} prefix_ok={ok_prefix} parses={len(ps)}")
    seen = set()
    for pr in ps[:6]:
        key = tuple(pr)
        if key in seen:
            continue
        seen.add(key)
        desc = " ".join(f"{t}{sum(w)}" if t == 'C' else f"F{w}" for t, w in pr)
        print(f"    {desc}")


for p in ["[21]1", "[21]12345", "[01]95012345678903", "[17]261231",
          "[10]ABC", "[10]ABCDEFGHIJ", "[01]95012345678903[10]ABC123",
          "[01]95012345678903[3103]000123", "[10]ABCDEFGHIJ[21]12345"]:
    try:
        analyze(p)
    except Exception as e:
        print(f"== {p} ERR {e}")
