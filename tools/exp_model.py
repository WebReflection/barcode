"""DataBar Expanded: (17,4) character value machinery.

Model (from session 28 + this session's derivations):
- char = 8 elements (4 ink + 4 light), 17 modules.
- The subset (ink or light) whose sum is ODD is the "odd subset"; the group
  is named (s_odd, s_even).
- Groups: (5,12),(7,10),(9,8),(11,6),(13,4).  (5,12) is pinned: GSUM=0,
  widest (2,7).
- Ranking: reverse the physical element order of each subset, then rank
  lex-ascending among compositions of s into 4 parts <= widest.
- Value: the LARGER-sum subset is slowest:
      VD = V_large_rank * T_small + V_small_rank + G
  where T_small = #compositions of the small subset.
"""
import sys
from functools import lru_cache

sys.path.insert(0, '/Users/ag/git/barcode/tools')


@lru_cache(maxsize=None)
def comp(k, t, w):
    """#compositions of t into k parts in [1,w]."""
    if k == 0:
        return 1 if t == 0 else 0
    if t < k or t > k * w:
        return 0
    return sum(comp(k - 1, t - p, w) for p in range(1, min(w, t) + 1))


@lru_cache(maxsize=None)
def rank_of(vec, w):
    """Lex-ascending rank of composition `vec` (tuple, logical order) among
    comps of sum(vec) into len(vec) parts <= w."""
    s = sum(vec)
    r = 0
    for i, x in enumerate(vec):
        rem_pos = len(vec) - i - 1
        rem_sum = s
        for v in range(1, x):
            if v <= rem_sum:
                r += comp(rem_pos, rem_sum - v, w)
        # subtract consumed
        s -= x
    return r


# widest pairs (wo for odd subset, we for even) with wo + we = 9
WIDEST = {
    (5, 12): (2, 7),
}


def char_value(widths, first_pol):
    """widths: 8 element widths in physical order. first_pol: '0' (light)
    or '1' (ink) polarity of element 0. Returns value or None."""
    ink = widths[0::2] if first_pol == '1' else widths[1::2]
    light = widths[1::2] if first_pol == '1' else widths[0::2]
    si, sl = sum(ink), sum(light)
    if si + sl != 17:
        return None
    # odd-sum subset = odd subset
    if si % 2 == 1:
        s_odd, odd, s_ev, ev = si, ink, sl, light
    else:
        s_odd, odd, s_ev, ev = sl, light, si, ink
    if (s_odd, s_ev) not in WIDEST:
        return None
    wo, we = WIDEST[(s_odd, s_ev)]
    v_odd = rank_of(tuple(reversed(odd)), wo)
    v_ev = rank_of(tuple(reversed(ev)), we)
    # larger-sum subset slowest
    if s_ev > s_odd:
        return v_ev * comp(4, s_odd, wo) + v_odd
    else:
        return v_odd * comp(4, s_ev, we) + v_ev
