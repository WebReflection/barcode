/** Families cheap enough for every live frame (measured on a phone). */
export const CHEAP_2D: string[];
/** Every supported family, in the wasm's canonical order. */
export const ALL_FORMATS: string[];
/** The linear (1D) tier families. */
export const LINEAR: string[];
export function is2DResult(symbology: string): boolean;
export function needsScheduling(formats: string[] | undefined): boolean;
export function cheapFormats(formats: string[] | undefined, allowLinear?: boolean): string[];
export function dueForFullSweep(now: number): boolean;
export function markFullSweep(now: number): void;
