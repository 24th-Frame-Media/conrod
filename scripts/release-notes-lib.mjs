// The pure half of release-notes.mjs: turns parsed commits into release notes, with no git and no files, so
// frontend/tests/releaseNotes.test.mjs can exercise it. Output shape (one entry per release):
//   { version, date?, highlights?, sections: [{ title: 'New' | 'Improved' | 'Fixed', items: string[] }] }
import { compareVersions } from '../frontend/src/lib/whatsNew.mjs';

export { compareVersions };

export const SECTION_TITLES = ['New', 'Improved', 'Fixed'];

// feat adds something, perf and security make what is there better, fix repairs it. Every other type
// (chore, ci, test, docs, refactor, style, build, merge, ...) is dropped.
const TYPE_SECTION = { feat: 'New', perf: 'Improved', security: 'Improved', fix: 'Fixed' };

// Scopes that only matter to people working on the code, even under feat/fix.
const DEV_SCOPES = /^(scripts?|ci|build|deps|test|tests|docs|dev|release|repo|fmt|clippy|lint|plans?|research|bench)$/i;
// Wording that gives away a developer-only change.
const DEV_ONLY =
  /\b(refactor(ed|ing)?|lint(er|ing)?|clippy|rustfmt|cargo fmt|typecheck|tsc|ci|workflows?|github actions|dependabot|bumps?|bumped|lockfile|changelog|release notes?|harness|fixtures?)\b/i;

const SUBJECT = /^(\w+)(?:\(([^)]*)\))?(!)?:\s*(.+)$/;
const SKIP = 'skip';

/** `{ type, scope, text }` for a conventional-commit subject, or null. */
export function parseSubject(subject) {
  const match = SUBJECT.exec(subject.trim());
  if (!match) return null;
  return { type: match[1].toLowerCase(), scope: match[2] ?? '', text: match[4].trim() };
}

