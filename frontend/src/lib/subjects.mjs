// What an album is looking at, in the shoot's own words. Every screen that used to say
// "cars" asks this instead: a rodeo reads animals and the riders on them, a wedding people.
// The answer comes from the album's resolved lane summary, never from a hardcoded car.
// Pure, so node --test can import it.
import { isOn } from './presetPolicy.mjs';

/** The subject kinds the detector lanes can put first, and the words a screen needs for them. */
export const SUBJECT_WORDS = {
  vehicles: { one: 'vehicle', many: 'vehicles', title: 'Vehicles', rate: 'vehicles/min', each: 's/vehicle' },
  animals: { one: 'animal', many: 'animals', title: 'Animals', rate: 'animals/min', each: 's/animal' },
  people: { one: 'person', many: 'people', title: 'People', rate: 'people/min', each: 's/person' },
  subjects: { one: 'subject', many: 'subjects', title: 'Subjects', rate: 'subjects/min', each: 's/subject' },
};

/**
 * The kind of subject an album puts first: `vehicles`, `animals`, `people`, or `subjects`.
 * A type that looks for both animals and people says "subjects", because a ride is an
 * animal and its rider and no one word is true for both.
 */
export function subjectKind(summary) {
  if (!summary || typeof summary !== 'object') return 'subjects';
  if (isOn(summary, 'vehicles')) return 'vehicles';
  const animals = isOn(summary, 'animals');
  const people = isOn(summary, 'people');
  if (animals && people) return 'subjects';
  if (animals) return 'animals';
  if (people) return 'people';
  return 'subjects';
}

/** The plain words for that kind: `{ one, many, title, rate, each }`. */
export function subjectWords(summary) {
  return SUBJECT_WORDS[subjectKind(summary)];
}

/**
 * Whether the album has a reader that runs over a subject crop: numbers, plates, make and
 * model, or livery. Names ride the face pass instead, so they do not count here; a rodeo
 * reads none of these and judges moments (and suggests names from faces) instead.
 * Unknown (no summary) keeps the readers on, so a screen never hides work it cannot see.
 */
export function readsSubjects(summary) {
  if (!summary || typeof summary !== 'object') return true;
  return ['numbers', 'plates', 'make_model', 'sponsors'].some((key) => isOn(summary, key));
}

/** A detector's purpose line: "Vehicles and people detector", "Person detector", "Finds the shoot's subjects". */
export function detectorPurpose(words) {
  const w = words ?? SUBJECT_WORDS.subjects;
  if (w.many === 'people') return 'Person detector';
  if (w.many === 'subjects') return "Finds the shoot's subjects";
  return `${w.title} and people detector`;
}

/** The live identification banner's title: "Identifying animals", or "Judging moments" with nothing to read. */
export function identifyTitle(summary) {
  return readsSubjects(summary) ? `Identifying ${subjectWords(summary).many}` : 'Judging moments';
}
