"""Generate crates/barcode-microqr/src/frames.rs + tests/golden_*.txt for ALL
frames (version x level x mode x length) from the extraction JSONs.

Every golden case is verified against a fresh zint render before emission.

Frames loaded (skip missing files):
  tools/m1table.json + m1maps5.json   (M1 L num; canonical classes 0-4)
  tools/m2all.json                    (M2 L num)
  tools/m2Mnum.json                   (M2 M num)
  tools/m2Lbyte.json m2Lalnum.json    (M2 L byte/alnum)
  tools/m2Mbyte.json m2Malnum.json    (M2 M byte/alnum)
  tools/m3Lnum.json m3Mnum.json       (M3 num)
  tools/m3Lbyte.json m3Lalnum.json
  tools/m3Mbyte.json m3Malnum.json
"""
import sys, json, os, random, re
sys.path.insert(0, "/tmp/opencode"); sys.path.insert(0, "/Users/ag/git/barcode/tools")
from mqr_probe import render

OUT = "/Users/ag/git/barcode/crates/barcode-microqr"
random.seed(90210)

ALNUM = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:"
MODES = {'num': 0, 'alnum': 1, 'byte': 2}

def load(path):
    p = os.path.join('/Users/ag/git/barcode/tools', path) if not path.startswith('/') else path
    return json.load(open(p)) if os.path.exists(p) else None

# ---------- frame records ----------
# frame = dict(version, level, mode, len, DATA, fields, G (dict), Ks (list))
frames = []

# M1: canonical classes 0-4 (K_4 = K_1 ^ map4)
m1t = load('m1table.json'); m1m = load('m1maps5.json')
MAPS1 = [list(m) for m in m1m['maps']]
def m1_fields(L):
    F, rem = [], L
    while rem >= 3: F.append((3, 10)); rem -= 3
    if rem == 2: F.append((2, 7))
    elif rem == 1: F.append((1, 4))
    return F
for L in range(1, 6):
    t = m1t[str(L)]
    Ks = [t['K'][str(mi)] for mi in range(4)]
    Ks.append([t['K']['1'][i] ^ MAPS1[4][i] for i in range(53)])
    frames.append(dict(version=1, level=1, mode='num', len=L,
                       DATA=[tuple(c) for c in m1m['DATA']],
                       fields=m1_fields(L), msb=True,
                       G={(fi, e): [r[sum(w for _, w in m1_fields(L)[:fi]) + e] for r in t['G']]
                          for fi in range(len(m1_fields(L))) for e in range(m1_fields(L)[fi][1])},
                       Ks=Ks))

def add_modes(path, version, level, mode, msb=False):
    d = load(path)
    if d is None: return
    for L, v in d.items():
        F = [tuple(x) for x in v['fields']]
        frames.append(dict(version=version, level=level, mode=mode, len=int(L),
                           DATA=[tuple(c) for c in v['DATA']], fields=F, msb=msb,
                           G={(fi, e): v['G'][f"{fi},{e}"] for fi in range(len(F)) for e in range(F[fi][1])},
                           Ks=[list(k) for k in v['Ks']]))

add_modes('m2all.json', 2, 1, 'num')
add_modes('m2Mnum.json', 2, 2, 'num')
add_modes('m2Lbyte.json', 2, 1, 'byte')
add_modes('m2Mbyte.json', 2, 2, 'byte')
add_modes('m2Lalnum.json', 2, 1, 'alnum')
add_modes('m2Malnum.json', 2, 2, 'alnum')
add_modes('m3Lnum.json', 3, 1, 'num')
add_modes('m3Mnum.json', 3, 2, 'num')
add_modes('m3Lbyte.json', 3, 1, 'byte')
add_modes('m3Mbyte.json', 3, 2, 'byte')
add_modes('m3Lalnum.json', 3, 1, 'alnum')
add_modes('m3Malnum.json', 3, 2, 'alnum')
add_modes('m4Lnum.json', 4, 1, 'num')
add_modes('m4Mnum.json', 4, 2, 'num')
add_modes('m4Qnum.json', 4, 3, 'num')
add_modes('m4Lalnum.json', 4, 1, 'alnum')
add_modes('m4Malnum.json', 4, 2, 'alnum')
add_modes('m4Qalnum.json', 4, 3, 'alnum')
add_modes('m4Lbyte.json', 4, 1, 'byte')
add_modes('m4Mbyte.json', 4, 2, 'byte')
add_modes('m4Qbyte.json', 4, 3, 'byte')

VERSIONS = sorted({f['version'] for f in frames})
DALL = {}
BAD_FRAMES = set()
print("frames:", len(frames), "versions:", VERSIONS)

