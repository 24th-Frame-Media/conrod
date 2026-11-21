/* Which subjects identification reads (the suggested keeper of each burst, or everything) and the counts behind its labels. Pure, so node --test can import it. */
import { subjectExcluded } from '../review.mjs';

export const IDENTIFY_SCOPES = ['keepers', 'everything'];
export const DEFAULT_IDENTIFY_SCOPE = 'keepers';

/** The two choices, in the order the pickers list them: the same words in the shoot settings, the import sheet and the toolbar. */
export const IDENTIFY_SCOPE_OPTIONS = [
  { id: 'keepers', label: 'Keepers (suggested)', hint: 'The keeper you picked, or the suggested keeper of each burst.' },
  { id: 'everything', label: 'Everything', hint: 'Every subject, including frames you have not kept.' },
];

/** Whatever was stored (or nothing) becomes a scope the rest of the UI can switch on. */
export function identifyScopeOf(value) {
  return IDENTIFY_SCOPES.includes(value) ? value : DEFAULT_IDENTIFY_SCOPE;
}

/** The picker's words for a scope; an unknown value reads as the default. */
export function identifyScopeLabel(value) {
  const scope = identifyScopeOf(value);
  return IDENTIFY_SCOPE_OPTIONS.find((option) => option.id === scope).label;
}

/**
 * `Identify keepers (n)` or `Identify everything (n)`. The workflow mode has no say here: it decides whether
 * identification exists at all, the scope decides what it reads.
 */
export function identifyActionLabel(value, n) {
  const count = Number.isFinite(n) ? Number(n) : 0;
  return `${identifyScopeOf(value) === 'everything' ? 'Identify everything' : 'Identify keepers'} (${count.toLocaleString()})`;
}

const detsOf = (facts, id) => facts?.get?.(id)?.dets ?? [];
const isVehicle = (d) => (d.region_type ?? 'vehicle') === 'vehicle';
/** Has Identify read this subject? `read_at` is set by a read run and by Skip; `reviewed` is the photographer's acceptance. */
const isRead = (d) => d?.read_at != null;

/**
 * How many subjects identification would read on the album's effective keepers.
 *
 * `keepers` is `rv.keepers` (`keeperIds` -> `keepersOf`): the photographer's manual pick when a burst has one, the
 * engine's suggestion (`burst_pick` on the detection or its frame) otherwise. A manual pick therefore counts and
 * the suggestion it replaced does not, and a hand star that lost the pick counts nothing. This is never re-derived
 * from `burst_pick` here: the grid, the toolbar and the engine all answer the same question the same way.
 *
 * Only unread vehicle rows on a keeper frame count, and rows the cull excluded (rejected, bystander, an
 * automatic cull nobody starred) are skipped like the engine's query.
 */
export function keeperCount(frames, facts, keepers) {
  let count = 0;
  for (const frame of frames ?? []) {
    if (!keepers?.has?.(frame.id)) continue;
    for (const d of detsOf(facts, frame.id)) {
      if (isRead(d) || subjectExcluded(frame, d)) continue;
      if (isVehicle(d)) count += 1;
    }
  }
  return count;
}

/**
 * Every subject the everything scope could still read: unread vehicle rows, bystanders skipped, automatic
 * culls and manual rejects included. The engine filters the final rows; this is only the label's count.
 */
export function everythingCount(frames, facts) {
  let count = 0;
  for (const frame of frames ?? []) {
    for (const d of detsOf(facts, frame.id)) {
      if (isRead(d) || d.bystander) continue;
      if (isVehicle(d)) count += 1;
    }
  }
  return count;
}

/** True when Identify has read at least one subject here: the album has been identified. */
export function hasReadings(frames, facts) {
  for (const frame of frames ?? []) {
    if (detsOf(facts, frame.id).some(isRead)) return true;
  }
  return false;
}

/** The primary action once the album has a reading: it only reads what is new. */
export function identifyNewLabel(n) {
  const count = Number.isFinite(n) ? Number(n) : 0;
  return `Identify ${count.toLocaleString()} new ${count === 1 ? 'car' : 'cars'}`;
}
