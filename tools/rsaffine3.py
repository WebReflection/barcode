"""Column recovery via single-bit flips around row-even bases."""
import sys, random, json
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of

def payload(setbits):
    b = ['\x00']*9
    for j in setbits: b[j//8] = chr(ord(b[j//8]) | (1 << (7 - j%8)))
    return ''.join(b)

def ecc_of(setbits):
    name, t = stream_law(payload(setbits), 3)
    if name != 'row-even': return None
    return cws_of(t, 78, 14)

random.seed(11)
cols = {}
for trial in range(60):
    base = set(random.sample(range(72), random.randint(6, 30)))
    eb = ecc_of(base)
    if eb is None: continue
    got = 0
    for j in range(72):
        if j in cols: continue
        e = ecc_of(base ^ {j})
        if e:
            cols[j] = [x^y for x,y in zip(eb, e)]; got += 1
    print(f"trial {trial}: base size {len(base)}, +{got} cols (total {len(cols)})")
    if len(cols) == 72: break
json.dump({str(k): v for k,v in cols.items()}, open('/tmp/opencode/cols3.json','w'))
print("columns:", len(cols))
