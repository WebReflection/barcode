"""Collect FULL 14-byte columns for all 72 bits from ROW-EVEN renders only.
Then extract full Q_m and run deterministic g identification."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode")
from bprobe import ecc_of, payload, stream_law_b, cws_of

cols = {}
pool = {}   # frozenset -> ecc (row-even only)

def collect(S):
    key = frozenset(S)
    if key in pool: return pool[key]
    e = ecc_of(S)
    if e is not None: pool[key] = e
    return pool.get(key)

# seed: single-bit probes + random bases
random.seed(3)
for j in range(72):
    collect([j])
for _ in range(150):
    collect(set(random.sample(range(72), random.randint(1, 25))))
print(f"pool: {len(pool)} row-even renders")

# columns via difference pairs
for trial in range(300):
    if len(cols) == 72: break
    base = set(random.sample(range(72), random.randint(0, 30)))
    eb = collect(base)
    if eb is None: continue
    for j in range(72):
        if j in cols: continue
        e2 = collect(base ^ {j})
        if e2 is not None:
            cols[j] = [x ^ y for x, y in zip(eb, e2)]
print(f"columns: {len(cols)}/72")
json.dump({str(k): v for k, v in cols.items()}, open('/tmp/opencode/cols_deep.json','w'))

# cross-check columns against the 6-byte trusted versions
old = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_full.json')).items()}
agree = sum(1 for j in cols if j in old and cols[j][:6] == old[j][:6])
common = sum(1 for j in cols if j in old)
print(f"trusted-prefix agreement: {agree}/{common}")
