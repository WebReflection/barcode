"""Definitive M4 function-cell solver.

For each payload: build expected bitstream E (header + data). For each
candidate mask, run a reachability DP over (path-cell k, expected-index ei):
  consume cell k  possible iff bit_k == E[ei] and done(k+1, ei+1)
  skip    cell k  possible iff done(k+1, ei)
A cell is NECESSARILY SKIPPED if no feasible consume transition exists at any
reachable state; NECESSARILY CONSUMED if no feasible skip. Intersect the
necessarily-skipped sets across payloads -> true function cells.
"""
import sys
sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

A = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:"
def av(c): return A.index(c)

def _funct(x, y):
    return (x <= 7 and y <= 7) or (x == 8 and y <= 8) or (y == 8 and x <= 7)
_STRIPS = [(16,15),(14,13),(12,11),(10,9),(8,7),(6,5),(4,3),(2,1),(0,None)]
PATH = []
for _si, (_hi, _lo) in enumerate(_STRIPS):
    _rows = range(16, -1, -1) if _si % 2 == 0 else range(17)
    for _y in _rows:
        for _x in ((_hi,) if _lo is None else (_hi, _lo)):
            if not _funct(_x, _y):
                PATH.append((_x, _y))
assert len(PATH) == 208

def bits_l2r(v, w):
    return [(v >> (w - 1 - i)) & 1 for i in range(w)]

def expected_stream(s):
    n = len(s)
    isalnum = all(c in A for c in s) and not s.isdigit()
    cnt = n // 2 if s.isdigit() else n
    E = [1 if isalnum else 0] + bits_l2r(cnt, 5)   # t0 + count(5b, MSB-first)
    if s.isdigit():
        E.append(n % 2)                   # odd-digit flag
        d = s
        while len(d) >= 3:
            E += bits_l2r(int(d[:3]), 10); d = d[3:]
        if len(d) == 2: E += bits_l2r(int(d), 7)
        elif len(d) == 1: E += bits_l2r(int(d), 4)
    elif all(c in A for c in s):
        i = 0
        while i + 1 < n:
            E += bits_l2r(av(s[i]) * 45 + av(s[i+1]), 11); i += 2
        if i < n: E += bits_l2r(av(s[i]), 6)
    else:
        for ch in s.encode(): E += bits_l2r(ch, 8)
    return E

MASKS = {
    'r%2=0':    lambda x, y: y % 2 == 0,
    '(y>>1)&1': lambda x, y: (y >> 1) & 1,
    '(x>>1)&1': lambda x, y: (x >> 1) & 1,
    '(x+y)%2=0': lambda x, y: (x + y) % 2 == 0,
    '(x+y)%3=0': lambda x, y: (x + y) % 3 == 0,
    'x%3=0':    lambda x, y: x % 3 == 0,
    '(x*y)%2+xy%3=0': lambda x, y: (x*y) % 2 + (x*y) % 3 == 0,
}

def solve(m, f, E):
    """Returns (necessarily_skipped_set, ok) for PATH-ordered cells."""
    N, M = len(PATH), len(E)
    # bits per cell
    b = [m[y][x] ^ (1 if f(x, y) else 0) for x, y in PATH]
    # done[k][ei]: can complete cells k.. consuming E[ei:]
    done = [[False] * (M + 1) for _ in range(N + 1)]
    for ei in range(M + 1): done[N][ei] = (ei == M)
    for k in range(N - 1, -1, -1):
        for ei in range(M, -1, -1):
            if ei == M:
                done[k][ei] = True   # data complete; rest are pad/ecc
                continue
            if done[k+1][ei]:
                done[k][ei] = True
            elif b[k] == E[ei] and done[k+1][ei+1]:
                done[k][ei] = True
    if not done[0][0]:
        return None, False
    # reach + classify
    reach = [[False] * (M + 1) for _ in range(N + 1)]
    reach[0][0] = True
    skipped = set(); consumed = set()
    amb = set()
    for k in range(N):
        any_skip = any(reach[k][ei] and done[k+1][ei] for ei in range(M + 1))
        any_consume = any(reach[k][ei] and ei < M and b[k] == E[ei] and done[k+1][ei+1]
                          for ei in range(M + 1))
        if any_skip and not any_consume: skipped.add(k)
        elif any_consume and not any_skip: consumed.add(k)
        elif any_consume and any_skip: amb.add(k)
        for ei in range(M + 1):
            if reach[k][ei]:
                if done[k+1][ei]: reach[k+1][ei] = True
                if ei < M and b[k] == E[ei] and done[k+1][ei+1]: reach[k+1][ei+1] = True
    return skipped, True

def solve_payload(s, lv):
    try:
        m = render(s, level=lv, mask=0, version=4)
    except Exception:
        return None
    if len(m) != 17: return None
    E = expected_stream(s)
    for name, f in MASKS.items():
        sk, ok = solve(m, f, E)
        if ok:
            return (name, sk)
    return None

if __name__ == '__main__':
    payloads = [
        ('abcdefghi', 3), ('abcdefgh', 3), ('abcdefg', 3), ('abcdef', 3),
        ('!!!!!!!', 3), ('!!!!!', 3), ('~~~!!~~!', 3), ('[[]][][]', 3),
        ('xyzxyzxy', 3), ('{{{{{{{', 3),
        ('1234567890', 3), ('12345678901234567890', 3), ('1234560', 3),
        ('1234563', 3), ('99999999999999999999', 3), ('1122334455', 3),
        ('ABCDEFGHIJK', 3), ('ZZZZZZZZZ', 3), ('A1B2C3D4E', 3), ('X Y Z Y X', 3),
        ('12345678901234567890123456789012345', 1), ('a1b2c3d4e5f6g7h8i9', 3),
    ]
    union = set(); cnt = 0
    for s, lv in payloads:
        r = solve_payload(s, lv)
        if r:
            name, sk = r; cnt += 1
            union |= sk
            print(f"{s[:24]:26} mask={name:10} nskipped={len(sk)} skipped={sorted(sk)[:14]}")
        else:
            print(f"{s[:24]:26} NO MASK/FEASIBLE")
    print(f"\nvalidated: {cnt}; UNION of necessarily-skipped: {sorted(union)}")
    print("count:", len(union))
    OLD = {0,1,30,33,66,67,68,69,134,135} | set(range(200,208))
    sp = {}; j = 0
    for k in range(208):
        if k not in OLD: sp[k] = j; j += 1
    print("as old-SP idx:", sorted(sp[k] for k in union if k in sp))
