// Pure logic behind "What's new": version order, which entries to open, and when to show the sheet by itself.
// scripts/release-notes.mjs imports compareVersions from here, so the app and the generator order versions the same way.

export const SEEN_KEY = 'conrod:whats-new-seen';

const VERSION = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z]+(?:\.[0-9A-Za-z]+)*))?$/;

/** Oldest first: numbers, then a pre-release before its stable release (1.0.0-beta.2 < 1.0.0-beta.10 < 1.0.0). */
export function compareVersions(a, b) {
  const pa = VERSION.exec(a);
  const pb = VERSION.exec(b);
  if (!pa || !pb) return String(a).localeCompare(String(b));
  for (let i = 1; i <= 3; i += 1) {
    const diff = Number(pa[i]) - Number(pb[i]);
    if (diff !== 0) return diff;
  }
  if (!pa[4] || !pb[4]) return pa[4] ? -1 : pb[4] ? 1 : 0;
  const x = pa[4].split('.');
  const y = pb[4].split('.');
  for (let i = 0; i < Math.max(x.length, y.length); i += 1) {
    if (x[i] === undefined) return -1;
    if (y[i] === undefined) return 1;
    const numeric = /^\d+$/.test(x[i]) && /^\d+$/.test(y[i]);
    const diff = numeric ? Number(x[i]) - Number(y[i]) : x[i].localeCompare(y[i]);
    if (diff !== 0) return diff;
  }
  return 0;
}

export const isVersion = (value) => typeof value === 'string' && VERSION.test(value);

/** Released entries only (an "Unreleased" entry never reaches a user), newest first. */
export function orderEntries(entries) {
  return (entries ?? [])
    .filter((entry) => entry && isVersion(entry.version) && !entry.unreleased)
    .sort((a, b) => compareVersions(b.version, a.version));
}

const clean = (version) => (isVersion(version) ? String(version).replace(/^v/, '') : null);

/**
 * The sheet opens by itself once per new version: when this build has an entry and the version last seen is
 * older (or there is none, as on a first run or the first time after this feature shipped). A downgrade
 * to a version already seen stays quiet.
 */
export function shouldShowWhatsNew({ current, lastSeen, entries }) {
  const now = clean(current);
  if (!now) return false;
  if (!orderEntries(entries).some((entry) => entry.version === now)) return false;
  const seen = clean(lastSeen);
  return seen === null || compareVersions(now, seen) > 0;
}

/**
 * What the sheet shows. `fresh` is everything released since the version last seen, up to this build, newest
 * first (at least the newest entry of this build); `earlier` is the rest, for the collapsed list.
 * With no known current version (a browser run) everything counts as released.
 */
export function splitEntries({ entries, current, lastSeen }) {
  const now = clean(current);
  const seen = clean(lastSeen);
  const ordered = orderEntries(entries).filter((entry) => now === null || compareVersions(entry.version, now) <= 0);
  const fresh = ordered.filter((entry, i) => i === 0 || (seen !== null && compareVersions(entry.version, seen) > 0));
  const freshVersions = new Set(fresh.map((entry) => entry.version));
  return { fresh, earlier: ordered.filter((entry) => !freshVersions.has(entry.version)) };
}

/** The version last seen, or null. `storage` is localStorage (or a stand-in); a blocked store reads as never seen. */
export function readSeen(storage) {
  try { return clean(storage?.getItem(SEEN_KEY)); } catch { return null; }
}

/** Remembers `version` as seen, never moving the mark backwards. Returns what is now stored. */
export function writeSeen(storage, version) {
  const next = clean(version);
  const seen = readSeen(storage);
  if (!next) return seen;
  if (seen !== null && compareVersions(next, seen) <= 0) return seen;
  try { storage?.setItem(SEEN_KEY, next); } catch { /* not persisted */ }
  return next;
}

/** "20 Nov 2026" from a YYYY-MM-DD date; null for anything else. */
export function formatDate(date) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date ?? '');
  if (!match) return null;
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  return `${Number(match[3])} ${months[Number(match[2]) - 1]} ${match[1]}`;
}
