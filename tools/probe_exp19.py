"""Probe 19: definitive digit->char map + AI independence.

1. Digit sweep: [21]12345, vary each digit position over 0-9.
   Which char slots change? (chars indexed from first data-ish char = CHECK)
2. AI independence: [10]12345 vs [21]12345 - do non-AI chars match?
3. Value vs pair packing: [21]12 vs [21]02 etc.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from exp_seg import chars_of

base = '12345'
print('=== 1. digit sweep: which char slots change ===')
slotdata = {}
for pos in range(5):
    seen = {}
    for d in '0123456789':
        p = f'[21]{base[:pos]}{d}{base[pos+1:]}'
        u = chars_of(p)
        assert u, p
        seen[d] = u
    slotdata[pos] = seen

nchars = len(slotdata[0]['0'][1])
print(f'char count n={nchars + 1} (check + {nchars} data)')
for pos in range(5):
    ref = slotdata[pos]['5']
    changing = []
    for ci in range(nchars + 1):
        vals = set()
        for d in '0123456789':
            u = slotdata[pos][d]
            w = tuple(u[0]) if ci == 0 else tuple(u[1][ci - 1])
            vals.add(w)
        if len(vals) > 1:
            changing.append(ci)
    print(f'  d{pos + 1} varies -> chars(changed): {changing}')
    for ci in changing:
        if ci == 0:
            print(f'    check vectors: ' + ' '.join(str(slotdata[pos][d][0]) for d in '0123456789'))

print()
print('=== 2. AI independence: [10] vs [21] vs [37], data 12345 ===')
for ai in ('10', '21', '37', '90', '99'):
    u = chars_of(f'[{ai}]12345')
    print(f'  [{ai}]: check={u[0]}')
    for i, w in enumerate(u[1]):
        print(f'    c{i + 1}={w}')

print()
print('=== 3. same-length data, leading zeros: [21]12 vs [21]02; [21]12345 vs [21]02345 ===')
for a, b in (('[21]12', '[21]02'), ('[21]12345', '[21]02345'), ('[21]12345', '[21]10345')):
    ua, ub = chars_of(a), chars_of(b)
    same = ua[0] == ub[0]
    samec = ua[1] == ub[1]
    print(f'  {a} vs {b}: check same={same}, data chars same={samec}')
    if not samec:
        for i, (wa, wb) in enumerate(zip(ua[1], ub[1])):
            if wa != wb:
                print(f'    c{i + 1}: {wa} vs {wb}')
