"""Classify PATH cells as definitely-data / definitely-function / ambiguous
using DP over (cell, expected-bit-index) states, restricted to cells that
occur before data completion. Union across diverse payloads."""
import sys
sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render
from funcsolve import expected_stream, MASKS, PATH, A

def classify(m, f, E):
    """Returns (func_cells, data_cells, ok, last_relevant) for PATH-ordered cells."""
    N, M = len(PATH), len(E)
    b = [m[y][x] ^ (1 if f(x, y) else 0) for x, y in PATH]
    done = [[False]*(M+2) for _ in range(N+1)]
    for ei in range(M+1): done[N][ei] = True
    for k in range(N-1, -1, -1):
        for ei in range(M, -1, -1):
            if ei == M: done[k][ei] = True; continue
            if done[k+1][ei]: done[k][ei] = True
            elif b[k] == E[ei] and done[k+1][ei+1]: done[k][ei] = True
    if not done[0][0]: return None, None, False, None
    reach = [[False]*(M+2) for _ in range(N+1)]
    reach[0][0] = True
    func, data, amb = set(), set(), set()
    last = 0
    for k in range(N):
        pre = [ei for ei in range(M+1) if reach[k][ei]]
        if not pre: break
        last = k
        any_pre = any(ei < M for ei in pre)   # cell can still be before data end
        if any_pre:
            can_skip = any(ei < M and done[k+1][ei] for ei in pre if ei < M)
            can_consume = any(ei < M and b[k] == E[ei] and done[k+1][ei+1] for ei in pre if ei < M)
            if can_skip and not can_consume: func.add(k)
            elif can_consume and not can_skip: data.add(k)
            elif can_consume and can_skip: amb.add(k)
        for ei in list(pre):
            if ei < M and b[k] == E[ei] and done[k+1][ei+1]: reach[k+1][ei+1] = True
            if done[k+1][ei]: reach[k+1][ei] = True
    return func, data, True, last

payloads = [
    ('abcdefghi',3),('abcdefgh',3),('abcdefg',3),('abcdef',3),
    ('!!!!!!!',3),('!!!!!',3),('~~~!!~~!',3),('[[]][][]',3),
    ('xyzxyzxy',3),('{{{{{{{',3),('1234567890',3),('12345678901234567890',3),
    ('1234560',3),('1234563',3),('99999999999999999999',3),('1122334455',3),
    ('ABCDEFGHIJK',3),('ZZZZZZZZZ',3),('A1B2C3D4E',3),('X Y Z Y X',3),
]
func_all = {}; data_all = {}
for s, lv in payloads:
    try: m = render(s, level=lv, mask=0, version=4)
    except Exception: continue
    if len(m) != 17: continue
    E = expected_stream(s)
    best = None
    for name, f in MASKS.items():
        fu, da, ok, last = classify(m, f, E)
        if ok and (best is None or len(fu) < len(best[1])):
            best = (name, fu, da, last)
    if best:
        name, fu, da, last = best
        func_all[s] = fu; data_all[s] = da
        print(f"{s[:22]:24} {name:10} func={sorted(fu)[:10]} data_last={last}")

# cells definitely function in >=1 payload:
nev_data = set()
for s in func_all: nev_data |= func_all[s]
always_data = None
for s in data_all:
    always_data = data_all[s] if always_data is None else (always_data & data_all[s])
print("\nUNION func (never-data anywhere):", sorted(nev_data))
print("INTERSECTION data (always consumed):", sorted(always_data) if always_data else None)
OLD = {0,1,30,33,66,67,68,69,134,135} | set(range(200,208))
sp = {}; j = 0
for k in range(208):
    if k not in OLD: sp[k] = j; j += 1
print("union func as old-SP:", sorted(sp[k] for k in nev_data if k in sp))
