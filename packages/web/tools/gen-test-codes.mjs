#!/usr/bin/env node
/**
 * Generate the E-ink test-code corpus: one 264x176 PNG per variant
 * (QR versions 1-14, EC-level variants, all 1D symbologies, two
 * pre-rotated QRs), plus manifest.json and MANIFEST.md — the human-driven
 * verification checklist.
 *
 * Usage: node tools/gen-test-codes.mjs [cdpPort]   (default 9333)
 * Requires: the headless Chrome with the freshly built wasm, and vite.
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const PORT = process.argv[2] ?? "9333";
const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../test-codes");
const SHEET_W = 264, SHEET_H = 176;

// ---- variant corpus -------------------------------------------------------
// QR payloads sized so each version's smallest fitting version is the
// labeled one (byte mode unless noted). EC M unless noted.
const QR = [
  { v: 1, ec: 0, text: "HELLO WORLD" },
  { v: 2, ec: 0, text: "https://example.com/" },
  { v: 3, ec: 0, text: "4006381333931" },
  { v: 4, ec: 0, text: "CODE 39 $%*+-./:" },
  { v: 5, ec: 0, text: "Grüße aus Wien" },
  { v: 6, ec: 0, text: "https://example.com/p?a=1&b=2" },
  { v: 7, ec: 0, text: "https://example.com/very/long/path/for/version/seven/testing?with=query&params=1" },
  { v: 8, ec: 0, text: "1234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890" },
  { v: 9, ec: 0, text: "MILESTONE FIVE VERSION NINE ON THE E-INK TEST SHEET" },
  { v: 10, ec: 0, text: "https://example.com/" + "v10/".repeat(40) },
  { v: 11, ec: 0, text: "QR VERSION ELEVEN ON THE E-INK TEST SHEET FOR THE CAMERA" },
  { v: 12, ec: 0, text: "1234567890".repeat(15) },
  { v: 13, ec: 0, text: "https://example.com/version-thirteen/test?sheet=eink&camera=phone" },
  { v: 14, ec: 0, text: "QR VERSION FOURTEEN - THE LARGEST THAT FITS 264X176 AT 2PX PER MODULE" },
  { v: 5, ec: 1, text: "EC LEVEL LOW ON VERSION FIVE" },
  { v: 5, ec: 3, text: "EC LEVEL QUARTILE ON VERSION FIVE" },
  { v: 5, ec: 2, text: "EC LEVEL HIGH ON VERSION FIVE" },
  { v: 10, ec: 1, text: "EC LEVEL LOW ON VERSION TEN" },
  { v: 10, ec: 3, text: "EC LEVEL QUARTILE ON VERSION TEN" },
  { v: 10, ec: 2, text: "EC LEVEL HIGH ON VERSION TEN" },
];
const EC_NAME = { 0: "m", 1: "l", 2: "h", 3: "q" };

// 1D payloads kept short: the sheet allows (264 - 20 quiet) / module = 2px.
const D1 = [
  { sym: "ean13", text: "4006381333931", module: 2, height: 90 },
  { sym: "ean8", text: "96385074", module: 2, height: 90 },
  { sym: "upce", text: "042100005264", module: 2, height: 90 },
  { sym: "code39", text: "ABC-123", module: 2, height: 70 },
  { sym: "code93", text: "TEST93", module: 2, height: 70 },
  { sym: "code128", text: "CODE128", module: 2, height: 70 },
  { sym: "itf", text: "12345670", module: 2, height: 70 },
  { sym: "codabar", text: "40156", module: 2, height: 70 },
];

const VARIANTS = [
  ...QR.map((q) => ({
    kind: "qr",
    file: `qr-v${String(q.v).padStart(2, "0")}-${EC_NAME[q.ec]}.png`,
    symbology: "qrcode",
    text: q.text,
    module: Math.max(2, Math.min(
      Math.floor(SHEET_W / (17 + 4 * q.v + 12)),
      Math.floor(SHEET_H / (17 + 4 * q.v + 12)),
    )),
    height: 0,
    expect: `QRCode: ${q.text}`,
  })),
  ...D1.map((d) => ({
    kind: "1d",
    file: `${d.sym}.png`,
    symbology: d.sym,
    text: d.text,
    module: d.module,
    height: d.height,
    expect: `${d.sym}: ${d.text}`,
  })),
  // Two pre-rotated QRs: rotation handling via content.
  { kind: "rot", file: "qr-v2-m-rot180.png", symbology: "qrcode", text: "https://example.com/", module: 4, height: 0, deg: 180,
    expect: "QRCode: https://example.com/ (rotated 180°)" },
  { kind: "rot", file: "qr-v3-m-rot90.png", symbology: "qrcode", text: "4006381333931", module: 4, height: 0, deg: 90,
    expect: "QRCode: 4006381333931 (rotated 90°)" },
];

// ---- drive the headless chrome --------------------------------------------
const targets = await fetch(`http://127.0.0.1:${PORT}/json`).then((r) => r.json());
const page = targets.find((t) => t.type === "page");
if (!page) throw new Error("no page target; is headless chrome running on " + PORT + "?");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r));
let id = 0;
const pend = new Map();
ws.addEventListener("message", (ev) => {
  const d = JSON.parse(ev.data);
  if (d.id && pend.has(d.id)) {
    const p = pend.get(d.id);
    pend.delete(d.id);
    d.error ? p.reject(new Error(JSON.stringify(d.error))) : p.resolve(d.result);
  }
});
const send = (m, p = {}, timeoutMs = 300000) =>
  new Promise((res, rej) => {
    const mid = ++id;
    const to = setTimeout(() => rej(new Error("TO " + m)), timeoutMs);
    pend.set(mid, { resolve: (v) => { clearTimeout(to); res(v); }, reject: e => { clearTimeout(to); rej(e); } });
    ws.send(JSON.stringify({ id: mid, method: m, params: p }));
  });

await send("Page.enable");
await send("Page.navigate", { url: "http://localhost:5173/test.html" });
await new Promise((r) => setTimeout(r, 8000));

// Generate in-page: encode each variant, center it on the 264x176 sheet,
// return PNG data URLs. Batched in one evaluate to amortize round trips.
const WASM_MODULE = "/@fs" + process.cwd() + "/src/wasm/pkg/barcode_wasm.js";
const EXPR = `(async () => {
  const { encode_fixture_opts } = await import(${JSON.stringify(WASM_MODULE)});
  await (await import(${JSON.stringify(WASM_MODULE)})).default();
  const VARIANTS = ${JSON.stringify(VARIANTS)};
  const out = [];
  for (const v of VARIANTS) {
    const enc = encode_fixture_opts(v.symbology, v.text, v.module, v.height || 320, -1, -1, 0, -1, 0, 0, 0);
    const c = document.createElement("canvas");
    c.width = ${SHEET_W}; c.height = ${SHEET_H};
    const ctx = c.getContext("2d");
    ctx.fillStyle = "#fff"; ctx.fillRect(0, 0, c.width, c.height);
    // fixture mask -> ImageData (ink = black, paper = white)
    const spec = document.createElement("canvas");
    spec.width = enc.width; spec.height = enc.height;
    const sctx = spec.getContext("2d");
    const img = sctx.createImageData(enc.width, enc.height);
    for (let y = 0; y < enc.height; y++) for (let x = 0; x < enc.width; x++) {
      const ink = (enc.data[y * enc.bytesPerRow + (x >> 3)] >> (x & 7)) & 1;
      const o = (y * enc.width + x) * 4, val = ink ? 0 : 255;
      img.data[o] = img.data[o+1] = img.data[o+2] = val; img.data[o+3] = 255;
    }
    sctx.putImageData(img, 0, 0);
    let draw = spec;
    if (v.kind === "rot") {
      const r = document.createElement("canvas");
      r.width = spec.width; r.height = spec.height;
      const rctx = r.getContext("2d");
      rctx.translate(spec.width / 2, spec.height / 2);
      rctx.rotate((v.deg * Math.PI) / 180);
      rctx.drawImage(spec, -spec.width / 2, -spec.height / 2);
      draw = r;
    }
    const scale = Math.min(1, (c.width - 8) / draw.width, (c.height - 8) / draw.height);
    const w = Math.round(draw.width * scale), h = Math.round(draw.height * scale);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(draw, Math.floor((c.width - w) / 2), Math.floor((c.height - h) / 2), w, h);
    out.push({ file: v.file, png: c.toDataURL("image/png") });
  }
  return JSON.stringify(out);
})()`;

const r = await send("Runtime.evaluate", { expression: EXPR, returnByValue: true, awaitPromise: true, timeout: 300000 });
if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? "evaluate failed");
const pngs = JSON.parse(r.result.value);

fs.mkdirSync(OUT, { recursive: true });
for (const { file, png } of pngs) {
  fs.writeFileSync(path.join(OUT, file), Buffer.from(png.split(",")[1], "base64"));
}

// manifest.json + MANIFEST.md
const manifest = VARIANTS.map((v) => ({
  file: v.file,
  symbology: v.symbology,
  expected: v.expect,
  modulePx: v.module ?? null,
}));
fs.writeFileSync(path.join(OUT, "manifest.json"), JSON.stringify(manifest, null, 2));

const lines = [
  "# E-ink test-code checklist (human-driven)",
  "",
  `Display: 2.7" 264x176 E-ink. Every PNG is exactly ${SHEET_W}x${SHEET_H} —`,
  "upload as-is so one image pixel = one e-ink pixel (no scaling blur).",
  "",
  "Per variant: upload → hold the phone ~10-15 cm, square-on → confirm the",
  "result card shows the expected symbology + text AND the SVG re-presentation",
  "appears → tick. The status line should read `gpu binarizer` throughout.",
  "Re-test any failure once at a slightly different distance before marking it.",
  "",
  "| # | file | expected | pass | notes |",
  "|---|------|----------|------|-------|",
];
manifest.forEach((m, i) => {
  lines.push(`| ${i + 1} | ${m.file} | ${m.expected.replace(/\|/g, "/")} | ☐ | |`);
});
lines.push("", "## Angle matrix (after the base pass)", "",
  "Repeat qr-v05-m.png and ean13.png at: 90° CW, 180°, 90° CCW (rotate the",
  "IMAGE before upload), and phone-tilted ~15° in each of the four directions.",
  "Expect: image rotations always pass (rotation-agnostic decoder); phone",
  "tilt up to ~±15° passes (affine grid); beyond that, perspective work is",
  "still pending — note the threshold angle where it breaks.", "");
fs.writeFileSync(path.join(OUT, "MANIFEST.md"), lines.join("\n"));

console.log(`generated ${pngs.length} PNGs + manifest.json + MANIFEST.md in ${OUT}`);
process.exit(0);
