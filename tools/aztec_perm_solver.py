#!/usr/bin/env python3
"""
Recover zint's true cell order for the vers-2 compact ring by consistency
across many random uppercase payloads (stuffing-free by construction).

For each sample symbol we know the true 240-bit stream (spec message, pad,
RS check). The path reads 240 real cells; the mapping path-index -> true
index is a fixed permutation. Match bit-vectors across samples to solve it.

Usage: python3 tools/aztec_perm_solver.py
"""
import sys
import random

sys.path.insert(0, "tools")
from aztec_probe import encode_matrix
from aztec_placement_probe import data_path, read_mode, rs_encode


def spec_stream(text):
    """True 240-bit stream (message+pad+check) or None if stuffing would fire."""
    bits = []
    for ch in text:
        c = 2 + ord(ch) - 65
        bits += [(c >> (4 - i)) & 1 for i in range(5)]
    D = (len(bits) + 5) // 6
    padded = bits + [1] * (D * 6 - len(bits))
    if all(b == 1 for b in padded[D * 6 - 6 : D * 6]):
        padded[D * 6 - 1] = 0
    for i in range(D):  # spec stuffing trigger: first b-1 bits all same
        w = padded[i * 6 : (i + 1) * 6]
        if all(b == w[0] for b in w[:5]):
            return None
    data = [int("".join(map(str, padded[i * 6 : (i + 1) * 6])), 2) for i in range(D)]
    stream = padded[:]
    for w in rs_encode(data, 40 - D, 6):
        stream += [(w >> (5 - i)) & 1 for i in range(6)]
    return stream


def main():
    random.seed(7)
    payloads = []
    while len(payloads) < 60:
        n = random.randint(1, 6)
        t = "".join(random.choice("ABCDEFGHIJKLMNOPQRSTUVWXYZ") for _ in range(n))
        if t not in payloads:
            payloads.append(t)

    samples = []
    for t in payloads:
        m = encode_matrix(t.encode(), option_2=2)[0]
        L, D = read_mode(m, False)
        st = spec_stream(t)
        if st is None or len(st) != 240:
            continue
        bits = [1 - m[y][x] for (y, x) in data_path(len(m), True, L)]
        samples.append((t, bits, st))
    print(f"samples: {len(samples)}")

    # pack vectors as ints for fast matching
    true_int = []
    for k in range(240):
        v = 0
        for si, (_, _, st) in enumerate(samples):
            v |= st[k] << si
        true_int.append(v)
    path_int = []
    for j in range(240):
        v = 0
        for si, (_, bits, _) in enumerate(samples):
            v |= bits[j] << si
        path_int.append(v)

    lookup = {}
    for k, v in enumerate(true_int):
        lookup.setdefault(v, []).append(k)

    perm = {}
    ambiguous = []
    for j in range(240):
        hits = lookup.get(path_int[j], [])
        if len(hits) == 1:
            perm[j] = hits[0]
        else:
            ambiguous.append((j, hits))
    print(f"solved: {len(perm)}/240, ambiguous: {len(ambiguous)}")
    if ambiguous:
        print("first ambiguous:", ambiguous[:5])

    diffs = [(j, perm[j]) for j in sorted(perm) if perm[j] != j]
    print(f"non-identity mappings: {len(diffs)}")
    for j, k in diffs[:30]:
        print(f"  path[{j}] holds true[{k}]")
    # sanity: is perm a bijection on the solved subset?
    tgt = [k for k in perm.values()]
    print("bijection so far:", len(tgt) == len(set(tgt)))


if __name__ == "__main__":
    main()
