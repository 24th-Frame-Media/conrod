// Composition labels for the review payload's `comp_*` fields. Pure, so node tests reach the same thresholds the UI shows.
// Nothing here is a cull: a label describes how a frame sits in its frame. Cross-frame claims are relative only: a stack's
// own median background. A frame with none of the fields measured returns no labels at all (never a "not measured" one).

const MODES = { centre: 'Centre', thirds: 'Thirds', phi: 'Golden ratio' };
/** How far off the frame's centre line a heavy side may sit and still read as balanced. */
const BALANCE_DEAD = 0.15;

const label = (id, text, tone) => ({ id, text, tone });

/**
 * Where the subject sits on the frame's composition grid. Below 0.5 it sits off every anchor, which is most moving
 * subjects and not worth a chip: silence instead of an "Off the grid" warning on every frame.
 */
function placement(frame) {
  const p = frame.comp_placement;
  if (p == null || p < 0.5) return null;
  const mode = MODES[frame.comp_mode];
  if (p >= 0.75) return label('placement', mode ?? 'Well placed', 'good');
  return label('placement', mode ?? 'Framing average', 'ok');
}

/** Which way the frame leans; both axes inside the dead zone is the only "Balanced". */
function balance(frame) {
  const x = frame.comp_balance_x, y = frame.comp_balance_y;
  if (x != null && y != null && Math.abs(x) <= BALANCE_DEAD && Math.abs(y) <= BALANCE_DEAD) return [label('balance', 'Balanced', 'good')];
  const out = [];
  if (x != null && Math.abs(x) > BALANCE_DEAD) out.push(label('balance-x', x < 0 ? 'Heavy left' : 'Heavy right', 'ok'));
  if (y != null && Math.abs(y) > BALANCE_DEAD) out.push(label('balance-y', y < 0 ? 'Top heavy' : 'Bottom heavy', 'ok'));
  return out;
}

/** A busy background is worth a warning and a clean one worth a tick; the middle is not worth a chip. */
function clutter(frame) {
  const c = frame.comp_clutter;
  if (c == null) return null;
  if (c <= 0.55) return label('clutter', 'Clean background', 'good');
  if (c >= 0.85) return label('clutter', 'Busy background', 'warn');
  return null;
}

function symmetry(frame) {
  const s = frame.comp_symmetry;
  return s != null && s >= 0.7 ? label('symmetry', 'Symmetric', 'good') : null;
}

/** One to three lines lead the eye; more than that is a tangle. None is simply no label. */
function leadingLines(frame) {
  const n = frame.comp_lines;
  if (n == null || n < 1) return null;
  if (n <= 3) return label('lines', `${n} leading line${n === 1 ? '' : 's'}`, 'good');
  return label('lines', 'Cluttered lines', 'warn');
}

/** Every label the frame earns, in a fixed order: placement, balance, background, symmetry, leading lines. */
export function compositionLabels(frame) {
  if (!frame) return [];
  return [
    placement(frame),
    ...balance(frame),
    clutter(frame),
    symmetry(frame),
    leadingLines(frame),
  ].filter(Boolean);
}

/**
 * A stack's own background baseline: the median clutter of the frames that have a value, and the ids a tenth
 * (and at least 0.05) below it. Relative because a crowded track lifts every absolute value: three measured
 * frames minimum, or one low outlier would be called cleanest by default.
 */
export function stackClutter(frames) {
  const list = frames ?? [];
  const values = list.map((f) => f.comp_clutter).filter((c) => c != null).sort((a, b) => a - b);
  if (!values.length) return { median: null, cleanestIds: [] };
  const mid = Math.floor(values.length / 2);
  const median = values.length % 2 ? values[mid] : (values[mid - 1] + values[mid]) / 2;
  if (values.length < 3) return { median, cleanestIds: [] };
  const cleanestIds = list
    .filter((f) => f.comp_clutter != null && f.comp_clutter <= median * 0.9 && f.comp_clutter <= median - 0.05)
    .map((f) => f.id);
  return { median, cleanestIds };
}

/** True when a frame's background is at least a tenth calmer than its stack's median; the strip's "Cleaner" chip. */
export function relativeCleaner(median, clutter) {
  return median != null && clutter != null && clutter <= median * 0.9;
}

/**
 * The frame with the calmest background among those that have one measured: `{ id, clutter }`, or null.
 * The strip marks it only when `stackClutter` names cleanest frames, so one value never wins by default.
 */
export function panComparison(frames) {
  let best = null;
  for (const frame of frames ?? []) {
    if (frame.comp_clutter == null) continue;
    if (best == null || frame.comp_clutter < best.clutter) best = { id: frame.id, clutter: frame.comp_clutter };
  }
  return best;
}
