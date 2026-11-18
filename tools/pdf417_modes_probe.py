"""PDF417 mode interpreter probe: TC/BC/NC segments -> bytes.

Spec: 2.2.4.4 (TC), 2.2.4.5 (BC: 901/924 latches, 913 shift, 6 bytes
in 5 codewords base 256->900, partial tail 1 byte/codeword),
2.2.4.6 (NC: 902 latch, groups of 44 digits prefixed with leading 1,
base 10->900, 15 codewords per 44-digit group).

Latches (900-928) can never be segment DATA (BC codewords < 900 from
6*255 < 900^5; NC < 900; TC max 29*30+29 = 899), so segmenting is
unambiguous.
"""

def _bc_group(cws):
    """6 bytes from 5 codewords (base 900 -> base 256)."""
    v = 0
    for c in cws:
        v = v * 900 + c
    out = []
    for _ in range(6):
        out.append(v % 256)
        v //= 256
    return out[::-1]


def decode_bc(seg, latch):
    """Byte segment -> bytes. latch 924: len%6==0; 901: partial tail."""
    if latch == 924:
        assert len(seg) % 5 == 0, seg
        groups = [seg[i:i + 5] for i in range(0, len(seg), 5)]
        return bytes(b for g in groups for b in _bc_group(g))
    n = len(seg)
    r = ((n - 1) % 5) + 1  # partial tail size (1..5); a 6-byte group
    # would have meant total%6==0 and the encoder used 924
    full, tail = seg[:n - r], seg[n - r:]
    out = bytearray()
    for i in range(0, len(full), 5):
        out += _bc_group(full[i:i + 5])
    out += bytes(tail)
    return bytes(out)


def _nc_group(cws):
    """Digits from one base-900 group (leading 1 stripped)."""
    v = 0
    for c in cws:
        v = v * 900 + c
    s = str(v)
    assert s[0] == "1", s
    return s[1:]


def decode_nc(seg):
    out = []
    i = 0
    while len(seg) - i >= 15:
        out.append(_nc_group(seg[i:i + 15]))
        i += 15
    if i < len(seg):
        out.append(_nc_group(seg[i:]))
    return "".join(out).encode()


def interpret(cws):
    """Data codewords (post-descriptor, pre-pads) -> payload bytes.

    TC state (sub-mode + pending shift) persists across 913 shifts.
    921 (reader init), 925-927 (GLI) and macro codewords are skipped
    (GLI operands consumed; macro structure parked for later).
    """
    out = bytearray()
    sub = 0          # TC sub-mode: 0 alpha, 1 lower, 2 mixed, 3 punct
    from pdf417_text_probe import TABLE3

    def cell(sub, sc):
        return TABLE3["alpha lower mixed punct".split()[sub]][sc]

    shift_back = None
    i = 0
    while i < len(cws):
        cw = cws[i]
        if cw in (900, 921):
            sub, shift_back = 0, None
        elif cw in (901, 924):
            j = i + 1
            while j < len(cws) and cws[j] < 900:
                j += 1
            out += decode_bc(cws[i + 1:j], cw)
            i = j
            continue
        elif cw == 902:
            j = i + 1
            while j < len(cws) and cws[j] < 900:
                j += 1
            out += decode_nc(cws[i + 1:j])
            i = j
            continue
        elif cw == 913:
            out.append(cws[i + 1])
            i += 2
            continue
        elif 925 <= cw <= 927:
            i += {925: 1, 926: 2, 927: 1}[cw] + 1
            continue
        elif cw in (922, 923, 928):
            raise ValueError("macro control block not implemented")
        else:
            for sc in (cw // 30, cw % 30):
                if shift_back is not None:
                    c = cell(sub, sc)  # shifted char lands in the TARGET
                    sub = shift_back   # ... then revert
                    shift_back = None
                else:
                    c = cell(sub, sc)
                    if isinstance(c, str):
                        if c == "ll":
                            sub = 1
                        elif c == "ml":
                            sub = 2
                        elif c == "pl":
                            sub = 3
                        elif c == "al":
                            sub = 0
                        elif c == "ps":
                            shift_back, sub = sub, 3
                        elif c == "as":
                            shift_back, sub = sub, 0
                        continue
                out.append(c)
        i += 1
    return bytes(out)
