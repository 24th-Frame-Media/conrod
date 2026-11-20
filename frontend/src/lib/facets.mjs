/* Which facet tabs the Review sidebar offers for a shoot. Pure, so node --test can import it.
   The tabs follow what the shoot's type reads (the engine's policy, `presetPolicy.mjs`), with the album's own switches. */
import { applyOverrides, defaultSummary, facetsFromSummary } from './presetPolicy.mjs';

/** Every facet kind, in the order the sidebar lists them. */
export const FACET_KINDS = ['number', 'plate', 'make', 'team', 'person', 'subject', 'moment'];

/** Tabs a shoot may show, in order. An empty list means the sidebar has nothing to offer. */
export function facetsFor(preset, overrides = {}) {
  return facetsFromSummary(applyOverrides(defaultSummary(preset), overrides));
}

/** `facetsFor`, minus the tabs with no entries in this album. `facets` maps kind to its item list. */
export function tabsWithData(preset, facets, overrides = {}) {
  return facetsFor(preset, overrides).filter((kind) => (facets?.[kind]?.length ?? 0) > 0);
}

/** The tab to show: the wanted one if it is still offered, else the first one left, else null. */
export function validTab(wanted, tabs) {
  return wanted && tabs.includes(wanted) ? wanted : tabs[0] ?? null;
}
