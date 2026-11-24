// A bounded cache of each album's newest progress payload. A long session opens many
// albums, and every identify snapshot carries a decider feed with recent answers, so an
// unbounded map only grows. Pure, so node --test can pin the eviction.

/** How many albums keep their last payload. The running one and the newest are never evicted. */
export const PROGRESS_KEEP = 6;

/**
 * Remember `payload` (keyed by `payload.job_id`) and evict the oldest FINISHED albums beyond
 * `limit`. `order` lists job ids oldest first. Returns the next `{ all, order }`; the inputs
 * are not changed.
 */
export function rememberProgress(all, order, payload, limit = PROGRESS_KEEP) {
  const id = payload.job_id;
  const nextOrder = [...order.filter((x) => x !== id), id];
  const next = { ...all, [id]: payload };
  let excess = nextOrder.length - Math.max(1, limit);
  const kept = [];
  for (const old of nextOrder) {
    // Oldest first: a finished, older album goes; the running one and this payload stay.
    if (excess > 0 && old !== id && next[old]?.state !== 'running') {
      delete next[old];
      excess -= 1;
    } else {
      kept.push(old);
    }
  }
  return { all: next, order: kept };
}
