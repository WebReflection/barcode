"""DataBar Expanded probe 15: the first data character vs AI.

Collect c1 (element widths, bars/spaces split) for every AI zint accepts.
If c1 = f(AI) only, the map AI -> widths reveals the value chain start.
Also: 3-digit AIs, and same-AI different-data constancy.
"""
import sys

sys.path.insert(0, '/Users/ag/git/barcode/tools')
from pyzint import zint
from databar_probe import svg_module_string, runs

START = (('0', 1), ('1', 8), ('0', 4), ('1', 1), ('0', 1))


def m(payload: str) -> str:
    return svg_module_string(payload, zint.BARCODE_RSS_EXP)


def c1_of(p):
    s = m(p)
    r = runs(s)
    assert tuple(r[10:15]) == START, p
    return [x for _, x in r[15:23]]


def split(w):
    return (sum(w[0::2]), sum(w[1::2]))  # (bars, spaces) ink-first


two_digit = ["00", "01", "02", "04", "10", "11", "12", "13", "15", "16", "17",
             "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28",
             "29", "30", "31", "32", "33", "34", "35", "36", "37", "38", "39",
             "40", "41", "42", "49", "50", "51", "52", "53", "54", "55", "56",
             "57", "58", "59", "60", "61", "62", "63", "64", "65", "66", "67",
             "68", "69", "70", "71", "72", "73", "74", "75", "76", "77", "78",
             "79", "80", "81", "82", "83", "84", "85", "86", "87", "88", "89",
             "90", "91", "92", "93", "94", "95", "96", "97", "98", "99"]
seen = {}
for ai in two_digit:
    # data chosen per AI family; many AIs have fixed-length fields, so try a
    # few shapes until one renders
    datas = ["12345", "1", "123456", "1234", "123", "12", "12345678901234"]
    got = None
    for d in datas:
        try:
            got = c1_of(f"[{ai}]{d}")
            break
        except Exception:
            continue
    if got is not None:
        seen[ai] = got

print("== AI -> c1 widths (split) ==")
for ai in sorted(seen):
    w = seen[ai]
    print(f"  {ai}: {w} split={split(w)}")

# constancy check
for ai in ["21", "10", "17"]:
    a = c1_of(f"[{ai}]12345")
    b = c1_of(f"[{ai}]99999")
    print(f"constancy {ai}: {a == b}")

# 3-digit AIs
for ai in ["235", "240", "241", "242", "243", "244", "250", "251", "252",
           "253", "254", "255", "256", "257", "258", "259", "260", "261",
           "262", "263", "264", "265", "266", "267", "268", "269",
           "3103", "3203", "3303", "3910", "3920", "3930", "3900",
           "7001", "7002", "7003"]:
    try:
        w = c1_of(f"[{ai}]" + "123")
        print(f"  {ai}: {w} split={split(w)}")
    except Exception as e:
        pass
