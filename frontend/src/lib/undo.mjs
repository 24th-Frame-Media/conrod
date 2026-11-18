/* Undo and redo for culling decisions. Pure, so node --test can import it. */

/**
 * The values that put a frame back the way it was, for only the keys the change touched:
 * stars go back to the manual rating (or none), reject and pick to their flags.
 */
export function capture(frame, values) {
  const restore = {};
  if ('stars' in values) restore.stars = frame?.manual_stars ?? null;
  if ('rejected' in values) restore.rejected = Boolean(frame?.rejected);
  if ('picked' in values) restore.picked = Boolean(frame?.picked);
  return restore;
}

/** Keep at most `limit` entries; the newest is last. */
export function record(past, entry, limit = 100) {
  const next = [...(past ?? []), entry];
  return next.length > limit ? next.slice(next.length - limit) : next;
}

/** Take the newest entry off `past` and park it on `future` for redo. */
export function undo(past, future) {
  if (!past?.length) return null;
  const entry = past[past.length - 1];
  return { entry, past: past.slice(0, -1), future: record(future, entry) };
}

/** Put the newest undone entry back, for both lists. */
export function redo(past, future) {
  if (!future?.length) return null;
  const entry = future[future.length - 1];
  return { entry, past: record(past, entry), future: future.slice(0, -1) };
}
