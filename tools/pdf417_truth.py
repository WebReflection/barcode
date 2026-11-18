"""PDF417 truth: zint matrices + hex payloads (option_1=EC, option_2=cols).

Run: /opt/homebrew/bin/python3.14 tools/pdf417_truth.py
"""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from pdf417_probe import encode_matrix

CASES = [
    # (label, text, option_1, option_2)
    ("auto_A", "A", 0, 0),
    ("auto_hello", "hello world", 0, 0),
    ("auto_url", "https://example.com/pdf417", 0, 0),
    ("auto_digits", "0123456789", 0, 0),
    ("auto_bytes", "\x00\x01\xff\xfe", 0, 0),
    ("auto_unicode", "Héllo — ünïcode", 0, 0),
    ("auto_long", "AZTEC-PHYS-001 00385F00030004L51QZWWY0003304960", 0, 0),
    ("ec1", "A", 1, 0),
    ("ec2", "A", 2, 0),
    ("ec3", "A", 3, 0),
    ("ec4", "A", 4, 0),
    ("ec5", "A", 5, 0),
    ("ec6", "A", 6, 0),
    ("ec7", "A", 7, 0),
    ("ec8", "A", 8, 0),
    ("cols1", "hello world", 0, 1),
    ("cols3", "hello world", 0, 3),
    ("cols5", "hello world", 0, 5),
]

out = []
for label, text, o1, o2 in CASES:
    m, h, w = encode_matrix(text, option_1=o1, option_2=o2)
    out.append(f"CASE {label} {o1} {o2} {text.encode().hex()}")
    for row in m:
        out.append("MATRIX " + "".join("#" if v == 0 else "." for v in row))
    print(label, f"{w}x{h}")
Path("crates/barcode-pdf417/testdata/zint_truth.txt").write_text("\n".join(out) + "\n")
