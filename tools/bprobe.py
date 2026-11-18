"""Bytes-payload probe layer: pyzint takes bytes 1:1 (str gets UTF-8 encoded,
which inflated 0x80 -> C2 80 and broke all MSB-only probes)."""
import sys
sys.path.insert(0, "/tmp/opencode")
sys.path.insert(0, "/Users/ag/git/barcode/tools")

from funcsolve import bits_l2r, A as ALNUM_A, av, PATH  # reuse
from mqr_probe import render

FUNC = {0,1,32,33,34,35,100,101,102,103,200,201,202,203,204,205,206,207}
SPv = [k for k in range(208) if k not in FUNC]
def A_(y): return ((y//3)%2)==1
def B_(y): return (((y-1)//3)%2)==1
def C_(y): return (y%2)==1
LAW = {'row-even': lambda x,y: y%2==0, 'xy%6<3': lambda x,y: (x*y)%6<3,
       'coarse': lambda x,y: (x//3+y//2+1)%2==1,
       'ABC': lambda x,y: [not A_(y),C_(y),B_(y),A_(y),not C_(y),not B_(y)][(16-x)%6]}

def expected_stream_b(data: bytes):
    """byte-mode only: header(6) + raw bytes."""
    E = [0] + bits_l2r(len(data), 5)
    for b in data:
        E += [(b >> (7-i)) & 1 for i in range(8)]
    return E

def stream_law_b(data: bytes, lv=3):
    try: m = render(data, level=lv, mask=0, version=4)
    except Exception: return None, None
    if len(m) != 17: return None, None
    E = expected_stream_b(data)
    for name, f in LAW.items():
        t = [m[y][x] ^ (1 if f(x,y) else 0) for x,y in [PATH[k] for k in SPv]]
        if t[:len(E)] == E: return name, t
    return None, None

def payload(setbits, n=9):
    b = bytearray(n)
    for j in setbits:
        b[j//8] |= 1 << (7 - j % 8)
    return bytes(b)

def cws_of(t, off, n):
    return [int(''.join(map(str, t[off+8*k:off+8*k+8])), 2) for k in range(n)]

def ecc_of(setbits, lv=3, want='row-even'):
    name, t = stream_law_b(payload(setbits), lv)
    if name != want: return None
    return cws_of(t, 78, 14)
