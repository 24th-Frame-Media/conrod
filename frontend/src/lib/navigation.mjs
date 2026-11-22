/* ← / → across stack edges. Pure, so node --test can import it. */

/**
 * The page a Library card opens on. A scan that is live, waiting, stopped or
 * failed opens its worker screen; everything else (including a finished scan
 * with unreadable frames) opens Review, where the retry lives.
 */
export function openJobPage(job, activeJob = null) {
  if (!job) return 'Review';
  if (activeJob === job.id) return 'Scan';
  return ['queued', 'scanning', 'stopped', 'error'].includes(job.status) ? 'Scan' : 'Review';
}

/**
 * The first stack after `from` in `direction` (+1 forward, -1 back) that holds at least one frame,
 * or null at the end of the album. Empty stacks are skipped; there is no wrap.
 */
export function nextStackIndex(stacks, from, direction) {
  const list = Array.isArray(stacks) ? stacks : [];
  const step = direction < 0 ? -1 : 1;
  const base = (Number.isInteger(from) ? from : -1) + step;
  const start = step > 0 ? Math.max(0, Math.min(list.length, base)) : Math.min(list.length - 1, base);
  for (let i = start; i >= 0 && i < list.length; i += step) if (list[i]?.frames?.length) return i;
  return null;
}

/**
 * One arrow press inside stack `from`. Inside the stack it returns `{ id, expanded }`: the frame to
 * focus, with the stack open (shooting order is untouched). At an edge with `crossStacks` it returns
 * `{ stackIndex }` for the neighbouring non-empty stack to enter; otherwise null and the key does nothing.
 */
export function stepInStack(stacks, from, id, direction, { crossStacks = false } = {}) {
  const list = Array.isArray(stacks) ? stacks : [];
  const frames = list[from]?.frames ?? [];
  const at = frames.findIndex((frame) => frame.id === id);
  if (at < 0) return null;
  const next = at + (direction === 'back' ? -1 : 1);
  if (next >= 0 && next < frames.length) return { id: frames[next].id, expanded: true };
  if (!crossStacks) return null;
  const stackIndex = nextStackIndex(list, from, direction === 'back' ? -1 : 1);
  return stackIndex == null ? null : { stackIndex };
}

/** The frame an edge `{ stackIndex }` lands on: the stack's first going forward, its last going back. */
export function edgeFrameId(stack, direction) {
  const frames = stack?.frames ?? [];
  return direction === 'back' ? frames[frames.length - 1]?.id ?? null : frames[0]?.id ?? null;
}
