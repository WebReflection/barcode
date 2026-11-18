"""Cross-check zint vs zxing Micro QR matrices for identical payload+level.
Also validate python RS port via syndrome property."""
import sys
sys.path.insert(0, "/tmp/opencode")
import numpy as np, zxingcpp
from rssolve2 import samples, build, gf_mul, encode, gen, NECC
from mqr_probe import render

# 1) syndrome sanity of my port
exp, log = build(0x11d)
g14 = gen(exp, log, 14, 0)
d0 = [0x61,0,0,0xEC,0x11,0xEC,0x11,0xEC,0x11]
e0 = encode(d0, 14, g14, exp, log)
def synd(cw, exp, log, nn):
    out = []
    for j in range(nn):
        a = exp[j]; acc = 0
        for c in cw: acc = gf_mul(acc, a, exp, log) ^ c
        out.append(acc)
    return out
print("syndromes of my encode (should be all 0):", set(synd(d0+e0, exp, log, 14)))

# 2) matrix helpers
def zx_matrix(payload, version=4, **kw):
    b = zxingcpp.create_barcode(payload, zxingcpp.BarcodeFormat.MicroQRCode,
                                version=version, **kw)
    a = np.asarray(b.to_image(8, True), dtype=np.uint8) < 128
    ys, xs = np.where(a)
    a = a[ys.min():ys.max()+1, xs.min():xs.max()+1]
    n = a.shape[0]; step = n / 17
    return [[int(a[int(y*step+step/2)][int(x*step+step/2)]) for x in range(17)] for y in range(17)]

def zint_matrix(payload, lv):
    return render(payload, level=lv, mask=0, version=4)

print("zxing kwargs test: try ecc_level")
try:
    m = zx_matrix('a', ecc_level=1)
    print("  ecc_level=1 ok")
except Exception as ex:
    print("  ecc_level failed:", ex)
    try:
        m = zx_matrix('a', ecc='Q')
        print("  ecc='Q' ok")
    except Exception as ex2:
        print("  ecc failed:", ex2)

# compare matrices for same payload across levels
for lv in (1,2,3):
    mz = None
    for kw in ({'ecc_level': lv}, {'ecc': {1:'L',2:'M',3:'Q'}[lv]}):
        try:
            mz = zx_matrix('aaaaaaaaa', **kw); break
        except Exception: pass
    mzk = zx_matrix('aaaaaaaaa') if mz is None else mz
    mzi = zint_matrix('aaaaaaaaa', lv)
    diff = sum(1 for y in range(17) for x in range(17) if mzk[y][x] != mzi[y][x])
    print(f"lv{lv}: zint vs zxing cell diffs = {diff}/289")
