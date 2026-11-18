"""Extract the PDF417 bar-space pattern table from the public-domain
USS-PDF417 spec PDF (AIM USA), Appendix H Table H-1.

Provenance: the spec states the symbology is "entirely in the public
domain and free of all user restrictions, licences and fees" (the ISO
text says the same). Source PDF: USS-PDF-417 (AIM USA), cached at
/tmp/uss_pdf417.pdf, from apps.sceis.sc.gov (Appendix 6 USS-PDF-417).

Output: /tmp/pdf417_table.json  {cluster: {value: "b1s1b2s2b3s3b4s4"}}

Validation: every pattern is 8 digits 1-6 summing to 17 and satisfies
K = (b1 - b2 + b3 - b4 + 9) mod 9 == cluster. Six entries on pages 22
(values 11, 209, 257, 258, 269, 270) are OCR-split in the PDF text
layer and are repaired by hand; each repair re-validated.
"""

import json
import re
import sys

from pypdf import PdfReader

PDF = sys.argv[1] if len(sys.argv) > 1 else "/tmp/uss_pdf417.pdf"
OUT = sys.argv[2] if len(sys.argv) > 2 else "/tmp/pdf417_table.json"

# Pages hold one cluster each (verified: headers on 25 and 28; 22-24 are
# the cluster-0 block). 330+330+269 entries per cluster = 3x310-ish
# layout, 929 total.
PAGES = {0: [22, 23, 24], 3: [25, 26, 27], 6: [28, 29, 30]}

# OCR-split repairs (pattern, value) -- each re-validated below.
REPAIRS = {
    0: [(11, "11111615"), (209, "32122142"), (257, "22131422"),
        (258, "11222414"), (269, "21223232"), (270, "22132331")],
}


def cluster_k(pattern: str) -> int:
    b = [int(c) for c in pattern]
    return (b[0] - b[2] + b[4] - b[6] + 9) % 9


def main() -> None:
    r = PdfReader(PDF)
    tables = {0: {}, 3: {}, 6: {}}
    for k, pgs in PAGES.items():
        for i in pgs:
            text = r.pages[i].extract_text() or ""
            for pat, val in re.findall(r"(?<![\d])([1-6]{8})\s+(\d{1,3})(?![\d])", text):
                tables[k][int(val)] = pat
    for k, reps in REPAIRS.items():
        for v, p in reps:
            tables[k][v] = p

    for k in (0, 3, 6):
        t = tables[k]
        assert sorted(t) == list(range(929)), f"cluster {k}: values incomplete"
        for v, p in t.items():
            assert len(p) == 8 and sum(map(int, p)) == 17 and cluster_k(p) == k, (k, v, p)
        print(f"cluster {k}: 929 entries validated")

    json.dump(tables, open(OUT, "w"))
    print(f"saved {OUT}")


if __name__ == "__main__":
    main()
