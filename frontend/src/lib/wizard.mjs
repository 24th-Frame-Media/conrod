// Pure logic behind the first-run setup wizard: which shoot types to offer, how the answers turn into the
// photographer's profile (`user_profile` in the global settings), when the wizard opens by itself, and the
// summary it ends on. The engine owns the order the profile is applied in (album override, then the profile's
// default for the shoot type, then the type's own policy; see `Lanes::resolve_with`); this builds the profile and
// previews its effect with the same rules. Pure, so node --test can import it.
import { applyOverrides, defaultSummary, READ_KEYS } from './presetPolicy.mjs';

/**
 * What the wizard offers, grouped exactly as the Import sheet groups it (types.ts `PROFILE_GROUPS`): the same
 * Motorsport / Portraits / Events / Mixed headings and the same names, so a photographer learns one list.
 */
export const WIZARD_GROUPS = [
  { id: 'motorsport', title: 'Motorsport', choices: [
    { id: 'motorsport', title: 'Motorsport', hint: 'Cars, bikes and the perfect pan' },
    { id: 'motorsport-burnouts', title: 'Burnouts', hint: 'Smoke, crowds, comps' },
    { id: 'motorsport-track', title: 'Track days', hint: 'Road cars on a circuit' },
    { id: 'motorsport-rally', title: 'Rallies', hint: 'Stages, service parks, spectators' },
    { id: 'motorsport-meet', title: 'Car meets & shows', hint: 'Parked cars, owners and badges' },
    { id: 'sport-cycling', title: 'Cycling', hint: 'Riders, bikes and the race' },
    { id: 'sport-action', title: 'Action sports', hint: 'Riders, boards and tricks' },
    { id: 'auto-dealer', title: 'Dealership stock', hint: 'One car at a time, clean and sharp' },
    { id: 'aviation', title: 'Aviation', hint: 'Air shows and spotting' },
  ] },
  { id: 'portrait', title: 'Portraits', choices: [
    { id: 'portrait', title: 'Portraits', hint: 'Eyes and faces decide the frame' },
    { id: 'portrait-group', title: 'Group photos', hint: 'Everyone looking at once' },
    { id: 'portrait-couple', title: 'Couple photos', hint: 'Two people, one moment' },
    { id: 'portrait-pets', title: 'Pets', hint: 'Eyes, play and expression' },
  ] },
  { id: 'event', title: 'Events', choices: [
    { id: 'event', title: 'Events', hint: 'Speakers, crowds, the occasion' },
    { id: 'event-shows', title: 'Live shows (performers)', hint: 'Performers, animals and prizes — not car shows' },
    { id: 'event-parties', title: 'Parties', hint: 'Candids and celebrations' },
    { id: 'sport-field', title: 'Field sports', hint: 'Football, rugby, cricket' },
    { id: 'sport-court', title: 'Court sports', hint: 'Basketball, netball, volleyball' },
    { id: 'sport-rodeo', title: 'Rodeo', hint: 'Rider and animal' },
    { id: 'sport-equestrian', title: 'Equestrian', hint: 'Horse and rider' },
    { id: 'wedding', title: 'Weddings', hint: 'Couple, guests, expressions' },
    { id: 'concert', title: 'Concerts', hint: 'Stage light, dark frames are fine' },
  ] },
  { id: 'mix', title: 'Mixed', choices: [
    { id: 'mix', title: 'Mixed', hint: 'Nature, property and no dominant subject' },
    { id: 'property-listing', title: 'Property listing', hint: 'Rooms and exteriors' },
    { id: 'property-report', title: 'Condition report', hint: 'Every detail, nothing culled for taste' },
    { id: 'wildlife-birds', title: 'Birds', hint: 'Animals and birds in flight' },
    { id: 'landscape', title: 'Landscape', hint: 'Horizon, foreground, sky' },
    { id: 'astro', title: 'Astro', hint: 'Stars, meteors, aurora' },
  ] },
];

const CHOICES = WIZARD_GROUPS.flatMap((g) => g.choices);
export const WIZARD_PRESETS = CHOICES.map((c) => c.id);

/** The plain name of a shoot type the wizard offers (the id itself for any other). */
export function choiceTitle(id) {
  return CHOICES.find((c) => c.id === id)?.title ?? id;
}

/** Motorsport types where drivers, crew and a crowd are around the cars: the wizard asks who is in the pictures. */
export const PEOPLE_PRESETS = ['motorsport', 'motorsport-track', 'motorsport-rally', 'motorsport-burnouts'];

/** The three answers to "who is in your pictures". `around` is Conrod's own default for these types. */
export const PEOPLE_ANSWERS = [
  { id: 'none', title: 'Just the cars', hint: 'Skip people altogether. Fastest.' },
  { id: 'around', title: 'Pit lane, paddock and crowd', hint: 'Look for people, but do not work on faces. The usual choice.' },
  { id: 'faces', title: 'Drivers and crew too', hint: 'Find faces and eyes, and suggest names.' },
];

/** How readers are named on the wizard screen. The engine's own rows say "Race numbers" or "Jersey numbers" per type. */
export const READ_LABELS = {
  numbers: 'Race and jersey numbers',
  plates: 'Number plates',
  make_model: 'Make and model',
  sponsors: 'Sponsors and livery',
  names: 'Names, from faces',
};

export { READ_KEYS };

/** The first run's stored answer, or an empty one. Anything else in `settings.user_profile` is ignored. */
export function profileOf(settings) {
  const raw = settings && typeof settings === 'object' ? settings.user_profile : null;
  return raw && typeof raw === 'object' && !Array.isArray(raw) ? raw : {};
}

