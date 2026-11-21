// What Conrod does for a shoot type, as the photographer sees it. The engine owns the policy
// (`preset_policy`); this is the pure logic around its answer: applying an album's switches,
// the plain-language notes, and which Review sidebar tabs the shoot can fill. Pure, so node --test can import it.
import policies from './presetPolicies.mjs';

/** Detector and reader keys, as the engine stores them in the album's `overrides`. */
export const DETECT_KEYS = ['vehicles', 'people', 'faces', 'animals'];
export const READ_KEYS = ['numbers', 'plates', 'make_model', 'sponsors', 'names'];
/** The moment judge's two switches: spot moments, and let them lift ranking. */
export const MOMENT_KEYS = ['moments_detect', 'moments_rank'];

const rowsOf = (summary) => [...summary.detects, ...summary.reads, summary.moments.detect, summary.moments.rank];

/** The summary the browser keeps for each preset's defaults (generated from the Rust table). */
export function defaultSummary(preset) {
  const id = typeof preset === 'string' ? preset.trim().toLowerCase() : '';
  return Object.hasOwn(policies, id) ? policies[id] : policies.mix;
}

/** The album's stored `overrides` as a plain `{key: boolean}`; anything else in it is ignored. */
export function overridesOf(stored) {
  const out = {};
  if (stored && typeof stored === 'object' && !Array.isArray(stored)) {
    for (const key of [...DETECT_KEYS, ...READ_KEYS, ...MOMENT_KEYS]) if (typeof stored[key] === 'boolean') out[key] = stored[key];
  }
  return out;
}

/**
 * The summary with these switches applied, the way the engine resolves them: the album's `overrides`, then the
 * photographer's own `user` defaults for the type, then the type's policy (a reader needs its detector).
 * A `user` default is what clearing the album's switch goes back to, so it becomes the row's `default`.
 */
export function applyOverrides(summary, overrides = {}, user = {}) {
  const mine = (key) => (typeof user[key] === 'boolean' ? user[key] : undefined);
  const raw = new Map(rowsOf(summary).map((r) => [r.key, typeof overrides[r.key] === 'boolean' ? overrides[r.key] : mine(r.key) ?? r.default]));
  const row = (r) => {
    const blocked = r.requires != null && raw.get(r.requires) !== true;
    return { ...r, default: mine(r.key) ?? r.default, on: raw.get(r.key) === true && !blocked, overridden: typeof overrides[r.key] === 'boolean', blocked };
  };
  return {
    ...summary,
    detects: summary.detects.map(row),
    reads: summary.reads.map(row),
    moments: { ...summary.moments, detect: row(summary.moments.detect), rank: row(summary.moments.rank) },
  };
}

/** The overrides after the photographer flips `row`: a switch back at the type's default stops being an override. */
export function nextOverrides(overrides, row, on) {
  const next = { ...overridesOf(overrides) };
  if (on === row.default) delete next[row.key];
  else next[row.key] = on;
  return next;
}

/** Whether the row's reader is on in this summary. */
export function isOn(summary, key) {
  return rowsOf(summary).some((r) => r.key === key && r.on);
}

const BLUR_WORDS = { critical: 'critical', strict: 'strict', moderate: 'moderate', lenient: 'lenient' };

/** The settings that are not switches, as short plain sentences. */
export function noteList(summary) {
  const notes = [];
  notes.push(summary.panForgiven ? 'Panning blur is forgiven' : 'Soft backgrounds are not forgiven');
  notes.push(`Blur check: ${BLUR_WORDS[summary.blur] ?? 'moderate'}`);
  if (summary.levelCheck) notes.push('Horizon and verticals checked');
  if (summary.darkOk) notes.push('Dark frames are fine');
  if (summary.deliverMost) notes.push('Keeps nearly everything');
  return notes;
}

/** The label of the row a blocked row waits for ("needs Cars & bikes"). */
export function needsLabel(summary, row) {
  if (!row.requires) return '';
  const need = rowsOf(summary).find((r) => r.key === row.requires);
  return need ? `needs ${need.label}` : '';
}

/** Sidebar tabs the shoot can fill, from what it reads: numbers, plates, makes, teams (sponsors), people, subjects, moments. */
export const FACET_ORDER = ['number', 'plate', 'make', 'team', 'person', 'subject', 'moment'];
export function facetsFromSummary(summary) {
  const on = (key) => isOn(summary, key);
  const wanted = {
    number: on('numbers'),
    plate: on('plates'),
    make: on('make_model'),
    team: on('sponsors'),
    person: on('names') || on('faces') || on('people'),
    subject: on('animals'),
    moment: on('moments_detect'),
  };
  return FACET_ORDER.filter((kind) => wanted[kind]);
}
