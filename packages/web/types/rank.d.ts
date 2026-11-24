/**
 * Center-priority ranking for decode results.
 *
 * The default UI draws a horizontal line across the middle of the
 * viewfinder: that line — and the center point it crosses — is what the
 * user is aiming with, so the decoder must honour the same promise. A flat
 * "distance from the center" score is not enough: on a packed label a
 * corner QR 130px above the center still scored 0.77-0.82 and outranked
 * (and, through the worker's early exits, suppressed) the barcode under
 * the line.
 *
 * The score is therefore tiered by *reticle overlap*:
 *
 *   Tier A — the frame center point is inside the symbol's quad
 *            (`0.85 .. 1.00`): the code under the reticle.
 *   Tier B — the quad crosses the horizontal center scan line (or the
 *            vertical axis for a vertically oriented 1D strip), but not
 *            the center point (`0.45 .. 0.78`).
 *   Tier C — surrounding / peripheral (`0 .. 0.42`), steeply decaying.
 *
 * The floors do not overlap and `CENTERED_SCORE` sits at Tier A's floor
 * (`0.85`), so a non-centered Tier B symbol can never trigger centered
 * early exits or 2-frame centered confirmation. Tiering is a *ranking*,
 * never a filter: a peripheral code is still returned, it just loses the
 * race. The public contract (`score` 1 = dead center, 0 = frame corner;
 * bigger symbol breaks ties) is unchanged.
 */
/** @typedef {{ position: Array<{x: number, y: number}>, score?: number }} Rankable */
/** Tier A floor: only a quad containing the frame center is "centered". */
export const CENTERED_SCORE: 0.85;
/** Tier C ceiling: surrounding codes never reach a centered tier. */
export const PERIPHERAL_CAP: 0.42;
export function rankCodes<T extends Rankable>(codes: T[], width: number, height: number): T[];
export function isCentered(code: Rankable | undefined): boolean;
export function hasCentered(codes: Rankable[]): boolean;
export type Rankable = {
    position: Array<{
        x: number;
        y: number;
    }>;
    score?: number;
};
