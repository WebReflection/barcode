# E-ink test-code checklist (human-driven)

Display: 2.7" 264x176 E-ink. Every PNG is exactly 264x176 —
upload as-is so one image pixel = one e-ink pixel (no scaling blur).

Per variant: upload → hold the phone ~10-15 cm, square-on → confirm the
result card shows the expected symbology + text AND the SVG re-presentation
appears → tick. The status line should read `gpu binarizer` throughout.
Re-test any failure once at a slightly different distance before marking it.

| # | file | expected | pass | notes |
|---|------|----------|------|-------|
| 1 | qr-v01-m.png | QRCode: HELLO WORLD | ☐ | |
| 2 | qr-v02-m.png | QRCode: https://example.com/ | ☐ | |
| 3 | qr-v03-m.png | QRCode: 4006381333931 | ☐ | |
| 4 | qr-v04-m.png | QRCode: CODE 39 $%*+-./: | ☐ | |
| 5 | qr-v05-m.png | QRCode: Grüße aus Wien | ☐ | |
| 6 | qr-v06-m.png | QRCode: https://example.com/p?a=1&b=2 | ☐ | |
| 7 | qr-v07-m.png | QRCode: https://example.com/very/long/path/for/version/seven/testing?with=query&params=1 | ☐ | |
| 8 | qr-v08-m.png | QRCode: 1234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890 | ☐ | |
| 9 | qr-v09-m.png | QRCode: MILESTONE FIVE VERSION NINE ON THE E-INK TEST SHEET | ☐ | |
| 10 | qr-v10-m.png | QRCode: https://example.com/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/v10/ | ☐ | |
| 11 | qr-v11-m.png | QRCode: QR VERSION ELEVEN ON THE E-INK TEST SHEET FOR THE CAMERA | ☐ | |
| 12 | qr-v12-m.png | QRCode: 123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890 | ☐ | |
| 13 | qr-v13-m.png | QRCode: https://example.com/version-thirteen/test?sheet=eink&camera=phone | ☐ | |
| 14 | qr-v14-m.png | QRCode: QR VERSION FOURTEEN - THE LARGEST THAT FITS 264X176 AT 2PX PER MODULE | ☐ | |
| 15 | qr-v05-l.png | QRCode: EC LEVEL LOW ON VERSION FIVE | ☐ | |
| 16 | qr-v05-q.png | QRCode: EC LEVEL QUARTILE ON VERSION FIVE | ☐ | |
| 17 | qr-v05-h.png | QRCode: EC LEVEL HIGH ON VERSION FIVE | ☐ | |
| 18 | qr-v10-l.png | QRCode: EC LEVEL LOW ON VERSION TEN | ☐ | |
| 19 | qr-v10-q.png | QRCode: EC LEVEL QUARTILE ON VERSION TEN | ☐ | |
| 20 | qr-v10-h.png | QRCode: EC LEVEL HIGH ON VERSION TEN | ☐ | |
| 21 | ean13.png | ean13: 4006381333931 | ☐ | |
| 22 | ean8.png | ean8: 96385074 | ☐ | |
| 23 | upce.png | upce: 042100005264 | ☐ | |
| 24 | code39.png | code39: ABC-123 | ☐ | |
| 25 | code93.png | code93: TEST93 | ☐ | |
| 26 | code128.png | code128: CODE128 | ☐ | |
| 27 | itf.png | itf: 12345670 | ☐ | |
| 28 | codabar.png | codabar: 40156 | ☐ | |
| 29 | qr-v2-m-rot180.png | QRCode: https://example.com/ (rotated 180°) | ☐ | |
| 30 | qr-v3-m-rot90.png | QRCode: 4006381333931 (rotated 90°) | ☐ | |

## Angle matrix (after the base pass)

Repeat qr-v05-m.png and ean13.png at: 90° CW, 180°, 90° CCW (rotate the
IMAGE before upload), and phone-tilted ~15° in each of the four directions.
Expect: image rotations always pass (rotation-agnostic decoder); phone
tilt up to ~±15° passes (affine grid); beyond that, perspective work is
still pending — note the threshold angle where it breaks.
