// Pure Review logic: stacks, the one keeper concept, keyboard navigation targets and virtual-grid layout.
// Everything here works on plain frames and a `facts` map ({cull, pick} per frame id), so it runs under node tests.
import { filename, frameStars } from '../review.mjs';

const clamp = (n, lo, hi) => Math.max(lo, Math.min(hi, n));
const natural = (a, b) => filename(a.path ?? '').localeCompare(filename(b.path ?? ''), undefined, { numeric: true }) || a.id - b.id;

/** 'rejected' (by hand or auto-culled), 'picked' (by hand) or 'undecided'. A manual reject beats a pick, a pick beats an auto-cull. */
export function decisionOf(frame, fact) {
  if (frame.rejected) return 'rejected';
  if (frame.picked) return 'picked';
  if (fact?.cull) return 'rejected';
  return 'undecided';
}

/** A burst's keeper frames: the ones picked by hand if any, else the engine's burst pick (never a rejected frame). */
export function keepersOf(group, facts) {
  const picked = group.filter((f) => decisionOf(f, facts.get(f.id)) === 'picked');
  if (picked.length) return picked;
  return group.filter((f) => facts.get(f.id)?.pick && decisionOf(f, facts.get(f.id)) !== 'rejected');
}

/**
 * The size of the burst each suggested keeper heads: the engine's unaccepted pick, and how many frames it beat.
 * A burst already picked by hand has no suggestion left to make, so it is absent. Whole-album, like `keeperIds`.
 * A group of one is absent too: there is nothing to choose between, and the grid draws it as a plain frame.
 *
 * Deliberately no confidence number: the engine's pick is a total order over many signals, not a rating, so a
 * rating margin would be a made-up probability. "Best of 18" is the honest claim.
 */
export function suggestionSizes(frames, facts) {
  const out = new Map();
  for (const group of groupBy(frames, (f) => groupKeyOf(f)).values()) {
    if (group.length < 2) continue;
    for (const pick of keepersOf(group, facts).filter((f) => !f.picked)) out.set(pick.id, group.length);
  }
  return out;
}

/**
 * A stack holding one frame over the whole album (a burst of one, or a frame with no burst at all).
 * The grid draws it as the frame itself: no count, no suggestion or keeper pill, no strip to expand.
 * `total` is the album-wide size, so a filter that leaves one frame of a real burst visible keeps the stack.
 */
export function isSingleton(stack) {
  return (stack?.total ?? stack?.frames?.length ?? 0) <= 1;
}

/** Ids of the frames that sit alone in their burst, so the flat grid can leave their stack chrome off. */
export function singletonIds(frames) {
  const ids = new Set();
  for (const group of groupBy(frames, (f) => groupKeyOf(f)).values()) if (group.length <= 1) ids.add(group[0].id);
  return ids;
}

/**
 * Bursts where nothing is worth keeping: every frame is below three stars and the engine picked none.
 * Keyed by burst key, valued with the burst's size and its best star rating, so the grid can say
 * "No keeper" instead of leaving an unexplained gap where a green suggestion would be.
 *
 * Mirrors `conrod_vision::sharpness::KEEPER_MIN_RATING` (three stars): the engine has no keeper below it.
 */
export function softBursts(frames, facts) {
  const out = new Map();
  for (const group of groupBy(frames, (f) => groupKeyOf(f)).values()) {
    const key = group[0].burst_key;
    if (key == null || group.length < 2) continue;
    if (keepersOf(group, facts).length) continue;
    const live = group.filter((f) => decisionOf(f, facts.get(f.id)) !== 'rejected');
    if (!live.length) continue;
    const best = live.reduce((n, f) => Math.max(n, frameStars(f)), 0);
    if (best >= KEEPER_STARS) continue;
    out.set(key, { size: group.length, best });
  }
  return out;
}

/** The star rating a burst must reach for the engine to keep a frame of it. Mirrors the Rust constant. */
export const KEEPER_STARS = 3;

/** Stack key for a frame: its burst (or dupe set); a frame with no group is its own stack. */
export function groupKeyOf(frame, dupes = false) {
  const key = dupes ? frame.dupe_key : frame.burst_key;
  return key == null ? `frame-${frame.id}` : `${dupes ? 'dupe' : 'burst'}-${key}`;
}

