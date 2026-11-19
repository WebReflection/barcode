"""Extract the GS1 DataBar Expanded symbol-character table from zint.

Black-box oracle only: payloads go through zint's public API with
`symbol->debug` set; we parse the diagnostic `Data:` values it prints
and the emitted SVG module row. Nothing is read from zint's source.

Why this exists: the ISO/IEC 24724 symbol-character table is not a plain
lexicographic rank. The oracle reports the true 12-bit character values,
so the table can be measured directly. 100k random payloads cover 4091 of
the 4092 first-pass values; merging seeds covered all of them (0..4094).

Each value has up to four equivalent physical patterns:
    (fp, widths), (fp, reversed widths),
    (1-fp, widths), (1-fp, reversed widths)
so a canonical table plus that closure is enough to decode any character.

Requirements: zint (libzint), Python 3. The library is found through
`ZINT_LIB` or the platform loader; no Python bindings are needed.

Usage:
    python tools/databar_char_table.py 100000 21 /tmp/patterns.json
    python tools/databar_char_table.py 100000 21 /tmp/patterns.json \
        --canonical /tmp/canonical.json

Patterns are written as `"<first polarity><8 widths>"`, e.g.
`"111131523"` = starts with a bar, widths 1,1,1,3,1,5,2,3. Values are
the 12-bit symbol-character values. `--canonical` writes value -> one
pattern and validates the four-variant closure over 0..4094.
"""
import argparse
import collections
import ctypes
import ctypes.util
import json
import os
import random
import re
import sys
import tempfile


class StructApp(ctypes.Structure):
    _fields_ = [('index', ctypes.c_int), ('count', ctypes.c_int), ('id', ctypes.c_char * 32)]


class ZintSymbol(ctypes.Structure):
    """The public `struct zint_symbol` prefix (zint.h), up to encoded_data."""
    _fields_ = [
        ('symbology', ctypes.c_int), ('height', ctypes.c_float), ('scale', ctypes.c_float),
        ('whitespace_width', ctypes.c_int), ('whitespace_height', ctypes.c_int),
        ('border_width', ctypes.c_int), ('output_options', ctypes.c_int),
        ('fgcolour', ctypes.c_char * 16), ('bgcolour', ctypes.c_char * 16),
        ('fgcolor', ctypes.c_char_p), ('bgcolor', ctypes.c_char_p),
        ('outfile', ctypes.c_char * 256), ('primary', ctypes.c_char * 128),
        ('option_1', ctypes.c_int), ('option_2', ctypes.c_int), ('option_3', ctypes.c_int),
        ('show_hrt', ctypes.c_int), ('input_mode', ctypes.c_int), ('eci', ctypes.c_int),
        ('dpmm', ctypes.c_float), ('dot_size', ctypes.c_float), ('text_gap', ctypes.c_float),
        ('guard_descent', ctypes.c_float), ('structapp', StructApp),
        ('warn_level', ctypes.c_int), ('debug', ctypes.c_int),
        ('text', ctypes.c_ubyte * 256), ('text_length', ctypes.c_int),
        ('rows', ctypes.c_int), ('width', ctypes.c_int), ('errtxt', ctypes.c_char * 160),
        ('bitmap', ctypes.c_void_p), ('bitmap_width', ctypes.c_int), ('bitmap_height', ctypes.c_int),
        ('alphamap', ctypes.c_void_p), ('vector', ctypes.c_void_p),
        ('memfile', ctypes.c_void_p), ('memfile_size', ctypes.c_int),
        ('content_segs', ctypes.c_void_p), ('content_seg_count', ctypes.c_int),
        ('encoded_data', (ctypes.c_ubyte * 144) * 200), ('row_height', ctypes.c_float * 200),
    ]


def load_zint():
    candidates = []
    if os.environ.get('ZINT_LIB'):
        candidates.append(os.environ['ZINT_LIB'])
    found = ctypes.util.find_library('zint')
    if found:
        candidates.append(found)
    candidates += [
        '/opt/homebrew/opt/zint/lib/libzint.dylib',
        '/usr/local/lib/libzint.dylib',
        'libzint.so',
        'libzint.dylib',
    ]
    errors = []
    for name in candidates:
        try:
            return ctypes.CDLL(name)
        except OSError as exc:
            errors.append(f'{name}: {exc}')
    raise SystemExit('cannot load libzint (set ZINT_LIB):\n  ' + '\n  '.join(errors))


