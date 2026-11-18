"""Complete column recovery: many random bases per missing bit."""
import sys, json, random
sys.path.insert(0, "/tmp/opencode")
from bprobe import ecc_of

PREF = 6
cols = {int(k): v for k, v in json.load(open('/tmp/opencode/cols_any.json')).items()}
print("start:", len(cols))

def try_bases(missing, seed=0, iters=300):
    random.seed(seed)
    got = 0
    for it in range(iters):
        if not missing: break
        base = set(random.sample(range(72), random.randint(0, 25)))
        e1 = ecc_of(base)
        if e1 is None: continue
        for j in list(missing):
            e2 = ecc_of(base ^ {j})
            if e2 is not None:
                cols[j] = [x ^ y for x, y in zip(e1[:PREF], e2[:PREF])]
                missing.discard(j); got += 1
    return got

missing = set(range(72)) - set(cols)
for seed in range(6):
    if not missing: break
    got = try_bases(missing, seed)
    print(f"seed {seed}: +{got}, remaining {sorted(missing) if missing else 'DONE'}")

print(f"total {len(cols)}/72")
json.dump({str(k): v for k, v in cols.items()}, open('/tmp/opencode/cols_full.json','w'))