const flags = (value) => {
  const out = {};
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    for (const [key, on] of Object.entries(value)) if (typeof on === 'boolean') out[key] = on;
  }
  return out;
};

/** The profile's switches for one shoot type: what applies to every type, then what it says for this one. */
export function profileDefaults(profile, preset) {
  const p = profileOf({ user_profile: profile });
  const by = p.presets && typeof p.presets === 'object' ? p.presets[preset] : null;
  return { ...flags(p.all), ...flags(by) };
}

/** The wizard opens by itself only on a fresh install: no finished profile, no albums, and What's New never seen. */
export function shouldAutoOpenWizard({ profile, albums, whatsNewSeen }) {
  if (profileOf({ user_profile: profile }).done === true) return false;
  return albums === 0 && (whatsNewSeen == null || whatsNewSeen === '');
}

const summaryFor = (preset, overrides) => applyOverrides(defaultSummary(preset), overrides);

/** Readers that are on across the chosen types, with the people answers applied: what the screen prefills. */
export function prefilledReads(selected, people = {}) {
  const on = Object.fromEntries(READ_KEYS.map((key) => [key, false]));
  for (const preset of selected) {
    const set = peopleSwitches(preset, people[preset]);
    for (const row of summaryFor(preset, set).reads) if (row.on) on[row.key] = true;
  }
  return on;
}

/** Which of the chosen types a reader can do anything for: a plate needs a car, a name needs a face. */
export function readerApplies(selected, key, people = {}) {
  return selected.filter((preset) => {
    const row = summaryFor(preset, peopleSwitches(preset, people[preset])).reads.find((r) => r.key === key);
    return row && !row.blocked;
  });
}

function peopleSwitches(preset, answer) {
  if (!PEOPLE_PRESETS.includes(preset)) return {};
  if (answer === 'none') return { people: false };
  if (answer === 'faces') return { faces: true };
  return {};
}

/**
 * The profile the wizard saves. `selected` is in the order chosen, the default type first. `reads` holds only
 * the readers the photographer changed (true/false); an untouched one follows each type. A reader is changed only
 * where it can work (a plate needs a car), so a wedding never gets a plate switch.
 */
export function buildProfile({ selected, people = {}, reads = {}, blur = null, momentsRank = true }) {
  const shoots = [...new Set(selected.filter((id) => WIZARD_PRESETS.includes(id)))];
  const presets = {};
  for (const preset of shoots) {
    const set = peopleSwitches(preset, people[preset]);
    const base = summaryFor(preset, set);
    for (const key of READ_KEYS) {
      if (typeof reads[key] !== 'boolean') continue;
      const row = base.reads.find((r) => r.key === key);
      if (row && !row.blocked && row.on !== reads[key]) set[key] = reads[key];
    }
    if (Object.keys(set).length) presets[preset] = set;
  }
  const profile = { done: true };
  if (shoots.length) { profile.default_preset = shoots[0]; profile.shoots = shoots; }
  if (typeof blur === 'string' && ['critical', 'strict', 'moderate', 'lenient'].includes(blur)) profile.blur = blur;
  if (momentsRank === false) profile.all = { moments_rank: false };
  if (Object.keys(presets).length) profile.presets = presets;
  return profile;
}

/** The wizard's starting answers when it is opened again from Settings: what the saved profile says. */
export function answersFromProfile(profile) {
  const p = profileOf({ user_profile: profile });
  const selected = Array.isArray(p.shoots) ? p.shoots.filter((id) => WIZARD_PRESETS.includes(id)) : [];
  const people = {};
  for (const preset of selected) {
    if (!PEOPLE_PRESETS.includes(preset)) continue;
    const set = profileDefaults(p, preset);
    people[preset] = set.people === false ? 'none' : set.faces === true ? 'faces' : 'around';
  }
  // A reader differs from the types' own defaults: it counts as changed, with the value the profile gives it.
  const reads = {};
  for (const key of READ_KEYS) {
    const own = selected.some((preset) => summaryFor(preset, peopleSwitches(preset, people[preset])).reads.some((r) => r.key === key && r.on));
    const mine = selected.some((preset) => summaryFor(preset, { ...peopleSwitches(preset, people[preset]), ...profileDefaults(p, preset) }).reads.some((r) => r.key === key && r.on));
    if (own !== mine) reads[key] = mine;
  }
  return {
    selected,
    defaultId: typeof p.default_preset === 'string' && selected.includes(p.default_preset) ? p.default_preset : selected[0] ?? null,
    people,
    reads,
    blur: typeof p.blur === 'string' ? p.blur : null,
    momentsRank: flags(p.all).moments_rank !== false,
  };
}

/** The shoot types in the order saved: the default first, then the rest as chosen. */
export function orderSelected(selected, defaultId) {
  if (!defaultId || !selected.includes(defaultId)) return [...selected];
  return [defaultId, ...selected.filter((id) => id !== defaultId)];
}

/** One plain line per shoot type for the last screen: what Conrod will look for and read there. */
export function summaryLines(profile) {
  const p = profileOf({ user_profile: profile });
  const shoots = Array.isArray(p.shoots) ? p.shoots : [];
  return shoots.map((preset) => {
    const s = summaryFor(preset, profileDefaults(p, preset));
    const names = (rows) => rows.filter((r) => r.on).map((r) => r.label.toLowerCase());
    return {
      preset,
      title: choiceTitle(preset),
      looks: names(s.detects),
      reads: names(s.reads),
      moments: s.moments.detect.on ? (s.moments.rank.on ? 'spots moments and lets them lift frames' : 'spots moments, never lifts frames') : '',
      blur: p.blur ?? s.blur,
    };
  });
}

/** "a, b and c" */
export function listWords(items) {
  if (items.length <= 1) return items.join('');
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
}
