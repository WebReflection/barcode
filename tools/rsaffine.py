"""Affine hypothesis: ecc = L(payload bits) XOR C where C = header contribution.
1) Verify C is constant across independent triples.
2) Reconstruct input-CW unit parities q_m from bit probes (grid-0 input)."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of

def payload(setbits):
    b = ['\x00']*9
    for j in setbits: b[j//8] = chr(ord(b[j//8]) | (1 << (7 - j%8)))
    return ''.join(b)

def ecc(bits, want='row-even'):
    name, t = stream_law(payload(bits), 3)
    if name != want: return None
    return cws_of(t, 78, 14)

# 1) constant check over several triples
Cs = []
for base in [(20,30),(40,50),(1,9),(0,7),(33,60)]:
    a, b = base
    ea, eb, eab = ecc([a]), ecc([b]), ecc([a,b])
    if ea and eb and eab:
        Cs.append(tuple(x^y^z for x,y,z in zip(ea,eb,eab)))
print("constants from triples:")
for c in Cs: print("  ", [hex(v) for v in c])
print("all equal:", len(set(Cs)) == 1)

# 2) bit-probe columns for payload bits j (row-even renders)
cols = {}
for j in range(72):
    e = ecc([j])
    if e: cols[j] = e
print(f"{len(cols)}/72 bit probes row-even")
import json
json.dump({str(k): v for k,v in cols.items()}, open('/tmp/opencode/cols.json','w'))
if Cs: json.dump([list(c) for c in Cs[0]], open('/tmp/opencode/Cconst.json','w'))
