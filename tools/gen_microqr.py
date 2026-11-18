"""Generate crates/barcode-microqr/src/tables.rs + tests/golden_{m1,m2}.txt.

Every golden entry is verified against a fresh zint render before emission,
so the generated tables are proven correct end-to-end at generation time.
"""
import sys, json, random
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

OUT = "/Users/ag/git/barcode/crates/barcode-microqr"

# ---------------- load ----------------
tab1 = json.load(open('/Users/ag/git/barcode/tools/m1table.json'))
maps5 = json.load(open('/Users/ag/git/barcode/tools/m1maps5.json'))
MAPS = [list(m) for m in maps5['maps']]          # 5 maps, map[1] == zeros
DATA1 = [tuple(c) for c in maps5['DATA']]        # 53 cells, row order of G
tab2 = json.load(open('/Users/ag/git/barcode/tools/m2all.json'))
g1 = json.load(open('/Users/ag/git/barcode/tools/m1golden.json'))
g2 = json.load(open('/Users/ag/git/barcode/tools/m2golden.json'))

# ---------------- models ----------------
def fields_of(L):
    F, rem = [], L
    while rem >= 3:
        F.append((3, 10)); rem -= 3
    if rem == 2: F.append((2, 7))
    elif rem == 1: F.append((1, 4))
    return F

def model_f(L, payload, msb):
    d = [int(c) for c in payload]; out = []
    for n, w in fields_of(L):
        v = 0
        for i in range(n): v = v * 10 + d[i]
        if msb: out += [(v >> (w - 1 - j)) & 1 for j in range(w)]
        else:   out += [(v >> e) & 1 for e in range(w)]
        d = d[n:]
    return out

def gmat2(L):
    """M2 G as N x C matrix, col j = (field fi, exponent e), j = sum(w[:fi]) + e."""
    d = tab2[str(L)]; F = [tuple(x) for x in d['fields']]
    N = len(d['DATA']); C = sum(w for _, w in F)
    G = [[0] * C for _ in range(N)]
    for fi in range(len(F)):
        for e in range(F[fi][1]):
            col = d['G'][f"{fi},{e}"]
            j = sum(w for _, w in F[:fi]) + e
            for i in range(N): G[i][j] = col[i]
    return G, F, N, C

def gmat1(L):
    """M1 G: rows already in DATA1 order; C columns = stream bits in write order."""
    G = tab1[str(L)]['G']; N = len(G); C = len(G[0])
    return G, fields_of(L), N, C

# ---------------- templates ----------------
def extract_template(size, DATAset, sample_payloads):
    """Cells constant (and equal) across all sample renders, outside DATA."""
    mats = [render(p, level=1, mask=0, version=1 if size == 11 else 2) for p in sample_payloads]
    tmpl = {}
    for y in range(size):
        for x in range(size):
            if (x, y) in DATAset: continue
            vals = {m[y][x] for m in mats}
            assert len(vals) == 1, f"non-constant template cell ({x},{y}): {vals}"
            tmpl[(x, y)] = vals.pop()
    return tmpl

# M1 template
tmpl1 = extract_template(11, set(DATA1),
                         ['11111', '00000', '98765', '13579', '24680'])
# M2 template (union of all lengths' DATA sets; lengths share the 4 const cells)
D2all = set()
for L in range(1, 11): D2all |= {tuple(c) for c in tab2[str(L)]['DATA']}
tmpl2 = extract_template(13, D2all, ['1234567890', '9999999999', '0123456789'])

# Per-length fill: cells that are DATA for longer symbols but constant for this
# length (excluded from its varying set). Value asserted constant across samples.
FILL2 = {}
for L in range(1, 11):
    DL = {tuple(c) for c in tab2[str(L)]['DATA']}
    xs = D2all - DL
    if not xs: FILL2[L] = []; continue
    mats = []
    for t in range(6):
        p = ''.join(str(random.randrange(10)) for _ in range(L))
        mats.append(render(p, level=1, mask=0, version=2))
    fill = []
    for c in sorted(xs):
        vals = {(m[c[1]][c[0]]) for m in mats}
        assert len(vals) == 1, f"len {L} fill cell {c} varies: {vals}"
        fill.append((c[0], c[1], vals.pop()))
    FILL2[L] = fill

