/**
 * Brighten a dim source so the binarizer's auto-levels gate does not
 * fire. Returns null for a normal-exposure or high-contrast frame (the
 * gate's own condition), so callers can skip the extra binarize.
 * @param {CanvasImageSource} source
 * @returns {Promise<{ canvas: OffscreenCanvas, factor: number } | null>}
 */
export function liftDim(source: CanvasImageSource): Promise<{
    canvas: OffscreenCanvas;
    factor: number;
} | null>;
