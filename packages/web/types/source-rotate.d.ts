/**
 * @param {CanvasImageSource} source
 * @param {number} deg
 * @returns {Promise<{ canvas: OffscreenCanvas | HTMLCanvasElement, width: number, height: number, map: (fw: number, fh: number, points: {x: number, y: number}[]) => {x: number, y: number}[] }>}
 */
export function rotateDrawable(source: CanvasImageSource, deg: number): Promise<{
    canvas: OffscreenCanvas | HTMLCanvasElement;
    width: number;
    height: number;
    map: (fw: number, fh: number, points: {
        x: number;
        y: number;
    }[]) => {
        x: number;
        y: number;
    }[];
}>;
