"""PDF417 text compaction probe: spec Table 3 + zint oracle verification.

Table 3 source: USS-PDF417 (public domain) page 5, read visually from a
250-dpi render (/tmp/spec_table3_top.png, /tmp/spec_table3_bottom.png)
because the PDF's OCR text layer drops/mangles code columns (verified
against pypdf extraction). Every cell reachable by a test payload is
then verified against zint oracle streams, which is the real authority.

Session rule: oracle patterns/values flow through JSON or generated
code, never retyped by hand.
"""

import json

from pyzint import zint

from pdf417_probe import module_runs, raw_rows, split_cells

TABLE_JSON = "/tmp/pdf417_table.json"

SUBS = ("alpha", "lower", "mixed", "punct")

# ---------------------------------------------------------------- Table 3
# {sub-mode: {value: ascii_code or mnemonic}} — transcribed from the
# rendered spec image, then cell-verified via oracle streams below.
TABLE3 = {
    "alpha": {
        **{v: 65 + v for v in range(26)}, 26: 32, 27: "ll", 28: "ml", 29: "ps",
    },
    "lower": {
        **{v: 97 + v for v in range(26)}, 26: 32, 27: "as", 28: "ml", 29: "ps",
    },
    "mixed": {
        **{v: 48 + v for v in range(10)}, 10: 38, 11: 13, 12: 9, 13: 44,
        14: 58, 15: 35, 16: 45, 17: 46, 18: 36, 19: 47, 20: 43, 21: 37,
        22: 42, 23: 61, 24: 94, 25: "pl", 26: 32, 27: "ll", 28: "al", 29: "ps",
    },
    "punct": {
        0: 59, 1: 60, 2: 62, 3: 64, 4: 91, 5: 92, 6: 93, 7: 95, 8: 96,
        9: 126, 10: 33, 11: 13, 12: 9, 13: 44, 14: 58, 15: 10, 16: 45,
        17: 46, 18: 36, 19: 47, 20: 34, 21: 124, 22: 42, 23: 40, 24: 41,
        25: 63, 26: 123, 27: 125, 28: 39, 29: "al",
    },
}

# Figure 5: which switch codes exist in which sub-mode.
SWITCH = {
    "alpha": {"lower": "ll", "mixed": "ml", "punct": "ps"},          # ps = shift
    "lower": {"alpha": "as", "mixed": "ml", "punct": "ps"},          # as = shift
    "mixed": {"lower": "ll", "alpha": "al", "punct": "pl"},          # all latches
    "punct": {"alpha": "al"},
}

# Shift edges (revert after one char): target -> source -> mnemonic.
SHIFT_TO = {
    "punct": {"alpha": "ps", "lower": "ps", "mixed": "ps"},
    "alpha": {"lower": "as"},
}


class Coder:
    def __init__(self, table=TABLE3):
        self.table = table
        self.base = "alpha"
        self.rev = {  # ascii -> value per sub-mode
            sub: {v: k for k, v in tab.items() if isinstance(v, int)}
            for sub, tab in table.items()
        }
        self.code = {  # mnemonic -> value per sub-mode
            sub: {v: k for k, v in tab.items() if isinstance(v, str)}
            for sub, tab in table.items()
        }

    def value_of(self, ch, sub):
        return self.rev[sub].get(ord(ch))

    def encode(self, payload):
        """Sub-mode state machine -> list of text codewords (base-30 pairs)."""
        subs = []
        cur = "alpha"  # latch to TC lands in Alpha (spec 2.2.4.4.2)
        for i, ch in enumerate(payload):
            v = self.value_of(ch, cur)
            if v is not None:
                subs.append(v)
                continue
            nxt = payload[i + 1] if i + 1 < len(payload) else ""
            path, target, is_shift = self._choose(cur, ch, nxt)
            base = cur
            for mnem in path:
                subs.append(self.code[cur][mnem])
                cur = self.apply(cur, mnem)
            subs.append(self.value_of(ch, cur))
            if is_shift:
                cur = base  # shifts revert to the pre-shift sub-mode
        if len(subs) % 2:
            subs.append(29)  # pad value (spec: odd count -> 29)
        return [30 * subs[j] + subs[j + 1] for j in range(0, len(subs), 2)]

    def apply(self, cur, mnem):
        """Sub-mode after emitting switch `mnem` from `cur`."""
        for dest, m in SWITCH[cur].items():
            if m == mnem:
                return dest
        for dest, srcs in SHIFT_TO.items():
            if srcs.get(cur) == mnem:
                return dest
        raise ValueError(mnem)

    def _choose(self, cur, ch, nxt):
        """zint's observed policy:
        1. if the next char is in a target containing ch -> latch there
           (SP counts: it is in every sub-mode column)
        2. elif a shift to a target containing ch exists -> shift
        3. else latch to the first target containing ch.
        Returns (switch_mnemonic_path, target, is_shift)."""
        targets = [T for T in SUBS if self.value_of(ch, T) is not None]
        if nxt:
            for T in targets:
                if self.value_of(nxt, T) is not None:
                    return self.latch_path(cur, T), T, False
        for T in targets:
            m = SHIFT_TO.get(T, {}).get(cur)
            if m:
                return [m], T, True
        for T in targets:
            path = self.latch_path(cur, T)
            if path:
                return path, T, False
        raise ValueError(f"no route for {ch!r} from {cur}")

    def latch_path(self, cur, dest):
        """Shortest latch-only switch path cur -> dest (BFS; ps/as excluded)."""
        if cur == dest:
            return []
        from collections import deque

        q = deque([(cur, [])])
        while q:
            node, path = q.popleft()
            for m in {v for v in SWITCH[node].values() if v not in ("ps", "as")}:
                nxt = self.apply(node, m)
                if nxt == dest:
                    return path + [m]
                q.append((nxt, path + [m]))
        return None