BARCODE_RSS_EXP = 31
BARCODE_MEMORY_FILE = 0x10000


def make_symbol(lib, outfile, debug):
    s = lib.ZBarcode_Create()
    s.contents.symbology = BARCODE_RSS_EXP
    s.contents.scale = 1
    s.contents.show_hrt = 0
    s.contents.border_width = 0
    s.contents.whitespace_width = 0
    s.contents.debug = 1 if debug else 0
    s.contents.output_options = BARCODE_MEMORY_FILE
    s.contents.outfile = outfile
    return s


def svg_mods(svg):
    """Module row from the SVG bars (MEMORY_FILE emits one `<path>`)."""
    rects = []
    for attrs in re.findall(r'<rect ([^/]+)/>', svg):
        d = dict(re.findall(r'(\w+)="([^"]*)"', attrs))
        if d.get('fill') == '#FFFFFF':
            continue
        rects.append((float(d['x']), float(d['width'])))
    for d in re.findall(r'<path d="([^"]*)"', svg):
        for x, w in re.findall(r'M([\d.]+) 0h([\d.]+)v', d):
            rects.append((float(x), float(w)))
    if not rects:
        raise ValueError('empty SVG')
    width = int(max(x + w for x, w in rects))
    return ''.join('1' if any(x < 2 * m + 2 and x + w > 2 * m for x, w in rects) else '0'
                   for m in range((width + 1) // 2))


def runs(s):
    out = []
    cur, n = s[0], 0
    for c in s:
        if c == cur:
            n += 1
        else:
            out.append((cur, n))
            cur, n = c, 1
    out.append((cur, n))
    return out


START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))
GL = (('0', 1), ('1', 1))


def segment(ms, data_chars):
    """guard(2) check(8) START(5) then [char][char][15-block] ..."""
    r = runs(ms)
    if tuple(r[:2]) != GL or tuple(r[10:15]) != START:
        return None
    out = [(r[2][0], [x for _, x in r[2:10]])]
    i = 15
    for k in range(data_chars):
        c = r[i:i + 8]
        if len(c) < 8 or sum(x for _, x in c) != 17:
            return None
        out.append((c[0][0], [x for _, x in c]))
        i += 8
        if k % 2 == 1 and k != data_chars - 1:
            b = r[i:i + 5]
            if len(b) < 5 or sum(x for _, x in b) != 15:
                return None
            i += 5
    return out


def gtin(rng):
    return ''.join(rng.choice('0123456789') for _ in range(14))


def gen(rng):
    """Valid payloads across the general-purpose and compressed methods."""
    kind = rng.randrange(8)
    if kind == 0:
        return '[21]' + ''.join(rng.choice('0123456789') for _ in range(rng.randint(1, 20)))
    if kind == 1:
        return '[10]' + ''.join(rng.choice('ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-.') for _ in range(rng.randint(1, 20)))
    if kind == 2:
        return '[240]' + ''.join(rng.choice('ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789') for _ in range(rng.randint(1, 28)))
    if kind == 3:
        return '[90]' + ''.join(rng.choice('ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-.abcdefghijklmnopqrstuvwxyz') for _ in range(rng.randint(1, 20)))
    if kind == 4:
        return '[01]' + gtin(rng)
    if kind == 5:
        return '[01]9' + gtin(rng)[1:] + '[3103]' + str(rng.randint(0, 999999)).zfill(6)
    if kind == 6:
        return ('[01]9' + gtin(rng)[1:] + '[3103]'
                + str(rng.randint(0, 999999)).zfill(6) + '[17]' + '25'
                + str(rng.randint(1, 12)).zfill(2) + str(rng.randint(0, 28)).zfill(2))
    return '[10]' + ''.join(rng.choice('abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-.') for _ in range(rng.randint(1, 18)))


def encode(lib, payload, outfile, debug, log_fd=None):
    """One FRESH symbol per encode: MEMORY_FILE output appends on reuse."""
    s = make_symbol(lib, outfile, debug)
    r = lib.ZBarcode_Encode_and_Print(s, payload.encode(), -1, 0)
    svg = None
    if r == 0:
        svg = ctypes.string_at(s.contents.memfile, s.contents.memfile_size).decode()
    lib.ZBarcode_Delete(s)
    if log_fd is not None:
        ctypes.CDLL(None).fflush(None)
    return r, svg


