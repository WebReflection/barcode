"""PDF417 Reed-Solomon probe: spec 2.6.3 vs zint oracle streams.

Spec (public domain USS-PDF417, page 11):
- message = length descriptor + data + pads (+ macro block); row
  indicators EXCLUDED
- ecc = complement of remainder of d(x) * x^k / g(x), mod 929
- g(x) = (x - 3)(x - 3^2)...(x - 3^k); k = 2^(level+1) (Table 6)
"""

import sys

sys.path.insert(0, "/Users/ag/git/barcode/tools")
from pdf417_text_probe import data_stream  # noqa: E402


def gen_poly(k):
    """g(x) = prod (x - 3^i), i = 1..k; coefficients highest-first."""
    gen = [1]
    for i in range(1, k + 1):
        r = pow(3, i, 929)
        new = gen + [0]  # multiply by x
        for j in range(len(gen)):
            new[j + 1] = (new[j + 1] - gen[j] * r) % 929
        gen = new
    return gen


def rs_encode(msg, k):
    """ECC codewords for `msg` (descriptor+data+pads) at k ecc codewords."""
    gen = gen_poly(k)[1:]  # drop leading 1 (monic)
    buf = list(msg) + [0] * k
    for i in range(len(msg)):
        c = buf[i] % 929
        if c == 0:
            continue
        for j, g in enumerate(gen, start=1):
            buf[i + j] = (buf[i + j] - c * g) % 929
    return [(-x) % 929 for x in buf[len(msg):]]


def k_for_level(level):
    return 2 ** (level + 1)


def poly_eval(coeffs, x):
    """Horner, highest-degree-first coefficient list."""
    acc = 0
    for c in coeffs:
        acc = (acc * x + c) % 929
    return acc


def syndromes(full, k):
    """S[j] = R(3^(j+1)), j = 0..k-1; all zero <=> undamaged."""
    return [poly_eval(full, pow(3, j + 1, 929)) for j in range(k)]


def berlekamp_massey(S):
    """Minimal connection polynomial C(x) (lowest-first, C[0]=1) with
    sum_i C[i] * S[n-i] == 0 for n >= L (Massey 1969, over GF(p))."""
    C, B = [1], [1]
    L, m, b = 0, 1, 1
    for n in range(len(S)):
        d = S[n]
        for i in range(1, L + 1):
            if i < len(C):
                d = (d + C[i] * S[n - i]) % 929
        if d == 0:
            m += 1
            continue
        coef = d * pow(b, 927, 929) % 929  # d/b via Fermat
        if 2 * L <= n:
            T = C[:]
            C += [0] * (len(B) + m - len(C))
            for i, Bc in enumerate(B):
                C[i + m] = (C[i + m] - coef * Bc) % 929
            L = n + 1 - L
            B, b, m = T, d, 1
        else:
            C += [0] * (len(B) + m - len(C))
            for i, Bc in enumerate(B):
                C[i + m] = (C[i + m] - coef * Bc) % 929
            m += 1
    return C, L


def solve_linear(mat, rhs):
    """Solve mat x = rhs over GF(929); mat n x n. Returns None if singular."""
    n = len(rhs)
    a = [row[:] + [r] for row, r in zip(mat, rhs)]
    for col in range(n):
        piv = next((r for r in range(col, n) if a[r][col] % 929), None)
        if piv is None:
            return None
        a[col], a[piv] = a[piv], a[col]
        inv = pow(a[col][col], 927, 929)
        a[col] = [(v * inv) % 929 for v in a[col]]
        for r in range(n):
            if r != col and a[r][col]:
                f = a[r][col]
                a[r] = [(v - f * w) % 929 for v, w in zip(a[r], a[col])]
    return [a[i][n] for i in range(n)]


def rs_correct(full, k):
    """Correct up to (k-2)//2 errors in the full codeword sequence
    (message ++ ecc, stream order = highest degree first).

    Returns (corrected, [(pos, old, new)]) or None if uncorrectable.
    Erasures may be passed as zeros: they are just errors with unknown
    positions (capacity 2 per slot instead of 1 -- fine for v0).
    """
    n = len(full)
    S = syndromes(full, k)
    if all(s == 0 for s in S):
        return list(full), []

    C, L = berlekamp_massey(S)
    nu = L
    if nu == 0 or nu > (k - 2) // 2:
        return None

    # Chien: error at degree d <=> C(3^(-d)) == 0; 3^928 = 1 mod 929.
    # C is lowest-degree-first, so evaluate reversed.
    degrees = [d for d in range(n)
               if poly_eval(C[::-1], pow(3, 928 - d, 929)) == 0]
    if len(degrees) != nu:
        return None  # locator inconsistent: uncorrectable

    # error values: sum_l e_l * X_l^j = S_j, X_l = 3^d_l, j = 1..nu
    Xs = [pow(3, d, 929) for d in degrees]
    mat = [[X ** j % 929 for X in Xs] for j in range(1, nu + 1)]
    vals = solve_linear(mat, [s % 929 for s in S[:nu]])
    if vals is None:
        return None

    out = list(full)
    fixes = []
    for d, e in zip(degrees, vals):
        pos = n - 1 - d  # stream index of coefficient at degree d
        old = out[pos]
        new = (old - e) % 929  # S_j = sum e_l X_l^j with the error ADDED
        out[pos] = new
        fixes.append((pos, old, new))
    if any(s != 0 for s in syndromes(out, k)):
        return None
    return out, fixes


def check(payload, level, cols=0):
    stream = data_stream(payload, cols=cols, ecc=level)
    desc = stream[0]
    data_pads, ecc = stream[1:desc], stream[desc:]
    k = k_for_level(level)
    mine = rs_encode(stream[:desc], k)  # message INCLUDES the descriptor
    ok = mine == ecc
    print(f"{'OK ' if ok else 'DIFF'} {payload!r:20} level={level} "
          f"rows_x_cols={len(stream)//desc}x? k={k} ecc={ecc[:6]}{'...' if len(ecc) > 6 else ''}")
    if not ok:
        print(f"      mine={mine}")
        print(f"      oracle={ecc}")
    return ok


if __name__ == "__main__":
    # level 0 basics (known from 'A1')
    results = [
        check("A1", 0),
        check("A", 0),
        check("HELLO WORLD 99", 0),  # has a 900 pad -> pads in message
    ]
    # sweep levels on a mid-size payload
    for level in range(1, 9):
        results.append(check("PDF417 RS", level))
    # larger payload, multi-row
    results.append(check("The quick brown fox jumps over the lazy dog", 5))
    n_ok = sum(results)
    print(f"\n{n_ok}/{len(results)} OK", "- ALL VERIFIED" if all(results) else "- FAILURES")