# ---------------------------------------------------------------- oracle


def load_pattern_table():
    t = json.load(open(TABLE_JSON))
    return {int(k): {p: int(v) for v, p in t[k].items()} for k in t}


def data_stream(text, cols=0, ecc=0):
    """Zint-encode `text`; return the full data-region codeword stream
    (length descriptor, data, pads, ecc) read row-major from the BMP."""
    z = zint.Zint(text, zint.BARCODE_PDF417, scale=1, show_text=False,
                  border_width=0, whitespace_width=0,
                  option_1=ecc, option_2=cols)
    bmp = z.render_bmp()
    w, h, get_row = raw_rows(bmp)
    mw = w // 2
    assert (mw - 18) % 17 == 0, f"width {mw} not a pdf417 row"
    ncols = (mw - 18) // 17 - 3
    # band height: rows within a codeword row are identical; find the
    # first change point scanning from the top of the image
    top = lambda y: tuple(get_row(h - 1 - y))
    BH = next(y for y in range(1, h) if top(y) != top(0))
    nrows = h // BH
    assert nrows * BH == h and nrows >= 1, (h, BH)

    # cumulative cell ends: start(17) left(17) data*nc right(17) stop(18)
    bounds = [17, 34]
    for _ in range(ncols):
        bounds.append(bounds[-1] + 17)
    bounds += [bounds[-1] + 17]  # right indicator
    bounds.append(bounds[-1] + 18)  # stop
    assert bounds[-1] == mw, (bounds, mw)

    inv = load_pattern_table()
    stream = []
    for b in range(nrows):
        bits = get_row(h - (b + 1) * BH + BH // 2)  # top-down band middle
        runs = module_runs(bits)
        cells = split_cells(runs, bounds)
        cluster = (b % 3) * 3
        for ci in range(2, 2 + ncols):
            elems, fd = cells[ci]
            assert fd, "data cell must start with a bar"
            v = inv[cluster].get("".join(map(str, elems)))
            assert v is not None, f"unmatched pattern band {b} col {ci}"
            stream.append(v)
    return stream


def split_stream(stream):
    """(descriptor, data_plus_pads, ecc): descriptor = 1 + #data + #pads."""
    desc = stream[0]
    return desc, stream[1:desc], stream[desc:]


# ---------------------------------------------------------------- verify


def check(coder, payload, **opts):
    stream = data_stream(payload, **opts)
    desc, data_pads, ecc = split_stream(stream)
    mine = coder.encode(payload)
    ok = data_pads[: len(mine)] == mine
    pads = data_pads[len(mine):]
    tag = "OK " if ok else "DIFF"
    print(f"{tag} {payload!r:22} oracle={data_pads} mine={mine} pads={pads}")
    if not ok:
        for j, (a, b) in enumerate(zip(data_pads, mine)):
            if a != b:
                print(f"      first diff at cw {j}: oracle {a} vs mine {b}")
                break
        if len(data_pads) != len(mine):
            print(f"      length: oracle {len(data_pads)} vs mine {len(mine)}")
    return ok and all(p == 900 for p in pads)


if __name__ == "__main__":
    coder = Coder()
    payloads = ["A1", "A", "HELLO WORLD 99", "lower case", "MiXeD 42!",
                "punct-only!", "a b c", "A$b", "ABC123",
                "aXb", "aXbc", "aXbYd", "aXYb", "MiXeD", "aX", "Xa", "aX9",
                "aD b", "aD 4", "aX b", "aDb", "aX9b", "aX 9", "MiXeD 4",
                "a$b", "a:b", "a*b", "a!b", "a!!b", "a!!", "a$$b", "a^b",
                "a:", "a!4", "a$4", "a4$b", "a!!c", "ab!cd", "a!b!c"]
    all_ok = all(check(coder, p) for p in payloads)
    print("ALL OK" if all_ok else "DIFFS FOUND")
