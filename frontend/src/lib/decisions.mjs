/* Per-photo keep/cull decisions and the stack-wide ones built from them. Pure, so node --test can import it. */

/**
 * The mark values for one decision. Keep clears a reject and cull clears a pick, so the two flags stay exclusive;
 * clear resets both. The frame is accepted for context (the pill asks it which segment is on) and may be null.
 */
export function applyDecision(_frame, action) {
  if (action === 'keep') return { picked: true, rejected: false };
  if (action === 'cull') return { rejected: true, picked: false };
  if (action === 'clear') return { picked: false, rejected: false };
  throw new Error(`unknown decision: ${action}`);
}

/**
 * The one change list a bulk stack decision sends to `markMany`: `keepId` keeps that frame and culls every other
 * frame of the stack; `'cull'` culls all of them. Frames already decided are included: the values are idempotent,
 * and one undo entry should cover the whole stack either way.
 */
export function stackDecision(frames, keepId) {
  const list = frames ?? [];
  return list.map((frame) => ({
    id: frame.id,
    values: keepId !== 'cull' && frame.id === keepId ? applyDecision(frame, 'keep') : applyDecision(frame, 'cull'),
  }));
}

/** How many stacks have no undecided frame at all; a single-frame stack counts like any other. */
export function reviewedStacks(stacks, isUndecided) {
  const list = stacks ?? [];
  let reviewed = 0;
  for (const stack of list) if (!stack.frames.some(isUndecided)) reviewed += 1;
  return { reviewed, total: list.length };
}

/** "No 4★+" while the stack's best live frame is below four stars, null once one reaches it. */
export function lowStarBadge(stars) {
  return (stars ?? 0) >= 4 ? null : 'No 4★+';
}
