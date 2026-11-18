"""Find hidden func cells in the ecc region: XOR two row-even renders with
the same mask but different data. Format/func cells cancel to 0; parity
bits differ. Consistent zeros across many pairs = func cells."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law
from collections import Counter

def payload(v, pos=0):
    b = ['\x00']*9; b[pos] = chr(v)
    return ''.join(b)

# gather many row-even renders (varying data, same level+mask=row-even)
streams = []
for v in range(1, 256):
    name, t = stream_law(payload(v), 3)
    if name == 'row-even':
        streams.append((v, t))
print(f"{len(streams)} row-even renders for position-0 probes")

zero_votes = Counter(); tot_votes = Counter()
import itertools
pairs = list(itertools.combinations(range(len(streams)), 2))[:400]
for a, b in pairs:
    va, ta = streams[a]; vb, tb = streams[b]
    for i in range(78, 190):
        tot_votes[i] += 1
        if ta[i] == tb[i]: zero_votes[i] += 1
# positions always equal across all pairs
always = [i for i in range(78, 190) if zero_votes[i] == tot_votes[i]]
mostly = [(i, zero_votes[i], tot_votes[i]) for i in range(78,190)
          if zero_votes[i] >= 0.95*tot_votes[i]]
print(f"always-equal positions ({len(always)}):", always)
print("95%+ equal:", mostly[:20])
