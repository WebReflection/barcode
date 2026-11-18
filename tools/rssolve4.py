"""Match RS candidates against only the TRUSTED prefix of the ecc region.
For lv3, ecc CWs 0..5 (stream 78..126) lie in validated mask/path territory
(EC11 pad clean there in lv1 renders); CWs 6..13 (strips 4-8) are not.
Score candidates on the trusted prefix only."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import samples, build, gf_mul, encode, NDATA, NECC

def gen_u(exp,log,nn,fcr,u):
    g=[1]
    for i in range(nn):
        a=exp[(fcr+u*i)%255]
        nxt=[0]*(len(g)+1)
        for k,c in enumerate(g):
            nxt[k+1]^=c; nxt[k]^=gf_mul(c,a,exp,log)
        g=nxt
    return g[::-1]

TRUSTED = {3: 6, 2: 2, 1: 0}   # trusted ecc CW count per level (lv1: none)

POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
       0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
       0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]

results = []
for poly in POLYS:
    exp,log=build(poly)
    for fcr in range(16):
        for u in range(1,17):
            score=0; tot=0
            for lv in (1,2,3):
                T = TRUSTED[lv]
                if T==0: continue
                g=gen_u(exp,log,NECC[lv],fcr,u)
                for s,d,e in samples[lv]:
                    calc=encode(d,NECC[lv],g,exp,log)
                    score += sum(1 for i in range(T) if calc[i]==e[i]); tot += T
            results.append((score/tot if tot else 0, poly, fcr, u, score, tot))
results.sort(reverse=True)
for r in results[:10]: print(r)
