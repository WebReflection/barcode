"""Bit-exact check: Rust databar_ltd_runs vs zint BARCODE_RSS_LTD.

Feeds N values through the Rust fixtures dumper (cargo run --example
ltd_dump) and compares every 46-run line against an independent zint
render. Usage: python3 tools/compare_ltd.py [count] [seed]
"""
import random
import subprocess
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

N = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
random.seed(int(sys.argv[2]) if len(sys.argv) > 2 else 9)
values = list(range(0, min(500, N))) + [
    random.randrange(0, 2_000_000_000_000) for _ in range(N - min(500, N))
]
values += [0, 1, 1_999_999_999_999, 2013570, 2013571, 2_013_571 * 993_260 + 2_013_570]
values = sorted(set(v for v in values if v < 2_000_000_000_000))

payload = "\n".join(str(v) for v in values) + "\n"
out = subprocess.run(
    ["cargo", "run", "--quiet", "--example", "ltd_dump", "--manifest-path",
     "/Users/ag/git/barcode/crates/barcode-fixtures/Cargo.toml"],
    input=payload, capture_output=True, text=True, check=True,
).stdout.strip().splitlines()
assert len(out) == len(values), (len(out), len(values))

bad = 0
for v, line in zip(values, out):
    mine = [int(x) for x in line.split(",")]
    theirs = [w for _, w in runs(svg_module_string(f"{v:013d}", zint.BARCODE_RSS_LTD))]
    if mine != theirs:
        bad += 1
        if bad <= 3:
            print(f"MISMATCH v={v}\n  mine : {mine}\n  theirs: {theirs}")
print(f"verification: {len(values) - bad}/{len(values)} bit-exact")
sys.exit(1 if bad else 0)
