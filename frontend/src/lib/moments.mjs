// What a judge saw in a frame (the engine stores it as JSON on the image), split the way ranking splits it:
// a moment that reaches the album's threshold is sure and may lift the frame; below it, down to 0.5, it is
// only a "maybe". Pure, so node --test can import it.

export const DEFAULT_THRESHOLD = 0.85;
export const MAYBE_FLOOR = 0.5;

/** The stored verdict, or an empty one for anything unreadable. */
export function parseVerdict(stored) {
  const empty = { backend: '', readings: [], strength: null, peak: null };
  if (typeof stored !== 'string' || !stored) return empty;
  try {
    const v = JSON.parse(stored);
    if (!v || typeof v !== 'object') return empty;
    const readings = Array.isArray(v.readings)
      ? v.readings.filter((r) => r && typeof r.tag === 'string' && Number.isFinite(r.p)).map((r) => ({ tag: r.tag, p: r.p, calibrated: Boolean(r.calibrated) }))
      : [];
    return { backend: String(v.backend ?? ''), readings, strength: Number.isFinite(v.strength) ? v.strength : null, peak: Number.isFinite(v.peak) ? v.peak : null };
  } catch { return empty; }
}

/** A threshold the engine would accept: 0.5 to 1, else the default. */
export function thresholdOf(value) {
  const n = Number(value);
  return Number.isFinite(n) && n >= MAYBE_FLOOR && n <= 1 ? n : DEFAULT_THRESHOLD;
}

/** The tag's plain name: the preset's label when it has one, else the tag made readable. */
export function labelOf(tag, vocabulary = []) {
  const known = vocabulary.find((m) => m.key === tag);
  if (known) return known.label;
  const words = String(tag).replace(/_/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Sure moments, maybes, and the peak flag of one frame. */
export function momentsOf(stored, vocabulary = [], threshold = DEFAULT_THRESHOLD) {
  const verdict = parseVerdict(stored);
  const bar = thresholdOf(threshold);
  const shape = (r) => ({ tag: r.tag, label: labelOf(r.tag, vocabulary), p: r.p, calibrated: r.calibrated });
  const sorted = [...verdict.readings].sort((a, b) => b.p - a.p);
  return {
    sure: sorted.filter((r) => r.p >= bar).map(shape),
    maybe: sorted.filter((r) => r.p >= MAYBE_FLOOR && r.p < bar).map(shape),
    peak: verdict.peak != null,
    backend: verdict.backend,
  };
}

/** Why a frame was lifted, in the photographer's words, or '' when nothing lifts it. `rankOn` is the album's switch;
 *  only a vocabulary tag counts, like the engine. */
export function liftNote(stored, vocabulary, threshold, rankOn) {
  if (!rankOn) return '';
  const { sure, peak } = momentsOf(stored, vocabulary, threshold);
  const counted = sure.filter((m) => vocabulary.some((v) => v.key === m.tag));
  const bits = [];
  if (counted.length) {
    const best = counted.reduce((a, b) => ((vocabulary.find((v) => v.key === b.tag)?.weight ?? 0) > (vocabulary.find((v) => v.key === a.tag)?.weight ?? 0) ? b : a));
    bits.push(best.label.toLowerCase());
  }
  if (peak) bits.push('peak of the burst');
  return bits.length ? `Lifted: ${bits.join(', ')}` : '';
}

/** One frame's confident tags, for the facet list. */
export function sureTags(stored, threshold = DEFAULT_THRESHOLD) {
  return momentsOf(stored, [], threshold).sure.map((m) => m.tag);
}

/** One line for a tooltip or chip: "First kiss 96%" and, when the judge is only a vision model, "(unverified)". */
export function describe(moment) {
  return `${moment.label} ${Math.round(moment.p * 100)}%${moment.calibrated ? '' : ' (unverified)'}`;
}
