"""Generate crates/barcode-aztec/testdata/aztec_truth.txt — zint Aztec
matrices for 'A' at every size index (1-36), EC 23% (zint's default
percent), plus the auto-mode choice for 'A'. Test tooling only."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from aztec_probe import encode_matrix

out = []
for vers in range(1, 37):
    try:
        m, r, c = encode_matrix("A", option_2=vers)
    except Exception as e:
        print(f"vers {vers}: FAIL {e}")
        continue
    out.append(f"VERS {vers} {r} {c}")
    for row in m:
        out.append("MATRIX " + "".join("." if v == 1 else "#" for v in row))  # probe native 0=ink -> invert

# auto mode: which size/EC does zint pick for 'A'?
for text in ["A", "hello world", "https://example.com/aztec"]:
    m, r, c = encode_matrix(text)
    out.append(f"AUTO {r} {c} {text!r}")

Path("crates/barcode-aztec/testdata").mkdir(exist_ok=True)
Path("crates/barcode-aztec/testdata/aztec_truth.txt").write_text("\n".join(out) + "\n")
print("entries:", sum(1 for l in out if l.startswith('VERS')))
for l in out:
    if l.startswith('AUTO'):
        print(l)
