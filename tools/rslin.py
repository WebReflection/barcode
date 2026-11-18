"""Linearity test: ecc(a XOR b) == ecc(a) XOR ecc(b) on trusted bits?
Also collect single-BIT probes (72 payload bits) to recover the linear
map M (48 x 72) for lv3."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of

def payload_with_bit(j):
    # payload bit j (0..71) within stream[6:78]; byte k=j//8, bit pos
    b = ['\x00']*9
    byte = j // 8; bit = 7 - (j % 8)
    b[byte] = chr(1 << bit)
    return ''.join(b)

def get(lv=3, want='row-even'):
    out = {}
    for j in range(72):
        s = payload_with_bit(j)
        name, t = stream_law(s, lv)
        if name == want:
            out[j] = cws_of(t, 78, 14)
    return out

single = get()
print(f"{len(single)}/72 single-bit row-even renders")
# linearity check on available pairs
import itertools
ok = bad = 0
for a, b in itertools.combinations(sorted(single), 2):
    # need a|b render too
    s = payload_with_bit(a)
    bs = list(payload_with_bit(b))
    cs = list(s)
    for k in range(9):
        cs[k] = chr(ord(cs[k]) | ord(bs[k]))
    name, t = stream_law(''.join(cs), 3)
    if name != 'row-even': continue
    ecc_ab = cws_of(t, 78, 6)
    x = single[a]; y = single[b]
    pred = [x[i] ^ y[i] for i in range(6)]
    if pred == ecc_ab: ok += 1
    else: bad += 1
print(f"linearity: {ok} ok, {bad} bad")
