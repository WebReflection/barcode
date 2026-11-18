"""Bulletproof linearity: for payloads (v@0), (w@1), (v@0,w@1) all row-even:
verify data CWs, then compare ecc(a) xor ecc(b) vs ecc(ab)."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of

def payload(pairs):
    b = ['\x00']*9
    for pos, v in pairs: b[pos] = chr(v)
    return ''.join(b)

def full(pairs):
    s = payload(pairs)
    name, t = stream_law(s, 3)
    if name != 'row-even': return None, None, None
    return cws_of(t, 6, 9), cws_of(t, 78, 14), t

found = 0
for v in (1,2,3,4,0x41,0x7e,0x21,0x5a):
    for w in (1,2,3,4,0x41,0x7e,0x21,0x5a):
        d0,e0,_ = full([(0,v)])
        d1,e1,_ = full([(1,w)])
        dab,eab,t = full([(0,v),(1,w)])
        if e0 is None or e1 is None or eab is None: continue
        if d0 != [v]+[0]*8 or d1 != [0,w]+[0]*7 or dab != [v,w]+[0]*7:
            print("DATA MISMATCH", v, w, d0, d1, dab); sys.exit(1)
        x = [e0[i]^e1[i] for i in range(14)]
        match = x == eab
        print(f"v={v:#04x} w={w:#04x}: match={match}")
        if not match:
            print("  xor  :", [hex(c) for c in x])
            print("  actual:", [hex(c) for c in eab])
            found += 1
            if found >= 2: sys.exit(0)
