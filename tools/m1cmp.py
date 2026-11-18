"""Compare per-length M1 code structure: systematic cells + parity subsets."""
import json

DATA5 = [tuple(c) for c in json.load(open('/tmp/opencode/m1fit.json'))['DATA']]
G5 = json.load(open('/tmp/opencode/m1fit.json'))['G']
N = len(DATA5)

def dump(name, G, K):
    print(f"--- {name} ---")
    sys_cells = []
    for j in range(K):
        ones = [i for i in range(N) if G[i][j] == 1 and sum(G[i]) == 1]
        sys_cells.append((j, [DATA5[i] for i in ones]))
    for j, cells in sys_cells:
        print(f"  bit{j:2}: {cells}")
    print("  parity:")
    for i in range(N):
        w = sum(G[i])
        if w >= 2:
            print(f"    {DATA5[i]}: bits {[j for j in range(K) if G[i][j]]}")

dump("len5 (3+2)", G5, 17)
L = json.load(open('/tmp/opencode/m1len.json'))
for Lx in ('4', '3', '2', '1'):
    r = L[Lx]
    dump(f"len{Lx} ({r['model']})", r['G'], len(r['G'][0]))
