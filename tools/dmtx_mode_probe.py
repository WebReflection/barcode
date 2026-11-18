"""Mode-probe helper: encode payloads with zint, read the codeword stream
through the validated placement, RS-validate it, print the data codewords.

One line per payload; dims always printed (zint may resize on overflow).
RS validation guards against broken reads: a bad placement/split cannot
pass. Reuses rs_encode from the 144x144 derivation script.

Usage:
  python3 tools/dmtx_mode_probe.py --opt2 13 'HELLO WORLD' 'A\\x80B'
  python3 tools/dmtx_mode_probe.py --hex 418042 2a4141414141
  python3 tools/dmtx_mode_probe.py --sizes          # list single-block sizes
"""

import json
import sys

sys.path.insert(0, "tools")
exec(open("tools/dmtx_oracle.py").read().split("if __name__")[0])
import dmtx_truth as T
from dmtx_placement_solve import region_grid, sweep, read_back
from dmtx_144_derive import rs_encode

STRUCT = "crates/barcode-dm/testdata/dm_structure.json"


def load_spec(R, C):
    spec = json.load(open(STRUCT))[f"{R}x{C}"]
    return spec


def read_stream(zm, R, C, total):
    n_bits = 8 * total
    grids = region_grid(R - 2, C - 2, n_bits)
    assert len(grids) == 1, grids
    kr, kc, sr, sc, _ = grids[0]
    mr, mc = kr * sr, kc * sc

    def to_sym(r, c, sr=sr, sc=sc):
        return (1 + r + 2 * (r // sr), 1 + c + 2 * (c // sc))

    positions = sweep(mr, mc, zm, to_sym, [], 0)
    return read_back(zm, positions, total)


def rs_validate(stream, spec):
    """True if every block's parity recomputes exactly (ported from
    crates/barcode-dm/src/deinterleave.rs split_blocks)."""
    b, d, nn = spec["blocks"], spec["data"], spec["ecc"]
    r = d % b
    blocks = [[] for _ in range(b)]
    for i, v in enumerate(stream):
        if i < d:
            blocks[(i + b - r) % b].append(v)
        else:
            blocks[(i - d) % b].append(v)
    for bl in blocks:
        if rs_encode(bl[:-nn], nn) != bl[-nn:]:
            return False
    return True


def probe(payload, opt2=None):
    zm, R, C = T.encode_matrix(payload, 0, 0, opt2)
    spec = load_spec(R, C)
    stream = read_stream(zm, R, C, spec["total"])
    ok = rs_validate(stream, spec)
    tag = "RS-OK " if ok else "RS-FAIL"
    print(f"{tag} {R}x{C} D={spec['data']} "
          f"data={stream[:spec['data']]}")
    if not ok:
        print(f"       full stream: {stream}")
    assert ok, "stream failed RS validation - broken read, do not trust"


def list_sizes():
    for key, spec in sorted(json.load(open(STRUCT)).items(),
                            key=lambda kv: kv[1]["total"]):
        mark = "" if spec["blocks"] == 1 else "  (multi-block)"
        print(f"{key:>8}  blocks={spec['blocks']} data={spec['data']:>4} "
              f"ecc={spec['ecc']:>3}{mark}")


def parse(arg, hexmode):
    if hexmode:
        return bytes.fromhex(arg)
    return arg.encode().decode("unicode_escape").encode("latin-1")


if __name__ == "__main__":
    args = sys.argv[1:]
    if args and args[0] == "--sizes":
        list_sizes()
        sys.exit(0)
    hexmode = "--hex" in args
    args = [a for a in args if a != "--hex"]
    opt2 = None
    if "--opt2" in args:
        i = args.index("--opt2")
        opt2 = int(args[i + 1])
        del args[i:i + 2]
    for a in args:
        probe(parse(a, hexmode), opt2)
