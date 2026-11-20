/**
 * One source of truth for the fake-camera tier, shared by the CDP driver
 * (tools/e2e/fakecam.mjs, dev servers + real phones) and the Playwright
 * driver (tools/e2e/playwright.mjs, CI + WebKit/iPhone profiles).
 */

/** Tracked known failures: XFAIL, not a red suite. Keep reasons specific. */
export const XFAIL = new Map([]);
export const CASES = [
  ["code39", "ABC-123"],
  ["code128", "CODE128"],
  ["code32", "A938261563"],
  ["itf", "12345670"],
  ["codabar", "40156"],
  ["ean13", "4006381333931"],
  ["upca", "036000291452"],
  ["ean8", "96385074"],
  ["upce", "042100005264"],
  // DataBar decodes to the GS1 AI(01) + GTIN-14 form; the fixture text is
  // also the expected result, and the encoder accepts that form through
  // `encodableText` (the re-render path).
  ["databar", "(01)40063813339314"],
  ["databarltd", "(01)10000000000007"],
  ["databarstk", "(01)40063813339314"],
  ["databarexpanded", "(01)40063813339314(10)ABC"],
  // compressed encodations: (3103) weight, (3922) price, weight + date
  ["databarexpanded", "(01)90012345678908(3103)001234"],
  ["databarexpanded", "(01)90012345678908(3922)12345"],
  ["databarexpanded", "(01)90012345678908(3103)001234(15)261231"],
  ["code93", "TEST93"],
  ["qrcode", "HELLO WORLD"],
  ["microqr", "12345"],
  ["datamatrix", "DM-E2E-001"],
  ["pdf417", "PDF417-E2E"],
  ["aztec", "AZTEC-E2E"],
  // polarity pass (roadmap item 5): white-on-black specimens
  ["inverted-qrcode", "INVERTED-QR-E2E"],
  ["inverted-datamatrix", "INVERTED-DM-E2E"],
  ["inverted-aztec", "INVERTED-AZTEC-E2E"],
  ["inverted-pdf417", "INVERTED-PDF417-E2E"],
  ["inverted-code128", "INVERTED-CODE128"],
  // inverted codes on a WHITE page: only the ROI crop retry can read these
  ["island-qrcode", "ISLAND-QR-E2E"],
  ["island-code128", "ISLAND-C128-E2E"],
  // quarter-turn gate (sideways 1D, e.g. loyalty cards / receipts)
  ["rotated-code128", "ROTATED-CODE128"],
  // diagonal gate (source-rotation retry, real camera path)
  ["diagonal-aztec", "DIAGONAL-AZTEC"],
  // small-in-frame specimens: the full-frame downscale loses the module
  // pitch, the native-resolution ROI pass must recover it (roadmap item 8)
  ["tiny132-qrcode", "TINY-QR-ROI"],
  ["tiny352-code128", "TINY-CODE128-ROI"],
];

