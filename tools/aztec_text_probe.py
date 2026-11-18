#!/usr/bin/env python3
"""
Aztec text-layer probe: compare zint's actual data-region bits against the
spec-expected message (Wikipedia tables, minimal latches, first code masked)
and report the minimal single-bit edit (insert/delete) that reconciles them.

Usage: python3 tools/aztec_text_probe.py
"""
import sys

sys.path.insert(0, "tools")
from aztec_probe import encode_matrix
from aztec_placement_probe import data_path, read_mode


def spec_codes(text, digit_mode):
    """Minimal-latch code sequence per the Wikipedia tables.

    digit_mode: 'wiki' -> digit d has code 2+d; '2d5' -> code 2*d+5 (empirical).
    """
    out = []
    mode = "U"
    for ch in text:
        if ch.isdigit():
            if mode != "D":
                out.append(30)
                mode = "D"
            out.append(2 + int(ch) if digit_mode == "wiki" else 2 * int(ch) + 5)
        elif "a" <= ch <= "z":
            if mode != "L":
                out.append(28)
                mode = "L"
            out.append(2 + ord(ch) - 97)
        elif "A" <= ch <= "Z":
            if mode != "U":
                out.append(14 if mode == "D" else 28)
                mode = "U"
            out.append(2 + ord(ch) - 65)
        else:
            raise ValueError(f"unsupported char {ch!r}")
    return out


def to_bits(codes):
    """5-bit codes MSB-first; the FIRST code is written masked with &7."""
    bits = []
    for i, c in enumerate(codes):
        v = (c & 7) if i == 0 else c
        bits += [(v >> (4 - j)) & 1 for j in range(5)]
    return bits


def pad_to(bits, n):
    """Pad with ones to n bits; all-ones final codeword gets last bit 0."""
    assert len(bits) <= n and n % 6 == 0
    b = bits + [1] * (n - len(bits))
    if all(x == 1 for x in b[n - 6 : n]):
        b[n - 1] = 0
    return b


def edits(spec, obs):
    """All single-bit insertions/deletions turning spec into obs (after pad)."""
    hits = []
    n = len(obs)
    for p in range(len(spec) + 1):
        for v in (0, 1):
            if pad_to(spec[:p] + [v] + spec[p:], n) == obs:
                hits.append(f"insert {v}@{p}")
    for p in range(len(spec)):
        if pad_to(spec[:p] + spec[p + 1 :], n) == obs:
            hits.append(f"delete@{p}")
    return hits


def main():
    payloads = list("0123456789") + ["aa", "42", "a0", "44", "20", "05", "abc", "H1"]
    print(f"{'payload':<8}{'D':<3}{'wiki':<16}{'2d5':<16}")
    for text in payloads:
        m = encode_matrix(text.encode(), option_2=2)[0]
        L, D = read_mode(m, False)
        obs = [1 - m[y][x] for (y, x) in data_path(len(m), True, L)][: D * 6]
        row = f"{text!r:<8}{D:<3}"
        for dm in ("wiki", "2d5"):
            spec = to_bits(spec_codes(text, dm))
            if pad_to(spec, D * 6) == obs:
                row += f"{'verbatim':<16}"
            else:
                hits = edits(spec, obs)
                row += f"{','.join(hits) or 'NO-MATCH':<16}"
        print(row)


if __name__ == "__main__":
    main()
