"""RS solve v3: widen space - all primitive degree-8 polys, fcr 0..15,
root spacing u in 1..16 (generator prod (x - a^(fcr+u*i))), order variants."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rssolve2 import samples, build, gf_mul, encode, NDATA, NECC  # reuse

POLYS=[0x11b,0x11d,0x12b,0x12d,0x139,0x13f,0x14d,0x15f,0x163,0x165,0x169,
       0x171,0x177,0x17b,0x187,0x18b,0x18d,0x19f,0x1a3,0x1a9,0x1b1,0x1bd,
       0x1c3,0x1cf,0x1d7,0x1dd,0x1e7,0x1f3,0x1f5,0x1f9]

def gen_u(exp,log,nn,fcr,u):
    g=[1]
    for i in range(nn):
        a=exp[(fcr+u*i)%255]
        nxt=[0]*(len(g)+1)
        for k,c in enumerate(g):
            nxt[k+1]^=c; nxt[k]^=gf_mul(c,a,exp,log)
        g=nxt
    return g[::-1]

best=(999,)
for poly in POLYS:
    exp,log=build(poly)
    for fcr in range(16):
        for u in range(1,17):
            gs={ne:gen_u(exp,log,ne,fcr,u) for ne in (8,10,14)}
            hits=0; tot=0
            for lv in (1,2,3):
                for s,d,e in samples[lv]:
                    tot+=1
                    if encode(d,NECC[lv],gs[NECC[lv]],exp,log)==e: hits+=1
            if hits>best[0]: best=(hits,poly,fcr,u)
            if hits==tot:
                print(f"*** MATCH poly=0x{poly:x} fcr={fcr} u={u}"); sys.exit(0)
print("no match; best:", best)
