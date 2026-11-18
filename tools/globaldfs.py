"""Global function-cell DFS.

One shared is_func[] over PATH cells (geometry is format-independent).
Per payload: expected stream E_p and a mask chosen from candidates (fixed
per payload, but unknown -> branch at cell 0 over masks that match the
header bits, which live in the first data cells).
DFS cells in order; at each cell try DATA (all active payloads' bits must
match their E) then FUNCTION (skip everywhere). Backtrack on contradiction.
Objective: complete all payloads with fewest function cells.
"""
import sys
sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render
from funcsolve import expected_stream, MASKS, PATH

def bits_of(m, f):
    return [m[y][x] ^ (1 if f(x, y) else 0) for x, y in PATH]

payloads = ['abcdefghi','abcdefgh','abcdefg','abcdef','!!!!!!!','!!!!!',
            '~~~!!~~!','[[]][][]','xyzxyzxy','{{{{{{{',
            '1234567890','12345678901234567890','1234560','1234563',
            '99999999999999999999','1122334455','ABCDEFGHIJK','ZZZZZZZZZ',
            'A1B2C3D4E','X Y Z Y X']
Es, BITS = {}, {}
for s in payloads:
    try: m = render(s, level=3, mask=0, version=4)
    except Exception: continue
    if len(m) != 17: continue
    Es[s] = expected_stream(s)
    BITS[s] = {name: bits_of(m, f) for name, f in MASKS.items()}
print(f"payloads: {len(Es)}")

NCELL = 150   # search data region only
names = list(Es)
best = {'n': 10**9, 'func': None}

def dfs(k, state, func):
    if len(func) > best['n']: return
    if k >= NCELL or all(ei >= len(Es[s]) for s, (ei, _) in state.items()):
        if len(func) < best['n']:
            best['n'] = len(func); best['func'] = set(func)
            print(f"  solution: {len(func)} func cells: {sorted(func)}")
        return
    # DATA branch: cell k consumed by every payload that still needs bits
    ok = True
    newstate = {}
    for s, (ei, mk) in state.items():
        if ei >= len(Es[s]): newstate[s] = (ei, mk); continue
        if BITS[s][mk][k] != Es[s][ei]: ok = False; break
        newstate[s] = (ei + 1, mk)
    if ok:
        dfs(k + 1, newstate, func)
    # FUNCTION branch: skipped everywhere
    if k not in func or True:
        dfs(k + 1, state, func | {k})

# initial masks: branch at cell 0 over all masks per payload is exponential;
# instead pick per-payload mask = the one matching hdr bits at cells 2..7
init = {}
for s in names:
    cands = []
    for name, f in MASKS.items():
        b = BITS[s][name]
        if all(b[2+i] == Es[s][i] for i in range(6)):
            cands.append(name)
    init[s] = cands
    print(f"{s[:22]:24} hdr-masks: {cands}")

# payloads that agree on the row-even mask at the header cells
use = [s for s in names if 'r%2=0' in init[s] and init[s][0] == 'r%2=0'
       or 'r%2=0' in init[s]]
use = [s for s in names if 'r%2=0' in init[s]]
print(f"using {len(use)} row-even payloads: {use}")
st = {s: (0, 'r%2=0') for s in use}
dfs(2, st, set())
if best['func'] is not None:
    print("BEST:", best['n'], sorted(best['func']))
    OLD = {0,1,30,33,66,67,68,69,134,135} | set(range(200,208))
    sp = {}; j = 0
    for k in range(208):
        if k not in OLD: sp[k] = j; j += 1
    print("as old-SP idx:", sorted(sp[k] for k in best['func'] if k in sp))
