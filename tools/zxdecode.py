"""Does zxing decode zint's lv3 render? Print ecc CWs for the cleanest
sample (aaaaaaaaa: 9 bytes, no pad, input certain = 61 x9)."""
import sys
sys.path.insert(0, "/tmp/opencode")
import numpy as np, zxingcpp
from rsunit2 import stream_law, cws_of
from mqr_probe import render

def zimg(s, lv):
    m = render(s, level=lv, mask=0, version=4)
    n = len(m)
    a = np.zeros((n, n), dtype=np.uint8)
    for y in range(n):
        for x in range(n):
            a[y][x] = 0 if m[y][x] else 255
    return a

for s in ('aaaaaaaaa', 'a'):
    img = zimg(s, 3)
    # pad with quiet zone
    big = np.full((img.shape[0]+16, img.shape[1]+16), 255, dtype=np.uint8)
    big[8:-8, 8:-8] = img
    from PIL import Image
    pil = Image.fromarray(big)
    res = zxingcpp.read_barcodes(pil)
    print(f"{s!r}: zxing decode -> {[r.text for r in res] if res else 'FAIL'}")
    name, t = stream_law(s, 3)
    if t:
        nd = 6 + 8*len(s) if s == 'aaaaaaaaa' else 78
        ndata = 9
        print(f"  law={name} data_cws={[hex(v) for v in cws_of(t,6,9)]}")
        print(f"  ecc_cws={[hex(v) for v in cws_of(t, 6+8*9, 14)]}")
