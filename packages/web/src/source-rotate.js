/**
 * Source-level rotation for the diagonal retry.
 *
 * The mask-level ±45° pass (`scan_diagonal` in the wasm scanner) is cheap
 * but loses Aztec's bullseye when the camera frame has already been
 * resampled once (canvas draw + binarizer): a binary mask cannot be
 * rotated back into clean modules. Rotating the SOURCE with antialiasing
 * and binarizing again does decode — that is this helper's job.
 *
 * `rotateDrawable(source, deg)` draws the source rotated about its centre
 * into a canvas sized to the rotated bounding box and returns the canvas
 * plus `map(frameW, frameH, points)`, which maps decoded points from the
 * binarized frame back into the original source's pixel space.
 */

/** @param {CanvasImageSource | HTMLVideoElement} source */
const dims = (source) => {
  // Videos carry videoWidth/videoHeight; every other drawable width/height.
  const box = /** @type {{ width?: number, height?: number, videoWidth?: number, videoHeight?: number }} */ (
    /** @type {unknown} */ (source)
  );
  return {
    w: Number(box.width ?? box.videoWidth ?? 0),
    h: Number(box.height ?? box.videoHeight ?? 0),
  };
};

/**
 * @param {CanvasImageSource} source
 * @param {number} deg
 * @returns {Promise<{ canvas: OffscreenCanvas | HTMLCanvasElement, width: number, height: number, map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[] }>}
 */
export async function rotateDrawable(source, deg) {
  const { w: sw, h: sh } = dims(source);
  if (!sw || !sh) throw new TypeError("rotateDrawable: source has no dimensions");
  const rad = (deg * Math.PI) / 180;
  const cw = Math.ceil(Math.abs(sw * Math.cos(rad)) + Math.abs(sh * Math.sin(rad)));
  const ch = Math.ceil(Math.abs(sw * Math.sin(rad)) + Math.abs(sh * Math.cos(rad)));
  /** @type {OffscreenCanvas | HTMLCanvasElement} */
  const canvas =
    typeof OffscreenCanvas !== "undefined"
      ? new OffscreenCanvas(cw, ch)
      : Object.assign(document.createElement("canvas"), { width: cw, height: ch });
  const ctx = /** @type {CanvasRenderingContext2D | null} */ (canvas.getContext("2d"));
  if (!ctx) throw new TypeError("rotateDrawable: no 2d context");
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, cw, ch);
  // Same quality choice as the demo/tilt fixtures: high-quality smoothing
  // is what a real camera's optics + downscale produce anyway.
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.translate(cw / 2, ch / 2);
  ctx.rotate(rad);
  ctx.drawImage(/** @type {CanvasImageSource} */ (source), -sw / 2, -sh / 2, sw, sh);

  /**
   * Frame point → canvas point (the binarizer scales uniformly, rounded
   * per axis) → source point (inverse rotation about the canvas centre).
   * @param {number} fw @param {number} fh
   * @param {{x: number, y: number}[]} points
   */
  const map = (fw, fh, points) => {
    const sx = cw / fw;
    const sy = ch / fh;
    const cos = Math.cos(-rad);
    const sin = Math.sin(-rad);
    return points.map(({ x, y }) => {
      const X = x * sx - cw / 2;
      const Y = y * sy - ch / 2;
      return {
        x: cos * X - sin * Y + sw / 2,
        y: sin * X + cos * Y + sh / 2,
      };
    });
  };
  return { canvas, width: cw, height: ch, map };
}
