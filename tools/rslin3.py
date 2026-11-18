"""Explicit linearity violation dump with FULL 14-CW eccs."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of

def payload(bits):
    b = ['\x00']*9
    for j in bits:
        b[j//8] = chr(ord(b[j//8]) | (1 << (7 - j % 8)))
    return ''.join(b)

store = {}
for bits in ([1], [9], [1, 9], [17], [1, 17], [9, 17]):
    s = payload(bits)
    name, t = stream_law(s, 3)
    tag = ','.join(map(str, bits))
    if name == 'row-even':
        store[tuple(bits)] = cws_of(t, 78, 14)
        print(f"bits {tag:6} ROW-EVEN ecc={[hex(v) for v in store[tuple(bits)][:6]]}")
    else:
        print(f"bits {tag:6} law={name}")

if (1,) in store and (9,) in store and (1,9) in store:
    a, b, ab = store[(1,)], store[(9,)], store[(1,9)]
    x = [a[i]^b[i] for i in range(14)]
    print("xor  :", [hex(v) for v in x[:6]])
    print("actual:", [hex(v) for v in ab[:6]])
