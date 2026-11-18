"""DataBar Expanded probe 17: joint check-char fit.

For (5,12)-split check chars: v_check = X + G5, X = 4*V_light_rank + V_ink_rank.
If the check = mod-211 weighted sum S of the data elements, then S - X = G5
must be CONSTANT across all such payloads. Sweep formula variants; the one
with zero variance simultaneously validates the checksum and pins G5.
"""
import sys
import random

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs
from exp_model import rank_of

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))
GL = (('0', 1), ('1', 1))


def m(p):
    return svg_module_string(p, zint.BARCODE_RSS_EXP)


def units(p):
    """(check widths, check pol, [char widths...]) for all 17-mod chars."""
    s = m(p)
    r = runs(s)
    if tuple(r[:2]) != GL or tuple(r[10:15]) != START:
        return None
    chk = [x for _, x in r[2:10]]
    chk_pol = r[2][0]
    i = 15
    end = len(r)
    chars = []
    while i < end:
        rem = end - i
        if rem == 6 and sum(x for _, x in r[i:i + 5]) == 15:
            i += 5
            continue
        if rem < 8:
            break
        c = [x for _, x in r[i:i + 8]]
        if sum(c) != 17:
            return None
        chars.append((c, r[i][0]))
        i += 8
        if end - i >= 5 and sum(x for _, x in r[i:i + 5]) == 15 and (end - i) % 17 != 0:
            i += 5
    return chk, chk_pol, chars


random.seed(23)
payloads = ['[21]' + '12345678901234567890'[:k] for k in [1, 2, 3, 5, 8, 13]]
for _ in range(120):
    k = random.randrange(1, 21)
    payloads.append('[21]' + ''.join(random.choice('0123456789') for _ in range(k)))

rows = []
for p in payloads:
    u = units(p)
    if u is None:
        continue
    chk, chk_pol, chars = u
    ink = chk[0::2] if chk_pol == '1' else chk[1::2]
    light = chk[1::2] if chk_pol == '1' else chk[0::2]
    if (sum(ink), sum(light)) != (5, 12):
        continue
    vl = rank_of(tuple(light[::-1]), 7)   # rev convention (AI-verified)
    vi = rank_of(tuple(ink[::-1]), 2)
    rows.append((p, 4 * vl + vi, chk, chars))

print(f'{len(rows)} payloads with (5,12)-split check char')


def elements(chars, rev):
    out = []
    for w, _pol in chars:
        out.extend(w[::-1] if rev else w)
    return out


def s_of(chars, rev, per_char, start, mod):
    if per_char:
        tot = 0
        for w, _pol in chars:
            ww = w[::-1] if rev else w
            tot += sum(3 ** (i + start) * x for i, x in enumerate(ww))
        return tot % mod
    els = elements(chars, rev)
    return sum(3 ** (i + start) * x for i, x in enumerate(els)) % mod


best = []
for rev in (False, True):
    for per_char in (False, True):
        for start in (0, 1):
            for mod in (211,):
                diffs = []
                for p, X, chk, chars in rows:
                    S = s_of(chars, rev, per_char, start, mod)
                    diffs.append((S - X) % mod)
                spread = max(diffs) - min(diffs)
                nuniq = len(set(diffs))
                best.append((nuniq, spread, rev, per_char, start, mod))
                tag = f"rev={int(rev)} per_char={int(per_char)} start={start}"
                print(f"  {tag} mod{mod}: unique S-X values={nuniq}, "
                      f"spread={spread}, sample={diffs[:6]}")

best.sort()
print("\nBEST:", best[0])
