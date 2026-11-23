// Ordering guard for run snapshot streams (status polls, hydration reads, and live events).
//
// The bug: a slow `status` / `scan_status` poll resolves AFTER a newer pushed
// live event, but `setStatus` / the `at` comparison was unconditional (and `at`
// was stamped with the RESOLVE time, not the REQUEST time), so the stale poll
// won. That produced the false "Scan complete" toast and reordered jobs, and
// drew a snapshot older than the event in hand.
//
// The fix, in one place:
//  * every update is stamped with a monotonic logical time at REQUEST/EMIT time;
//  * a strictly newer stamp always wins, an older stamp NEVER wins;
//  * on an exact tie a pushed live event outranks a poll/hydration (the `>` that
//    used to drop a same-millisecond live event);
//  * the clock is monotonic, so a backward wall-clock step / NTP correction can
//    never freeze a snapshot again.
//
// Pure, with no imports, so `node --test` can pin every branch and so both the
// engine status stream and the scan_status/scan-progress streams share it.

/** Source precedence when two updates carry the same logical stamp. */
export const SOURCE = Object.freeze({ poll: 0, hydrate: 1, live: 2 });

/**
 * A monotonic logical clock. `now(wall)` returns a number that tracks wall time
 * but never goes backwards: after a backwards system-clock step it keeps rising
 * by one. Pass the wall time captured at REQUEST time for a poll so its stamp
 * stays low while it is in flight.
 */
export function makeClock() {
  let last = 0;
  return {
    now(wall) {
      const t = Number.isFinite(wall) ? wall : Date.now();
      last = Math.max(t, last + 1);
      return last;
    },
    peek() { return last; },
  };
}

/** The one clock the whole app uses, so `at` is comparable across every stream. */
export const clock = makeClock();

/**
 * Does `incoming` replace `current`? Both are `{ at, source, seq? }`.
 *  - no current state yet: yes;
 *  - a newer request/emit stamp: yes, an older one (slow poll): no;
 *  - equal stamp: higher source precedence wins (live > hydrate > poll);
 *  - same source and stamp: the last arrival wins.
 */
export function orderWins(current, incoming) {
  if (!current) return true;
  if (!incoming) return false;
  const a = Number.isFinite(incoming.at) ? incoming.at : -Infinity;
  const b = Number.isFinite(current.at) ? current.at : -Infinity;
  if (a !== b) return a > b;
  const sa = SOURCE[incoming.source] ?? 0;
  const sb = SOURCE[current.source] ?? 0;
  if (sa !== sb) return sa > sb;
  return (incoming.seq ?? 0) >= (current.seq ?? 0);
}

/** The winner of two stamped updates (`incoming` when it wins, else `current`). */
export function applySnapshot(current, incoming) {
  return orderWins(current, incoming) ? incoming : current;
}
