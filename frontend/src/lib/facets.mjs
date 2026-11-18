/* Which facet tabs the Review sidebar offers for a shoot preset. Pure, so node --test can import it. */

/** Every facet kind, in the order the sidebar lists them. */
export const FACET_KINDS = ['number', 'plate', 'make', 'team', 'person', 'subject'];

const PEOPLE_ONLY = ['person'];
const NONE = [];

/** Preset id (`scan_profile`) to the ordered tabs it shows. Unlisted ids fall back to `FALLBACK`. */
const BY_PRESET = {
  'motorsport': ['number', 'person'],
  'motorsport-track': ['number', 'person'],
  'motorsport-rally': ['number', 'person'],
  'motorsport-burnouts': ['number', 'plate', 'person'],
  'motorsport-meet': ['plate', 'make', 'person'],
  'auto-dealer': ['plate', 'make', 'person'],
  'portrait': PEOPLE_ONLY,
  'portrait-group': PEOPLE_ONLY,
  'portrait-couple': PEOPLE_ONLY,
  'wedding': PEOPLE_ONLY,
  'event': PEOPLE_ONLY,
  'event-shows': PEOPLE_ONLY,
  'event-parties': PEOPLE_ONLY,
  'concert': PEOPLE_ONLY,
  'portrait-pets': ['subject', 'person'],
  'wildlife-birds': ['subject', 'person'],
  'sport-field': ['number', 'person'],
  'sport-court': ['number', 'person'],
  'sport-cycling': ['number', 'person'],
  'sport-action': ['number', 'person'],
  'sport-rodeo': ['person', 'subject'],
  'sport-equestrian': ['person', 'subject'],
  'property-listing': NONE,
  'property-report': NONE,
  'landscape': NONE,
  'astro': NONE,
  'aviation': ['plate', 'make'],
};

/** Mixed albums and ids the engine adds later: every tab that has data (subjects excluded, they repeat the vehicle list). */
const FALLBACK = ['number', 'plate', 'make', 'team', 'person'];

/** The tabs a preset may show, in order. An empty list means the sidebar has nothing to offer. */
export function facetsFor(preset) {
  const id = typeof preset === 'string' ? preset.trim().toLowerCase() : '';
  return [...(Object.hasOwn(BY_PRESET, id) ? BY_PRESET[id] : FALLBACK)];
}

/** `facetsFor`, minus the tabs with no entries in this album. `facets` maps kind to its item list. */
export function tabsWithData(preset, facets) {
  return facetsFor(preset).filter((kind) => (facets?.[kind]?.length ?? 0) > 0);
}

/** The tab to show: the wanted one if it is still offered, else the first one left, else null. */
export function validTab(wanted, tabs) {
  return wanted && tabs.includes(wanted) ? wanted : tabs[0] ?? null;
}