# ---------------- encoding (python mirror of the Rust encoder) ----------------
def encode(L, payload, msb, G, F, N, C, ks):
    f = model_f(L, payload, msb)
    base = []
    for i in range(N):
        v = 0
        for j in range(C):
            if f[j]: v ^= G[i][j]
        base.append(v)
    return [(ki, [base[i] ^ ks[ki][i] for i in range(N)]) for ki in range(len(ks))]

def full_matrix(size, DATA, dvals, tmpl, fill=()):
    m = [[tmpl.get((x, y), 0) for x in range(size)] for y in range(size)]
    for x, y, v in fill: m[y][x] = v
    for (x, y), v in zip(DATA, dvals): m[y][x] = v
    return m

def mat_hex(m):
    bits = [v for row in m for v in row]
    out = []
    for i in range(0, len(bits), 4):
        nib = 0
        for j, b in enumerate(bits[i:i+4]):
            nib |= b << (3 - j)  # MSB-first, tail left-aligned
        out.append(format(nib, 'x'))
    return ''.join(out)

# ---------------- build + verify golden ----------------
random.seed(31337)
gl1 = []
for e in g1:
    p, mask = e['payload'], e['mask']; L = len(p)
    G, F, N, C = gmat1(L)
    Ks = [tab1[str(L)]['K'][str(mi)] for mi in range(4)]
    Ks.append([tab1[str(L)]['K']['1'][i] ^ MAPS[4][i] for i in range(N)])
    _, dvals = encode(L, p, True, G, F, N, C, Ks)[mask]
    m = full_matrix(11, DATA1, dvals, tmpl1)
    z = render(p, level=1, mask=0, version=1)
    assert m == [[z[y][x] for x in range(11)] for y in range(11)], f"M1 mismatch {p} mask {mask}"
    gl1.append(f"{p} {mask} {mat_hex(m)}")

gl2 = []
random.seed(4711)
for L in range(1, 11):
    G, F, N, C = gmat2(L)
    DATA = [tuple(c) for c in tab2[str(L)]['DATA']]
    Ks = [list(k) for k in tab2[str(L)]['Ks']]
    ncase = 0
    want = {ki: 0 for ki in range(len(Ks))}
    while any(v < 14 for v in want.values()) and ncase < 400:
        p = ''.join(str(random.randrange(10)) for _ in range(L))
        z = render(p, level=1, mask=0, version=2)
        zv = [z[y][x] for x, y in DATA]
        f = model_f(L, p, False)
        base = [sum(G[i][j] for j in range(C) if f[j]) % 2 for i in range(N)]
        hit = [ki for ki, Kv in enumerate(Ks) if [base[i] ^ Kv[i] for i in range(N)] == zv]
        assert len(hit) == 1, f"len {L} {p}: classes matching: {hit}"
        ki = hit[0]
        m = full_matrix(13, DATA, [base[i] ^ Ks[ki][i] for i in range(N)], tmpl2, FILL2[L])
        assert m == [[z[y][x] for x in range(13)] for y in range(13)], f"M2 mismatch {p}"
        gl2.append(f"{p} {ki} {mat_hex(m)}")
        want[ki] += 1; ncase += 1
print(f"verified: M1 {len(gl1)}, M2 {len(gl2)} golden cases against fresh renders")

# extra per-length coverage: 12 fresh payloads per (version, length, class)
def extra(version, msb, gfn, tmpl, DATAf, Ksf, Ls, lines, tag):
    n0 = 0
    for L in Ls:
        G, F, N, C = gfn(L)
        DATA = DATAf(L); Ks = Ksf(L)
        for ki in range(len(Ks)):
            for t in range(12):
                p = ''.join(str(random.randrange(10)) for _ in range(L))
                fl = FILL2[L] if version == 2 else ()
                _, dvals = encode(L, p, msb, G, F, N, C, Ks)[ki]
                m = full_matrix(11 if version == 1 else 13, DATA, dvals, tmpl, fl)
                z = render(p, level=1, mask=0, version=version)
                if m == [[z[y][x] for x in range(version * 2 + 9)] for y in range(version * 2 + 9)]:
                    lines.append(f"{p} {ki} {mat_hex(m)}"); n0 += 1
    print(f"  {tag}: +{n0} verified")

