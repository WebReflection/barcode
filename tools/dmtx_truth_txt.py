"""Convert dm_truth.json to a trivial line format the Rust tests parse
without a JSON dependency. Derived artifact — gitignored like the JSON.

Format per size:
    SIZE <rows> <cols> <n_codewords>
    CODE <byte> <byte> ...
    MATRIX <0/1> rows*cols values, row-major
"""

import json
import sys

SRC = "crates/barcode-dm/testdata/dm_truth.json"
DST = "crates/barcode-dm/testdata/dm_truth.txt"

d = json.load(open(SRC))
with open(DST, "w") as f:
    for key in sorted(d, key=lambda k: (d[k]["rows"], d[k]["cols"])):
        t = d[key]
        m = t["matrix"]
        assert len(m) == t["rows"] and len(m[0]) == t["cols"]
        f.write(f"SIZE {t['rows']} {t['cols']} {len(t['code'])}\n")
        f.write("CODE " + " ".join(map(str, t["code"])) + "\n")
        flat = [str(v) for row in m for v in row]
        assert len(flat) == t["rows"] * t["cols"]
        f.write("MATRIX " + " ".join(flat) + "\n")
print(f"wrote {DST} ({len(d)} sizes)")
