// The learned taste order: pure, so node tests can reach it. `taste` is 0..1 from
// the engine's head; it is a ranking signal inside a burst, never a keeper pick.

/** The frame's taste score, or null when this album has not been scored. */
export const tasteOf = (frame) => (typeof frame?.taste === 'number' && Number.isFinite(frame.taste) ? frame.taste : null);

/** Highest taste first, unscored frames last, id as the stable tie-break. */
export function tasteOrder(frames) {
  return [...frames].sort((a, b) => {
    const ta = tasteOf(a), tb = tasteOf(b);
    if (ta != null && tb != null && ta !== tb) return tb - ta;
    if ((ta != null) !== (tb != null)) return ta != null ? -1 : 1;
    return a.id - b.id;
  });
}

/**
 * Stacks in taste order. Inside each stack the frames go by taste; the stacks
 * themselves are ordered by their lead frame's taste (the lead is unchanged --
 * it is still the keeper when the burst has one). Unscored leads sort last.
 */
export function tasteStacks(stacks) {
  return stacks
    .map((stack) => ({ ...stack, frames: tasteOrder(stack.frames) }))
    .sort((a, b) => {
      const ta = tasteOf(a.lead), tb = tasteOf(b.lead);
      if (ta != null && tb != null && ta !== tb) return tb - ta;
      if ((ta != null) !== (tb != null)) return ta != null ? -1 : 1;
      return String(a.key).localeCompare(String(b.key));
    });
}