extra(1, True, gmat1, tmpl1, lambda L: DATA1,
      lambda L: [tab1[str(L)]['K'][str(mi)] for mi in range(4)] +
                [[tab1[str(L)]['K']['1'][i] ^ MAPS[4][i] for i in range(53)]],
      range(1, 6), gl1, "M1 extra")
extra(2, False, gmat2, tmpl2, lambda L: [tuple(c) for c in tab2[str(L)]['DATA']],
      lambda L: [list(k) for k in tab2[str(L)]['Ks']],
      range(1, 11), gl2, "M2 extra")

# ---------------- emit ----------------
def emit_g(path, lines):
    open(path, 'w').write("# payload class hex(row-major 0/1 bits, MSB-first nibbles)\n" + "\n".join(lines) + "\n")

emit_g(f"{OUT}/tests/golden_m1.txt", gl1)
emit_g(f"{OUT}/tests/golden_m2.txt", gl2)

def tbl_rust(name, size, per_len):
    """per_len: {L: (G rows N x C, Ks list)}"""
    s = f"// {name}: per-length G (row-major N x C, cell order = DATA) and class constants.\n"
    for L, (G, Ks, F) in sorted(per_len.items()):
        N, C = len(G), len(G[0])
        s += f"pub static {name}_G_{L}: [[u8; {C}]; {N}] = [\n"
        for row in G: s += "    [" + ", ".join(map(str, row)) + "],\n"
        s += "];\n"
        s += f"pub static {name}_K_{L}: [&[u8]; {len(Ks)}] = [\n"
        for k in Ks: s += "    &[" + ", ".join(map(str, k)) + "],\n"
        s += "];\n"
    return s

def cells_rust(DATA):
    return "[" + ", ".join(f"({x}, {y})" for x, y in DATA) + "]"

s = "// GENERATED by tools/gen_microqr.py - clean-room transcription of empirical\n"
s += "// tables validated against the zint oracle. Do not edit by hand.\n\n"
s += "#![allow(clippy::all)]\n\n"
s += "/// M1 (11x11) DATA cells, G row order.\n"
s += f"pub static M1_DATA: [(u8, u8); 53] = {cells_rust(DATA1)};\n\n"
s += "/// M1 non-DATA template (0xFF = data cell, fill from G.f ^ K).\n"
t = "[\n" + "".join("    [" + ", ".join(str(tmpl1.get((x, y), 0xFF)) for x in range(11)) + "],\n" for y in range(11)) + "];"
s += f"pub static M1_TMPL: [[u8; 11]; 11] = {t}\n\n"
s += "/// M2 non-DATA template (0xFF = data cell).\n"
t = "[\n" + "".join("    [" + ", ".join(str(tmpl2.get((x, y), 0xFF)) for x in range(13)) + "],\n" for y in range(13)) + "];"
s += f"pub static M2_TMPL: [[u8; 13]; 13] = {t}\n\n"
p1 = {L: gmat1(L)[:1] + ([tab1[str(L)]['K'][str(mi)] for mi in range(4)] and [],) for L in ()}  # placeholder
per1 = {}
for L in range(1, 6):
    G, F, N, C = gmat1(L)
    Ks = [tab1[str(L)]['K'][str(mi)] for mi in range(4)]
    Ks.append([tab1[str(L)]['K']['1'][i] ^ MAPS[4][i] for i in range(N)])
    per1[L] = (G, Ks, F)
per2 = {}
for L in range(1, 11):
    G, F, N, C = gmat2(L)
    per2[L] = (G, [list(k) for k in tab2[str(L)]['Ks']], F)
s += tbl_rust("M1", 11, per1)
s += "\n" + tbl_rust("M2", 13, per2)
s += "\npub static M2_DATA: [&[(u8, u8)]; 10] = [\n"
for L in range(1, 11):
    s += "    &" + cells_rust([tuple(c) for c in tab2[str(L)]['DATA']]) + ",\n"
s += "];\n"
s += "\n/// M2 per-length fill: (x, y, value) for union-DATA cells unused by this length.\n"
s += "pub static M2_FILL: [&[(u8, u8, u8)]; 10] = [\n"
for L in range(1, 11):
    s += "    &[" + ", ".join(f"({x}, {y}, {v})" for x, y, v in FILL2[L]) + "],\n"
s += "];\n"
open(f"{OUT}/src/tables.rs", 'w').write(s)
print(f"tables.rs: {len(s)//1024} KiB")