function groupBy(frames, keyOf) {
  const map = new Map();
  for (const f of frames) {
    const key = keyOf(f);
    const list = map.get(key);
    if (list) list.push(f); else map.set(key, [f]);
  }
  return map;
}

/** Ids of every keeper in the album, burst by burst. Compute it on all frames, never on a filtered list. */
export function keeperIds(frames, facts) {
  const ids = new Set();
  for (const group of groupBy(frames, (f) => groupKeyOf(f)).values()) for (const f of keepersOf(group, facts)) ids.add(f.id);
  return ids;
}

const leadScore = (f) => (f.manual_stars ?? 0) * 10 + (Number.isFinite(f.rating) ? f.rating : 0) * 5;

/** The frame a collapsed stack shows: a keeper if there is one, else the best frame that is not rejected. */
export function leadOf(frames, keepers, facts) {
  if (keepers.length) return keepers[0];
  const live = frames.filter((f) => decisionOf(f, facts.get(f.id)) !== 'rejected');
  const pool = live.length ? live : frames;
  return pool.reduce((best, f) => (leadScore(f) > leadScore(best) ? f : best), pool[0]);
}

/** One stack from its visible frames. `keepers` is the album-wide keeper id set; `total` counts the frames the filter hides too. */
export function makeStack(key, frames, { facts, keepers, total }) {
  const ordered = [...frames].sort(natural);
  const keep = ordered.filter((f) => keepers.has(f.id));
  return { key, frames: ordered, keepers: keep, lead: leadOf(ordered, keep, facts), total: total ?? ordered.length };
}

/**
 * Burst stacks (or, with `dupes`, re-shoot stacks of 2+ frames) from the filtered frame list, in its order.
 * `all` is every frame of the album: it gives each stack its unfiltered size and decides which dupe sets are real.
 */
export function buildStacks(frames, { facts, keepers, all = frames, dupes = false }) {
  // A stack's key is its lowest frame id over the whole album, so a filter cannot rename it.
  const sizes = new Map(), lowest = new Map();
  for (const f of all) {
    const k = groupKeyOf(f, dupes);
    sizes.set(k, (sizes.get(k) ?? 0) + 1);
    lowest.set(k, Math.min(lowest.get(k) ?? f.id, f.id));
  }
  const stacks = [];
  for (const [key, list] of groupBy(frames, (f) => groupKeyOf(f, dupes))) {
    const size = sizes.get(key) ?? list.length;
    if (dupes && size < 2) continue;
    const id = lowest.get(key) ?? Math.min(...list.map((f) => f.id));
    stacks.push(makeStack(`${dupes ? 'd' : 'b'}${id}`, list, { facts, keepers, total: size }));
  }
  return stacks;
}

// ── navigation targets: every function returns a frame id (or null) and only ever lands on a frame in `stacks`/`frames`.

function locate(stacks, id) {
  for (let s = 0; s < stacks.length; s += 1) {
    const i = stacks[s].frames.findIndex((f) => f.id === id);
    if (i >= 0) return [s, i];
  }
  return null;
}

export const stackIndexOf = (stacks, id) => locate(stacks, id)?.[0] ?? -1;

/** Left/right: along the burst, stopping at its ends. */
export function stepInStack(stacks, id, delta) {
  const at = locate(stacks, id);
  if (!at) return null;
  const frames = stacks[at[0]].frames;
  return frames[clamp(at[1] + delta, 0, frames.length - 1)].id;
}

/** Up/down and Tab: to the previous/next burst, landing on its lead. At the first/last burst the selection stays. */
export function stepStack(stacks, id, delta) {
  if (!stacks.length) return null;
  const at = locate(stacks, id);
  if (!at) return stacks[delta < 0 ? stacks.length - 1 : 0].lead.id;
  const target = clamp(at[0] + delta, 0, stacks.length - 1);
  return target === at[0] ? id : stacks[target].lead.id;
}

/** Flat view, left/right. */
export function stepFlat(frames, id, delta) {
  if (!frames.length) return null;
  const i = frames.findIndex((f) => f.id === id);
  if (i < 0) return frames[delta < 0 ? frames.length - 1 : 0].id;
  return frames[clamp(i + delta, 0, frames.length - 1)].id;
}

