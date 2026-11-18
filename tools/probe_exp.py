"""DataBar Expanded structure probes, session 1: sizes + run layout.

Oracle rules (established): zint render_svg() at scale=1 (2px/module),
databar_probe.svg_module_string / runs.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs


def exp_runs(payload: str):
    """(module_string, 46+-style run list) for a BARCODE_RSS_EXP payload."""
    s = svg_module_string(payload, zint.BARCODE_RSS_EXP)
    return s, runs(s)


# Payload zoo: pure-digit pairs of every length 1..11, plus an AI mix.
# (01) is always valid; pad pairs are 10s (AI 10 = batch, needs data) —
# simplest valid GS1 payload: (01) + 14 digits (13 + check).
GTIN = "(01)95012345678903"
payloads = []
for n in range(1, 12):
    # n AI(01) pairs: GTIN-14 has 14 digits = 7 pairs... wait, AI(01) fixed
    # length 14 -> 7 pairs. Use (21) serial for shorter: variable length.
    pass

# Simplest: AI(21) with k digits -> k+3 characters? Let's just sweep lengths
# of a numeric-only payload via AI(21) serial (1..20 digits).
rows = []
for k in [1, 2, 3, 4, 6, 8, 10, 12, 14, 16, 18, 20]:
    p = f"(21){'1'*k}"
    try:
        s, r = exp_runs(p)
        rows.append((p, len(s), len(r), r))
    except Exception as e:
        rows.append((p, None, None, str(e)))

for p, L, n, r in rows:
    print(f"{p:28s} modules={L} runs={n}")
    if isinstance(r, list):
        print("   ", r)
    else:
        print("   ERR", r)