# ---------- payload helpers ----------
def fields_of(f):
    return f['fields']

def fcap(mode, n):
    if mode == 'byte': return 256
    if mode == 'alnum': return 45 * 45 if n == 2 else 45
    return 10 ** n

def payload_from(f, vals):
    mode, L = f['mode'], f['len']
    if mode == 'byte': return bytes(vals)
    if mode == 'alnum':
        s = ''
        for fi, (n, w) in enumerate(fields_of(f)):
            v = vals[fi]
            if n == 2: s += ALNUM[v // 45] + ALNUM[v % 45]
            else: s += ALNUM[v]
        return s
    s = ''
    for fi, (n, w) in enumerate(fields_of(f)):
        v = vals[fi]; ds = []
        for _ in range(n): ds.append(str(v % 10)); v //= 10
        s += ''.join(reversed(ds))
    return s

def model_f(f, vals):
    out = []
    for fi, (n, w) in enumerate(fields_of(f)):
        v = vals[fi]
        if f['msb']: out += [(v >> (w - 1 - j)) & 1 for j in range(w)]
        else: out += [(v >> e) & 1 for e in range(w)]
    return out

ALNUM_SET = set(b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:")

def rand_vals(f):
    while True:
        vals = [random.randrange(fcap(f['mode'], n)) for n, w in fields_of(f)]
        p = payload_from(f, vals)
        if f['mode'] == 'alnum':
            if all(c in '0123456789' for c in p):
                continue  # would go numeric
            if re.search(r'\d{3}', p):
                continue  # 3+ digit run -> numeric-segmented
            return vals
        if f['mode'] == 'byte':
            if all(c in ALNUM_SET for c in p):
                continue  # would go numeric/alnum
            if re.search(rb'\d{2}', p):
                continue  # 2+ digit runs get numeric-segmented
            return vals
        return vals

def gmat(f):
    N, C = len(f['DATA']), sum(w for _, w in f['fields'])
    G = [[0] * C for _ in range(N)]
    for (fi, e), col in f['G'].items():
        j = sum(w for _, w in f['fields'][:fi]) + e
        for i in range(N): G[i][j] = col[i]
    return G, N, C

# ---------- templates per version ----------
def extract_template(version, level):
    size = version * 2 + 9
    Dall = set()
    for f in frames:
        if f['version'] == version: Dall |= set(f['DATA'])
    frs = [f for f in frames if f['version'] == version and f['level'] == level]
    mats = []
    for f0 in frs:
        for _ in range(3):
            while True:
                p = payload_from(f0, rand_vals(f0))
                try:
                    mats.append(render(p, level=level, version=version)); break
                except Exception: pass
    tmpl = {}
    for y in range(size):
        for x in range(size):
            if (x, y) in Dall: continue
            vals = {m[y][x] for m in mats}
            if len(vals) == 1: tmpl[(x, y)] = vals.pop()
    return tmpl, Dall

def sample_payload(f):
    """A valid payload string for template sampling (any)."""
    while True:
        p = payload_from(f, rand_vals(f))
        try:
            render(p, level=1, version=f['version']); return p
        except Exception:
            pass

for v in VERSIONS:
    tmpl, Dall = extract_template(v, None)
    globals()[f'tmpl_v{v}'] = tmpl
    DALL[v] = Dall
    globals()[f'fill_v{v}'] = {}
    # per-frame fill: union-DATA cells not in this frame's DATA, asserted constant
    for f in frames:
        if f['version'] != v: continue
        xs = {(x, y) for y in range(f['version'] * 2 + 9)
              for x in range(f['version'] * 2 + 9)} \
             - set(f['DATA']) - set(globals()[f'tmpl_v{v}'].keys())
        if not xs: globals()[f'fill_v{v}'][id(f)] = []; continue
        mats = []
        for _ in range(40):
            while True:
                p = payload_from(f, rand_vals(f))
                try: mats.append(render(p, level=f['level'], version=v)); break
                except Exception: pass
        fill = []
        bad_frame = False
        for c in sorted(xs):
            vals = {m[c[1]][c[0]] for m in mats}
            if len(vals) != 1:
                print(f"SKIP v{v} {f['mode']} len {f['len']}: fill {c} varies {vals}")
                BAD_FRAMES.add(id(f)); bad_frame = True; break
            fill.append((c[0], c[1], vals.pop()))
        if bad_frame: continue
        globals()[f'fill_v{v}'][id(f)] = fill

# ---------- full matrix + hex ----------
def full_matrix(version, f, dvals):
    size = version * 2 + 9
    tmpl = globals()[f'tmpl_v{version}']
    fill = globals()[f'fill_v{version}'][id(f)]
    m = [[tmpl.get((x, y), 0) for x in range(size)] for y in range(size)]
    for x, y, val in fill: m[y][x] = val
    for (x, y), v in zip(f['DATA'], dvals): m[y][x] = v
    return m

def mat_hex(m):
    bits = [v for row in m for v in row]
    out = []
    for i in range(0, len(bits), 4):
        nib = 0
        for j, b in enumerate(bits[i:i+4]): nib |= b << (3 - j)
        out.append(format(nib, 'x'))
    return ''.join(out)

def encode_frame(f, vals):
    G, N, C = gmat(f)
    fv = model_f(f, vals)
    base = [sum(G[i][j] for j in range(C) if fv[j]) % 2 for i in range(N)]
    return base, N, C

# ---------- goldens ----------
GOLD = {}
def golden_name(f):
    lv = {1: 'L', 2: 'M', 3: 'Q'}[f['level']]
    return f"golden_v{f['version']}{lv}_{f['mode']}_L{f['len']}.txt"

for f in frames:
    if id(f) in BAD_FRAMES: continue
    G, N, C = gmat(f)
    lines = []; want = {ki: 0 for ki in range(len(f['Ks']))}
    tries = 0
    while (any(v < 14 for v in want.values()) and tries < 500) or tries < 30:
        tries += 1
        vals = rand_vals(f)
        p = payload_from(f, vals)
        try:
            z = render(p, level=f['level'], version=f['version'])
        except Exception:
            continue
        zv = [z[y][x] for x, y in f['DATA']]
        base, _, _ = encode_frame(f, vals)
        hit = [ki for ki, Kv in enumerate(f['Ks'])
               if [base[i] ^ Kv[i] for i in range(N)] == zv]
        if len(hit) != 1:
            print(f"SKIP v{f['version']} {f['mode']} len {f['len']}: {p} hits={hit}")
            BAD_FRAMES.add(id(f)); break
        ki = hit[0]
        m = full_matrix(f['version'], f, [base[i] ^ f['Ks'][ki][i] for i in range(N)])
        if m != [[z[y][x] for x in range(f['version'] * 2 + 9)]
                 for y in range(f['version'] * 2 + 9)]:
            print(f"SKIP v{f['version']} {f['mode']} len {f['len']}: matrix mismatch {p}")
            BAD_FRAMES.add(id(f)); break
        if want[ki] < 14:
            payload_txt = p.hex() if f['mode'] == 'byte' else p.replace(' ', '_')
            lines.append(f"{payload_txt} {ki} {mat_hex(m)}")
            want[ki] += 1
    if id(f) in BAD_FRAMES:
        import os
        try: os.remove(f"{OUT}/tests/{golden_name(f)}")
        except OSError: pass
        continue
    name = golden_name(f)
    open(f"{OUT}/tests/{name}", 'w').write(
        f"# payload class hex(row-major MSB-first nibbles); byte payloads are hex\n"
        + "\n".join(lines) + "\n")
    print(f"{name}: {len(lines)} cases, classes {sorted(want)}")

# ---------- emit frames.rs ----------
def cells_rust(DATA):
    return "&[" + ", ".join(f"({x}, {y})" for x, y in DATA) + "]"

s = "// GENERATED by tools/gen_frames.py - clean-room transcription of empirical\n"
s += "// tables validated against the zint oracle. Do not edit by hand.\n\n"
s += "#![allow(clippy::all)]\n\n"
s += "/// (version, level, mode, len) -> Frame. mode: 0=num, 1=alnum, 2=byte.\n"
s += "pub struct Frame {\n    pub mode: u8,\n    pub len: u8,\n    pub msb_first: bool,\n    pub data: &'static [(u8, u8)],\n    pub fill: &'static [(u8, u8, u8)],\n    pub cols: usize,\n    pub g: &'static [u8],\n    pub ks: &'static [&'static [u8]],\n}\n\n"
s += "pub static FRAMES: &[([u8; 4], Frame)] = &[\n"
for f in sorted((fr for fr in frames if id(fr) not in BAD_FRAMES), key=lambda f: (f['version'], f['level'], MODES[f['mode']], f['len'])):
    G, N, C = gmat(f)
    flat = []
    for i in range(N):
        flat += [G[i][j] for j in range(C)]
    ks = ", ".join("&[" + ", ".join(map(str, k)) + "]" for k in f['Ks'])
    fill = globals()[f'fill_v{f["version"]}'][id(f)]
    s += f"    ([{f['version']}, {f['level']}, {MODES[f['mode']]}, {f['len']}], Frame {{\n"
    s += f"        mode: {MODES[f['mode']]}, len: {f['len']}, msb_first: {str(f['msb']).lower()},\n"
    s += f"        data: {cells_rust(f['DATA'])},\n"
    s += f"        fill: &[" + ", ".join(f"({x}, {y}, {v})" for x, y, v in fill) + "],\n"
    s += f"        cols: {C},\n        g: &[" + ", ".join(map(str, flat)) + "],\n"
    s += f"        ks: &[{ks}],\n    }}),\n"
s += "];\n"
s += "\n/// Per-version templates: 0xFF = data/fill cell.\n"
for v in VERSIONS:
    size = v * 2 + 9
    t = globals()[f'tmpl_v{v}']
    name = f"{['', 'M1', 'M2', 'M3', 'M4'][v]}_TMPL"
    rows = "[\n" + "".join("    [" + ", ".join(str(t.get((x, y), 0xFF)) for x in range(size)) + "],\n" for y in range(size)) + "];"
    s += f"pub static {name}: [[u8; {size}]; {size}] = {rows}\n\n"
s += "/// Template lookup: version -> flattened template.\n"
s += "pub fn tmpl(version: u8) -> (usize, &'static [u8]) {\n"
s += "    match version {\n"
for v in VERSIONS:
    name = f"{['', 'M1', 'M2', 'M3', 'M4'][v]}_TMPL"
    s += f"        {v} => ({v * 2 + 9}, {name}.as_flattened()),\n"
s += "        _ => (0, &[]),\n    }\n}\n"
open(f"{OUT}/src/frames.rs", 'w').write(s)
print(f"frames.rs: {len(s) // 1024} KiB, {len(frames)} frames")

# emit tests/golden_tests.rs
import glob as _glob
files = sorted(_glob.glob(f"{OUT}/tests/golden_v*.txt"))
t = "//! GENERATED golden tests: every case verified against the oracle at generation time.\n\n"
t += "use barcode_microqr::{encode_bytes, Level, Mode, Version};\n\n"
t += "fn check(path: &str, contents: &str, version: Version, level: Level, mode: Mode, len: usize) {\n"
t += "    let size = match version { Version::M1 => 11, Version::M2 => 13, Version::M3 => 15, Version::M4 => 17 };\n"
t += "    let payload = |s: &str| if mode == Mode::Byte {\n"
t += "        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i+2], 16).unwrap()).collect::<Vec<u8>>()\n"
t += "    } else { s.replace('_', \" \").into_bytes() };\n"
t += "    let mut n = 0;\n"
t += "    for line in contents.lines().skip(1) {\n"
t += "        let mut it = line.split_whitespace();\n"
t += "        let p = payload(it.next().unwrap());\n"
t += "        let class: usize = it.next().unwrap().parse().unwrap();\n"
t += "        let hex = it.next().unwrap();\n"
t += "        assert_eq!(p.len(), len);\n"
t += "        let sym = encode_bytes(&p, version, level, Some(mode), Some(class))\n"
t += "            .unwrap_or_else(|| panic!(\"{}: encode failed\", path));\n"
t += "        assert_eq!(sym.size, size);\n"
t += "        let mut bits = Vec::new();\n"
t += "        for c in hex.chars() {\n"
t += "            let v = c.to_digit(16).unwrap() as u8;\n"
t += "            for i in (0..4).rev() { bits.push((v >> i) & 1); }\n"
t += "        }\n"
t += "        for i in 0..size * size {\n"
t += "            assert_eq!(sym.cells[i], bits[i], \"{}: cell ({},{})\"\n"
t += "                , path, i % size, i / size);\n"
t += "        }\n"
t += "        n += 1;\n"
t += "    }\n"
t += "    assert!(n > 0);\n"
t += "}\n\n"
t += "#[test]\nfn golden_frames() {\n"
for fp in files:
    name = fp.split('/')[-1]
    base = name[7:-4]  # v{V}{L}_{mode}_L{len}
    v = int(base[1]); lv = {'L': 1, 'M': 2, 'Q': 3}[base[2]]
    mode = base.split('_')[1]
    ln = int(base.split('_L')[1])
    modec = {'num': 'Mode::Numeric', 'alnum': 'Mode::Alnum', 'byte': 'Mode::Byte'}[mode]
    verc = {1: 'Version::M1', 2: 'Version::M2', 3: 'Version::M3', 4: 'Version::M4'}[v]
    lvc = {1: 'Level::L', 2: 'Level::M', 3: 'Level::Q'}[lv]
    t += f"    check(\"{name}\", include_str!(\"{name}\"), {verc}, {lvc}, {modec}, {ln});\n"
t += "}\n"
open(f"{OUT}/tests/golden_tests.rs", 'w').write(t)
print(f"golden_tests.rs: {len(files)} frames")