/** Flat view, up/down: one row of `cols` cards. A short last row is reached by going down from the row above it. */
export function stepRow(frames, id, delta, cols) {
  if (!frames.length) return null;
  const i = frames.findIndex((f) => f.id === id);
  if (i < 0) return frames[delta < 0 ? frames.length - 1 : 0].id;
  const width = Math.max(1, cols);
  const row = Math.floor(i / width), lastRow = Math.floor((frames.length - 1) / width);
  const target = i + delta * width;
  if (delta > 0) return frames[target < frames.length ? target : row < lastRow ? frames.length - 1 : i].id;
  return frames[target >= 0 ? target : i].id;
}

/** The order the keeper-first review walks a stack: its lead, then the rest as shot. */
export const reviewOrder = (stack) => [stack.lead, ...stack.frames.filter((f) => f !== stack.lead)];

/**
 * N and auto-advance: the next frame (after `id`, in `seq`) that `undecided` accepts. With `wrap` the search
 * continues from the start; without it, running off the end returns null (the selection stays).
 */
export function nextUndecided(seq, id, undecided, { dir = 1, wrap = true } = {}) {
  const n = seq.length;
  const start = seq.findIndex((f) => f.id === id);
  if (start < 0) return (dir > 0 ? seq : [...seq].reverse()).find(undecided)?.id ?? null;
  for (let k = 1; k < n; k += 1) {
    const j = start + dir * k;
    if (!wrap && (j < 0 || j >= n)) return null;
    const f = seq[((j % n) + n) % n];
    if (undecided(f)) return f.id;
  }
  return null;
}

/** When a filter hides the selection: the nearest frame (forward first) of the previous list that is still visible. */
export function nearestVisible(prevIds, visible, id) {
  const i = prevIds.indexOf(id);
  if (i < 0) return null;
  for (let j = i + 1; j < prevIds.length; j += 1) if (visible.has(prevIds[j])) return prevIds[j];
  for (let j = i - 1; j >= 0; j -= 1) if (visible.has(prevIds[j])) return prevIds[j];
  return null;
}

// ── virtual stack grid: rows of `cols` cards, with the expanded stack's strip inserted below its row.

export function stackLayout({ count, cols, rowH, gap, pad, expandedIndex = -1, expandedH = 0 }) {
  const width = Math.max(1, cols);
  const rows = Math.ceil(count / width);
  const expandedRow = expandedIndex >= 0 && expandedIndex < count ? Math.floor(expandedIndex / width) : -1;
  const rowTop = (r) => pad + r * (rowH + gap) + (expandedRow >= 0 && r > expandedRow ? expandedH + gap : 0);
  const expandedTop = expandedRow >= 0 ? rowTop(expandedRow) + rowH + gap : -1;
  const height = rows ? rowTop(rows - 1) + rowH + pad : 0;
  return { cols: width, rows, rowH, expandedRow, expandedTop, expandedH, rowTop, height };
}

/** Rows [first, last) that touch the viewport, plus `overscan` rows each side. */
export function visibleRows(layout, scrollTop, viewH, overscan = 1) {
  let lo = 0, hi = layout.rows;
  while (lo < hi) { const mid = (lo + hi) >> 1; if (layout.rowTop(mid) + layout.rowH < scrollTop) lo = mid + 1; else hi = mid; }
  let last = lo;
  while (last < layout.rows && layout.rowTop(last) < scrollTop + viewH) last += 1;
  return [Math.max(0, lo - overscan), Math.min(layout.rows, last + overscan)];
}

/** New scrollTop that brings stack `index` (and its strip, if open) into view, or null when it already is. */
export function scrollTopFor(layout, index, scrollTop, viewH, pad) {
  const row = Math.floor(index / layout.cols);
  const top = layout.rowTop(row);
  const bottom = layout.expandedRow === row ? layout.expandedTop + layout.expandedH : top + layout.rowH;
  if (top < scrollTop) return Math.max(0, top - pad);
  if (bottom > scrollTop + viewH) return bottom - top + pad > viewH ? Math.max(0, top - pad) : bottom - viewH + pad;
  return null;
}
