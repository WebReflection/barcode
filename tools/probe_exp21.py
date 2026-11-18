"""Probe 21: pin (9,8) widest via d5-sweep consecutiveness.

c3 = alpha + d5 (consecutive). d5=0..2 in (7,10) [top 3 ranks],
d5=3..9 in (9,8) [ranks 0..6]. Find (wo,we)/conventions making ranks fit.
"""
import sys
from itertools import product

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from exp_seg import chars_of

VEC9 = {  # d5 -> (ink=even-idx, light=odd-idx) for the (9,8) group
    3: ([2, 2, 1, 4], [4, 2, 1, 1]),
    4: ([2, 2, 3, 2], [2, 4, 1, 1]),
    5: ([1, 4, 3, 1], [3, 2, 2, 1]),
    6: ([1, 1, 3, 4], [2, 3, 2, 1]),
    7: ([1, 1, 5, 2], [3, 1, 3, 1]),
    8: ([2, 1, 5, 1], [1, 3, 3, 1]),
    9: ([5, 2, 1, 1], [1, 2, 4, 1]),
}
VEC7 = {
    0: ([4, 1, 1, 1], [1, 2, 4, 3]),
    1: ([1, 2, 3, 1], [2, 3, 1, 4]),
    2: ([2, 1, 1, 3], [2, 1, 3, 4]),
}


def comps(s, w, k=4):
    """compositions of s into k parts each in [1,w], lex ascending."""
    out = []

    def rec(rem, n, cur):
        if n == k:
            if rem == 0:
                out.append(tuple(cur))
            return
        for x in range(1, min(w, rem - (k - n - 1)) + 1):
            cur.append(x)
            rec(rem - x, n + 1, cur)
            cur.pop()
    rec(s, 0, [])
    return out


def rank(vec, w):
    cs = comps(sum(vec), w)
    return cs.index(tuple(vec))


cands = []
for wo in range(1, 9):
    we = 9 - wo
    if wo == we:
        continue
    T9 = len(comps(9, wo))
    T8 = len(comps(8, we))
    if not T9 or not T8:
        continue
    cands.append((wo, we, T9, T8))

print('valid (9,8) widest candidates (wo for 9-side, we for 8-side):')
for wo, we, t9, t8 in cands:
    print(f'  ({wo},{we}): T9={t9} T8={t8} span={t9 * t8}')

print()
for wo, we, t9, t8 in cands:
    for lsb in ('8side', '9side'):
        for rev in (False, True):
            for pol in ('even9', 'odd9'):
                ranks = []
                ok = True
                for d5 in range(3, 10):
                    a, b = VEC9[d5]
                    v9, v8 = (a, b) if pol == 'even9' else (b, a)
                    if rev:
                        v9, v8 = v9[::-1], v8[::-1]
                    try:
                        r9 = rank(v9, wo)
                        r8 = rank(v8, we)
                    except ValueError:
                        ok = False
                        break
                    X = r9 * T8 + r8 if lsb == '8side' else r8 * T9 + r9
                    ranks.append(X)
                if ok and ranks == list(range(7)):
                    print(f'MATCH (9,8) wo={wo} we={we} lsb={lsb} rev={rev} pol={pol}')
                    # check the (7,10) top-3 too
                    print('   9,8 ranks in d5 order:', ranks)