/** "feat(x): add a thing (#12)" -> "Add a thing." */
export function sentence(text) {
  const line = text
    .replace(/(\s*\(#\d+\))+\s*$/, '')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^./, (c) => c.toUpperCase());
  return /[.!?]$/.test(line) ? line : `${line}.`;
}

/**
 * The override for a commit, found by a hash prefix of at least 7 characters, as a list of
 * `{ section, text }` (empty for "skip"). Values: "skip", "text", { section, text }, or an array of those.
 * `fallback` is the section for a bare string.
 */
export function overrideFor(commits, hash, fallback) {
  const key = Object.keys(commits ?? {}).find((k) => k.length >= 7 && hash.startsWith(k));
  if (key === undefined) return null;
  return normalise(commits[key], fallback);
}

export function normalise(value, fallback) {
  const list = Array.isArray(value) ? value : [value];
  const out = [];
  for (const item of list) {
    if (item == null) continue;
    if (typeof item === 'string') {
      if (item.trim().toLowerCase() === SKIP) continue;
      out.push({ section: fallback, text: item.trim() });
    } else if (item.text) {
      if (item.text.trim().toLowerCase() === SKIP) continue;
      out.push({ section: SECTION_TITLES.includes(item.section) ? item.section : fallback, text: item.text.trim() });
    }
  }
  return out;
}

/**
 * What one commit contributes: a list of `{ section, text, reviewed }`. `reviewed` is false when the wording
 * comes from the commit subject itself (the audit lists those). Order of precedence: overrides file,
 * `Release-note:` trailer, the subject.
 */
export function classify(commit, overrides = {}) {
  const parsed = parseSubject(commit.subject);
  const section = parsed ? TYPE_SECTION[parsed.type] : undefined;
  // A commit that is not a conventional feat/fix/perf only counts when an override names it.
  const fallback = section ?? 'Improved';

  const override = overrideFor(overrides.commits, commit.hash, fallback);
  if (override) return override.map((note) => ({ ...note, reviewed: true }));

  if (!parsed || !section) return [];
  const trailer = /^Release-note:\s*(.+)$/im.exec(commit.body ?? '')?.[1].trim();
  if (trailer) return trailer.toLowerCase() === SKIP ? [] : [{ section, text: trailer, reviewed: true }];
  if (DEV_SCOPES.test(parsed.scope) || DEV_ONLY.test(parsed.text)) return [];
  return [{ section, text: parsed.text, reviewed: false }];
}

/** Commits that carry no override and would be worded from their subject (what to review before a release). */
export function unreviewed(commits, overrides = {}) {
  return commits.flatMap((commit) => classify(commit, overrides).filter((note) => !note.reviewed).map((note) => ({ commit, note })));
}

/** One release's notes from its commits (oldest first), plus the hand-written parts of the overrides file. */
export function buildEntry({ version, date, commits, overrides = {} }) {
  const extra = overrides.versions?.[version] ?? {};
  const bySection = new Map(SECTION_TITLES.map((title) => [title, []]));
  const seen = new Set();
  const add = ({ section, text }) => {
    const line = sentence(text);
    const key = line.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    bySection.get(section).push(line);
  };
  for (const commit of commits) classify(commit, overrides).forEach(add);
  normalise(extra.add, 'Improved').forEach(add);

  const entry = { version };
  const day = extra.date ?? date;
  if (day) entry.date = day;
  if (extra.highlights) entry.highlights = extra.highlights.trim();
  entry.sections = SECTION_TITLES.map((title) => ({ title, items: bySection.get(title) })).filter((s) => s.items.length);
  return entry;
}

export const isEmpty = (entry) => !entry.highlights && entry.sections.length === 0;

/** The notes under a version heading: the highlights line, then one list per section. */
export function renderBody(entry, level = 3) {
  const parts = [];
  if (entry.highlights) parts.push(entry.highlights);
  for (const { title, items } of entry.sections) {
    parts.push(`${'#'.repeat(level)} ${title}\n\n${items.map((item) => `- ${item}`).join('\n')}`);
  }
  return parts.join('\n\n');
}

/** The GitHub release body: the notes, or a one-line stand-in when a release has no user-facing changes. */
export function renderReleaseBody(entry) {
  return isEmpty(entry) ? 'Maintenance release: no changes you will notice.' : renderBody(entry, 3);
}

/** The whole CHANGELOG.md, newest first. `entries` come newest first; the unreleased one carries no date. */
export function renderChangelog(entries, { repoUrl } = {}) {
  const head = [
    '# Changelog',
    '',
    'What changed in Conrod, release by release, in plain language. Generated from the git history by',
    '`node scripts/release-notes.mjs --all --write`; wording is steered by `scripts/release-notes.overrides.json`.',
    ...(repoUrl ? ['', `Installers and checksums are on the [releases page](${repoUrl}/releases).`] : []),
  ].join('\n');
  const blocks = entries.map((entry) => {
    const title = entry.unreleased ? 'Unreleased' : entry.version;
    const heading = `## ${title}${entry.date ? ` - ${entry.date}` : ''}`;
    const body = isEmpty(entry) ? '_No changes you will notice._' : renderBody(entry, 3);
    return `${heading}\n\n${body}`;
  });
  return `${[head, ...blocks].join('\n\n')}\n`;
}

/** Release tags in version order, oldest first: `v1.2.3` and `v1.2.3-beta.4`, nothing else (not `updater`). */
export function releaseTags(tags) {
  return tags.filter((tag) => /^v\d+\.\d+\.\d+(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$/.test(tag)).sort(compareVersions);
}

/** The tag a release is measured from: the newest tag older than `version` (or than everything, with none). */
export function previousTag(tags, version) {
  const ordered = releaseTags(tags).filter((tag) => !version || compareVersions(tag, version) < 0);
  return ordered.length ? ordered[ordered.length - 1] : null;
}

/** The app's data file: released entries with something to say, newest first. */
export function appEntries(entries) {
  return entries.filter((entry) => !entry.unreleased && !isEmpty(entry));
}