// The injected mock: reads the per-navigation ?e2e=symbology:text hint.
// Runs as a classic script before the page's modules register.
export const MOCK = `
(function () {
  // fakezoom=1: advertise a zoom capability on the canvas track so the
  // default UI's pinch handler is covered without camera hardware.
  const fakezoom = new URLSearchParams(location.search).get("fakezoom") === "1";
  let q = new URLSearchParams(location.search).get("e2e");
  if (!q) return;
  // tiny<NPX>-<symbology>:<text> — draw the specimen at NPX across
  // (small-in-frame; exercises the native-resolution ROI pass)
  let tinySize = 0;
  const tiny = q.match(/^tiny(\\d+)-/);
  if (tiny) {
    tinySize = Number(tiny[1]);
    q = q.slice(tiny[0].length);
  }
  // inverted-<symbology>:<text> — white-on-black specimen (polarity pass)
  let invert = false;
  if (q.startsWith("inverted-")) {
    invert = true;
    q = q.slice("inverted-".length);
  }
  // island-<symbology>:<text> — white-on-black specimen on a WHITE page.
  // The full-frame ink fraction stays low, so this is the case only the
  // ROI crop's inverted retry can read (a screenshot / photo with margins).
  let island = false;
  if (q.startsWith("island-")) {
    island = true;
    q = q.slice("island-".length);
  }
  // rotated-<symbology>:<text> — quarter-turned specimen (sideways 1D)
  let rotate = 0;
  if (q.startsWith("rotated-")) {
    rotate = 90;
    q = q.slice("rotated-".length);
  }
  // diagonal-<symbology>:<text> — 45° specimen (source-rotation retry)
  if (q.startsWith("diagonal-")) {
    rotate = 45;
    q = q.slice("diagonal-".length);
  }
  const sep = q.indexOf(":");
  const symbology = q.slice(0, sep), text = q.slice(sep + 1);
  navigator.mediaDevices.getUserMedia = async function () {
    // The page's own encoder: demo/scan.js exposes the same render()
    // consumers get, so the mock never needs a checkout path of its own.
    const pageRender = globalThis.__barcodeRender;
    if (!pageRender) throw new Error("fixture renderer not exposed by the page");
    const node = await pageRender({ type: symbology, value: text });
    if (!node) throw new Error("fixture render null");
    const img = new Image();
    img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(node.outerHTML);
    await img.decode();
    const vw = node.viewBox.baseVal.width, vh = node.viewBox.baseVal.height;
    // Snap to whole pixels per viewBox unit (1 module): crisp synthetic
    // renders at non-integer module sizes are a known decoder worst
    // case (notes/STATUS.md aztec hardening note) that real cameras'
    // optical blur never produces.
    // A phone camera is PORTRAIT and the SDK's default viewfinder is the
    // centre SQUARE of the frame. Draw the specimen inside 90% of that
    // square (fractional modules: that is what a camera at a distance
    // actually produces). A landscape fixture here used to overflow the
    // square, so wide 1D/PDF417 were cropped away before reaching the
    // binarizer — a mock artifact, not a decoder failure.
    const camW = tinySize ? 3072 : 1080;
    const camH = tinySize ? 2304 : 1920;
    const side = Math.min(camW, camH) * 0.9;
    // Crisp whole-pixel modules whenever they fit the square (the decoder's
    // best case); only an oversize symbol is resampled down. Fractional
    // module edges everywhere made inverted Aztec flaky.
    let scale = Math.max(1, Math.floor(side / Math.max(vw, vh)));
    if (Math.max(vw, vh) * scale > side) scale = Math.min(side / vw, side / vh);
    const w = tinySize || Math.max(1, Math.round(vw * scale));
    const h = tinySize ? Math.max(1, Math.round(tinySize * (vh / vw))) : Math.max(1, Math.round(vh * scale));
    const canvas = document.createElement("canvas");
    if (rotate) {
      // room for the rotated bounding box: 90° needs max(w,h), 45° needs
      // (w+h)/sqrt2 — take whichever is larger, with a margin
      const side = Math.ceil(Math.max(w, h, (w + h) / Math.SQRT2) * 1.1 + 20);
      canvas.width = side; canvas.height = side;
    } else {
      canvas.width = camW; canvas.height = camH;
    }
    const cx = canvas.getContext("2d");
    const x0 = Math.round((canvas.width - w) / 2), y0 = Math.round((canvas.height - h) / 2);
    const draw = () => {
      // Always render the fixture normally first (dark ink on white),
      // then optionally turn it and/or invert the RGBA.
      cx.fillStyle = "#fff";
      cx.fillRect(0, 0, canvas.width, canvas.height);
      // Normal fixtures use whole-module scales (crisp). A tiny fixture
      // is deliberately fractional, so use the same high-quality resample
      // a camera produces instead of nearest-neighbour aliasing.
      // smoothing only matters when an oversize symbol had to be resampled
      cx.imageSmoothingEnabled = !Number.isInteger(scale);
      cx.imageSmoothingQuality = "high";
      if (rotate) {
        cx.save();
        cx.translate(canvas.width / 2, canvas.height / 2);
        cx.rotate((rotate * Math.PI) / 180);
        cx.drawImage(img, -w / 2, -h / 2, w, h);
        cx.restore();
      } else {
        cx.drawImage(img, x0, y0, w, h);
      }
      if (invert) {
        const d = cx.getImageData(0, 0, canvas.width, canvas.height);
        for (let i = 0; i < d.data.length; i += 4) {
          d.data[i] = 255 - d.data[i];
          d.data[i + 1] = 255 - d.data[i + 1];
          d.data[i + 2] = 255 - d.data[i + 2];
        }
        cx.putImageData(d, 0, 0);
      } else if (island) {
        // white-on-black specimen, white page around it
        const d = cx.getImageData(x0, y0, w, h);
        for (let i = 0; i < d.data.length; i += 4) {
          d.data[i] = 255 - d.data[i];
          d.data[i + 1] = 255 - d.data[i + 1];
          d.data[i + 2] = 255 - d.data[i + 2];
        }
        cx.putImageData(d, x0, y0);
      }
    };
    draw();
    const stream = canvas.captureStream(15);
    const timer = setInterval(draw, 66); // keep frames flowing at ~15fps
    stream.getVideoTracks().forEach((t) => t.addEventListener("ended", () => clearInterval(timer)));
    if (fakezoom) {
      // A real canvas track has no zoom constraint. Patch THIS instance so
      // Scanner.zoomTo()/currentZoom() see the same contract a phone
      // exposes; frames themselves are unchanged (this gate is only about
      // the gesture -> camera wiring, not optics).
      const track = stream.getVideoTracks()[0];
      const caps = track.getCapabilities?.() ?? {};
      const settings = track.getSettings.bind(track);
      let zoom = 1;
      Object.defineProperties(track, {
        getCapabilities: { value: () => ({ ...caps, zoom: { min: 1, max: 8, step: 0.1 } }) },
        getSettings: { value: () => ({ ...settings(), zoom }) },
        applyConstraints: {
          value: async (/** @type {any} */ constraints) => {
            const next = constraints?.advanced?.find((/** @type {any} */ c) => c.zoom != null)?.zoom;
            if (next != null) zoom = Math.min(8, Math.max(1, next));
          },
        },
      });
    }
    return stream;
  };
})();
`;