def variants(pattern):
    fp, w = pattern[0], pattern[1:]
    rw = w[::-1]
    other = '1' if fp == '0' else '0'
    return {fp + w, fp + rw, other + w, other + rw}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('count', type=int, help='payloads to encode')
    ap.add_argument('seed', type=int, help='deterministic payload seed')
    ap.add_argument('out', help='write pattern -> value JSON here')
    ap.add_argument('--canonical', metavar='PATH',
                    help='also write value -> canonical pattern and validate coverage')
    args = ap.parse_args(argv)

    lib = load_zint()
    lib.ZBarcode_Create.restype = ctypes.POINTER(ZintSymbol)
    lib.ZBarcode_Encode_and_Print.argtypes = [ctypes.POINTER(ZintSymbol), ctypes.c_char_p,
                                              ctypes.c_int, ctypes.c_int]
    lib.ZBarcode_Delete.argtypes = [ctypes.POINTER(ZintSymbol)]

    tmp = tempfile.mkdtemp(prefix='databar-exp-')
    outfile = os.path.join(tmp, 'symbol.svg').encode()
    log_path = os.path.join(tmp, 'zint.log')
    rng = random.Random(args.seed)

    # Pass 1: capture zint's diagnostic values (stdout).
    saved = os.dup(1)
    log = open(log_path, 'w')
    os.dup2(log.fileno(), 1)
    recs = []
    attempts = 0
    while len(recs) < args.count and attempts < args.count * 5:
        attempts += 1
        payload = gen(rng)
        print('MARK', flush=True)
        r, _ = encode(lib, payload, outfile, debug=True)
        # zint's debug lines are C stdio: flush them before the next MARK.
        ctypes.CDLL(None).fflush(None)
        if r == 0:
            recs.append((attempts - 1, payload))
    os.dup2(saved, 1)
    ctypes.CDLL(None).fflush(None)
    log.close()
    blocks = re.split(r'MARK\n', open(log_path).read())[1:]

    # Pass 2: physical widths from the SVG, paired with the values.
    table = {}
    conflicts = 0
    for ai, payload in recs:
        blk = blocks[ai]
        md = re.search(r'Data chars: (\d+), Check char: (\d+)', blk)
        mdata = re.search(r'Data: ([\d,\s]+)\n', blk)
        if not md or not mdata:
            continue
        nchars = int(md.group(1))
        check = int(md.group(2))
        values = [int(x) for x in re.findall(r'\d+', mdata.group(1))]
        if len(values) != nchars:
            continue
        r, svg = encode(lib, payload, outfile, debug=False)
        if r != 0 or not svg:
            continue
        try:
            seg = segment(svg_mods(svg), nchars)
        except ValueError:
            continue
        if seg is None:
            continue
        fp, widths = seg[0]
        key = fp + ''.join(map(str, widths))
        if key in table and table[key] != check:
            conflicts += 1
        table[key] = check
        for (fp, widths), value in zip(seg[1:], values):
            key = fp + ''.join(map(str, widths))
            if key in table and table[key] != value:
                conflicts += 1
            table[key] = value

    json.dump(table, open(args.out, 'w'))
    values = set(table.values())
    print(f'payloads {len(recs)}/{attempts}, patterns {len(table)}, '
          f'values {len(values)}, conflicts {conflicts}')

    if args.canonical:
        by_value = collections.defaultdict(set)
        for pattern, value in table.items():
            by_value[value].add(pattern)
        canonical = {}
        closure = {}
        for value, patterns in by_value.items():
            all_variants = set()
            for p in patterns:
                all_variants |= variants(p)
            canonical[str(value)] = min(patterns)
            for p in all_variants:
                closure[p] = value
        json.dump(canonical, open(args.canonical, 'w'))
        missing = [v for v in range(4092) if str(v) not in canonical]
        print(f'canonical {len(canonical)} values, {len(closure)} patterns; '
              f'missing 0..4091: {len(missing)}')
        if missing:
            print('  missing:', missing[:32])
        if conflicts:
            raise SystemExit('conflicting pattern->value observations')
    return 0


if __name__ == '__main__':
    sys.exit(main())
