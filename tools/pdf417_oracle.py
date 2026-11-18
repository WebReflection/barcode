"""Zint-oracle differential harness for the PDF417 high-level encoder.

The black-box oracle: pyzint renders a payload to SVG, we take the
module matrix, and read the codeword stream through OUR OWN validated
decoder (barcode-pdf417/examples/oracle_stream.rs — zint-verified via
tests/golden.rs). Our side of the comparison is the same example
binary reading hex payloads from stdin.

Usage (from packages/web or repo root):

    import sys; sys.path.insert(0, "tools")
    from pdf417_oracle import zint_stream, ours_stream, diff_zoo

Requires: pyzint, cargo (PATH with the rustup toolchain), and the
barcode-pdf417 example built once (cargo build -p barcode-pdf417
--examples --release).

Session history (2026-10-22): this replaced the lost BMP/pattern-table
probe tooling; results banked in docs/STATUS.md "PDF417 NC tie-breaks".
"""

import json
import os
import subprocess
import sys

import pdf417_probe  # tools/pdf417_probe.py: pyzint SVG -> matrix

HERE = os.path.dirname(os.path.abspath(__file__))
EXAMPLE = os.path.join(HERE, "..", "target", "release", "examples", "oracle_stream")
MATRICES = "/tmp/opencode/oracle_matrices.txt"
ENV = {
    "PATH": os.path.expanduser("~/.rustup/toolchains/stable-aarch64-apple-darwin/bin")
    + ":/usr/bin:/bin:/opt/homebrew/bin",
    "HOME": os.path.expanduser("~"),
}


def _run_example(args, stdin=None):
    r = subprocess.run([EXAMPLE, *args], input=stdin, capture_output=True, text=True, env=ENV)
    assert r.returncode == 0, r.stderr[-800:]
    return r.stdout


def _run_cargo(args, stdin=None):
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "barcode-pdf417", "--example", "oracle_stream", *args],
        input=stdin, capture_output=True, text=True, env=ENV,
        cwd=os.path.join(HERE, "..", "crates", "barcode-pdf417"),
    )
    assert r.returncode == 0, r.stderr[-800:]
    return r.stdout


def _cw(line):
    return [int(v) for v in line.split(",")]


def zint_stream(text):
    """zint's data-region codewords for `text` (one payload)."""
    m, _h, _w = pdf417_probe.encode_matrix(text)
    lines = ["SYM t"] + ["".join("#" if v == 0 else "." for v in row) for row in m]
    with open(MATRICES, "w") as f:
        f.write("\n".join(lines) + "\n")
    return _cw(_run_example([MATRICES]).strip().split("\n")[-1])


def ours_stream(payload):
    """Our high_level codewords for `payload` (str or bytes)."""
    h = payload.encode().hex() if isinstance(payload, str) else payload.hex()
    return _cw(_run_cargo([], stdin=h + "\n").strip())


def diff_zoo(payloads):
    """Compare our encoder with zint across a payload list.

    Returns (diffs, stats): diffs = [(payload, zint, ours)] and
    stats = {"n": total, "diff": count}. zint is consulted one
    subprocess per payload (SVG render + one example run each); ours
    is one batched run.
    """
    hexes = [p.encode().hex() for p in payloads]
    ours = [_cw(l) for l in _run_cargo([], stdin="\n".join(hexes) + "\n").strip().split("\n") if l]
    assert len(ours) == len(payloads)
    diffs = []
    for p, o in zip(payloads, ours):
        z = zint_stream(p)
        if o != z:
            diffs.append((p, z, o))
    return diffs, {"n": len(payloads), "diff": len(diffs)}


if __name__ == "__main__":
    # smoke: the payloads whose parity is pinned by the fitted rule
    payloads = ["1", "11", "1234567", "A1", "A1234567", "12345678A", "1é", "11é",
                "é1", "é11", "éA1", "v5Z+Jr", "A1B2C3D4"]
    diffs, stats = diff_zoo(payloads)
    print(json.dumps(stats))
    for p, z, o in diffs:
        print(f"DIFF {p!r}\n  zint: {z}\n  ours: {o}")
