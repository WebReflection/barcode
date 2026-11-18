"""Second truth set: payloads that trigger the stuff rule, forced at
skip-carrying sizes (vers 1: skip=2, and vers 13: skip=4) — the exact
collision where the fixed-grid vs sequential stuff models disagree."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from aztec_probe import encode_matrix

CASES = [("é", 1), ("Code 128!", 1), ("x\x7f", 1), ("é", 13), ("Code 128!", 13), ("é", 5), ("é", 2)]
out = []
for text, vers in CASES:
    m, r, c = encode_matrix(text, option_2=vers)
    out.append(f"VERS {vers} {r} {c} {text.encode().decode('unicode_escape') if False else text!r}")
    for row in m:
        out.append("MATRIX " + "".join("." if v == 1 else "#" for v in row))  # probe native 0=ink
    print(f"vers {vers} {text!r}: {r}x{c} ok")
Path("crates/barcode-aztec/testdata/aztec_truth2.txt").write_text("\n".join(out) + "\n")
