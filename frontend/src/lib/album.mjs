// Per-album Review view preferences and album-level settings; pure so node tests can reach them.
export const GROUPINGS = ['bursts', 'vehicles', 'dupes', 'flat'];
export const SORTS = ['review', 'best', 'worst', 'frame', 'pick'];
export const VIEWS = ['all', 'keepers', 'undecided', 'rejected', 'top3', 'review', 'needsLook'];
/** View ids from older builds, mapped to the closest current one. */
const LEGACY_VIEWS = { kept: 'keepers', picks: 'keepers', stacks: 'all', vehicles: 'all' };

export const prefsKey = (jobId) => `conrod.review.view.${jobId}`;
export const defaultPrefs = { grouping: 'bursts', sort: 'frame', view: 'all', minStars: 0 };

/** Whatever was stored (or nothing) becomes a complete, valid set of prefs. */
export function normalisePrefs(raw) {
  const r = raw && typeof raw === 'object' ? raw : {};
  const view = LEGACY_VIEWS[r.view] ?? r.view;
  return {
    grouping: GROUPINGS.includes(r.grouping) ? r.grouping : defaultPrefs.grouping,
    sort: SORTS.includes(r.sort) ? r.sort : defaultPrefs.sort,
    view: VIEWS.includes(view) ? view : defaultPrefs.view,
    minStars: Number.isInteger(r.minStars) && r.minStars >= 0 && r.minStars <= 5 ? r.minStars : defaultPrefs.minStars,
  };
}

export function parsePrefs(text) {
  try { return normalisePrefs(JSON.parse(text ?? 'null')); } catch { return normalisePrefs(null); }
}

/** An album's own value for a setting (its `settings_json` override), else the global default. */
export function albumSetting(job, key, globals) {
  let own;
  try { own = job?.settings_json ? JSON.parse(job.settings_json)[key] : undefined; } catch { own = undefined; }
  return own ?? globals?.[key];
}
