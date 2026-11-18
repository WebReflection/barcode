"""Concrete linearity dump: bits 0,1 (byte0) and their OR; also bits 8,9."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of

def payload(bits):
    b = ['\x00']*9
    for j in bits:
        byte = j // 8; bit = 7 - (j % 8)
        b[byte] = chr(ord(b[byte]) | (1 << bit))
    return ''.join(b)

for bits in ([0], [1], [0,1], [8], [9], [8,9], [0,8]):
    s = payload(bits)
    name, t = stream_law(s, 3)
    tag = ','.join(map(str, bits))
    if t:
        e = cws_of(t, 78, 6)
        print(f"bits {tag:8} law={name:9} ecc[0:6]={[hex(v) for v in e]}")
    else:
        print(f"bits {tag:8}: no law validated")
