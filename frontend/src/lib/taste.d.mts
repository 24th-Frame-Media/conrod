export type Tasted = { id: number; taste?: number | null };

/** The frame's taste score, or null when this album has not been scored. */
export function tasteOf(frame: Tasted | null | undefined): number | null;

/** Highest taste first, unscored frames last, id as the stable tie-break. */
export function tasteOrder<F extends Tasted>(frames: F[]): F[];

/**
 * Stacks in taste order. Inside each stack the frames go by taste; the stacks
 * themselves are ordered by their lead frame's taste (the lead is unchanged).
 */
export function tasteStacks<S extends { lead: Tasted; frames: Tasted[]; key: string }>(stacks: S[]): S[];
