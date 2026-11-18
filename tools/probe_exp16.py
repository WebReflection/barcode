"""DataBar Expanded probe 16: check character value + mod-211 formula fit.

For each payload:
  - segment [g2][chk17][S15][chars/blocks...]
  - invert the check char (5,12) machinery -> value V_chk
  - compute candidate checksums over the data chars' element widths
Find the formula with a 100% match.
"""
import sys
import random

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs
from exp_model import char_value, comp

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))
GL = (('0', 1), ('1', 1))


def m(payload):
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def units(p):
    """(check_widths, check_pol, [data char widths], nchars) or None."""
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
        if rem == 6 and sum(x for _, x in r[i:i+5]) == 15:
            i += 5
            continue
        if rem < 8:
            break
        c = [x for _, x in r[i:i+8]]
        if sum(c) != 17:
            return None
        chars.append((c, r[i][0]))
        i += 8
        if end - i >= 5 and sum(x for _, x in r[i:i+5]) == 15 and (end - i) % 17 != 0:
            i += 5
    return chk, chk_pol, chars


random.seed(23)
payloads = []
for k in [1, 2, 3, 5, 8, 13]:
    payloads.append("[21]" + "12345678901234567890"[:k])
for _ in range(60):
    k = random.randrange(1, 21)
    d = "".join(random.choice("0123456789") for _ in range(k))
    payloads.append(f"[21]{d}")

rows = []
for p in payloads:
    u = units(p)
    if u is None:
        continue
    chk, chk_pol, chars = u
    # check char must be (5,12): ink sum 5, light 12 (or via odd-sum rule)
    v = char_value(chk, chk_pol)
    if v is None:
        print("chk not (5,12):", p, chk)
        continue
    rows.append((p, v, chk, chk_pol, chars))

print(f"segmented {len(rows)} payloads; check values range "
      f"{min(r[1] for r in rows)}..{max(r[1] for r in rows)}")

# candidate checksums over data char elements
def elements(chars, rev):
    out = []
    for w, pol in chars:
        ww = w[::-1] if rev else w
        out.extend(ww)
    return out


def try_formulas():
    cands = {}
    for rev in (False, True):
        for per_char in (False, True):
            for base in (3,):
                for start in (0, 1):
                    key = (rev, per_char, base, start)
                    vals = []
                    for p, v, chk, chk_pol, chars in rows:
                        if per_char:
                            tot = 0
                            for w, pol in chars:
                                ww = w[::-1] if rev else w
                                tot += sum((base ** (i + start)) * x
                                           for i, x in enumerate(ww))
                        else:
                            els = elements([(w, pol) for w, pol in chars], rev)
                            tot = sum((base ** (i + start)) * x
                                      for i, x in enumerate(els))
                        vals.append(tot % 211 == v)
                    cands[key] = sum(vals)
    return cands


cands = try_formulas()
for k, hits in sorted(cands.items(), key=lambda kv: -kv[1]):
    print(f"  rev={k[0]} per_char={k[1]} base={k[2]} start={k[1] and k[3] or k[3]}: "
          f"{hits}/{len(rows)}")
