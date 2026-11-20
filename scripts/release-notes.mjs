// Release notes from git history: what a photographer would want to know, grouped as New, Improved and Fixed.
//
//   node scripts/release-notes.mjs --unreleased                    last tag -> HEAD, as Markdown on stdout
//   node scripts/release-notes.mjs --version 1.0.0-beta.9          the notes for that release (the range ends at its tag, or HEAD)
//   node scripts/release-notes.mjs --from v1.0.0-beta.7 --to v1.0.0-beta.8
//   node scripts/release-notes.mjs --all                           every release since v0.6.0 as a CHANGELOG, on stdout
//   node scripts/release-notes.mjs --all --write                   ...and write CHANGELOG.md + frontend/src/lib/whatsNew.json
//   node scripts/release-notes.mjs --all --write --release 1.0.0-beta.9
//                                                                  same, with the unreleased work filed under that version (used by release.mjs)
//   node scripts/release-notes.mjs --audit [--from ref]            commits worded from their own subject (no override yet)
//
// Options: --body <file> writes the GitHub release body (Markdown), --json <file> writes the entry for the app,
//          --date YYYY-MM-DD overrides a release's date, --since <tag> starts --all later (default v0.6.0).
//
// Only user-facing commits are kept: feat -> New, perf and security -> Improved, fix -> Fixed. chore, ci, test, docs,
// refactor, merges and internal scopes are dropped. Wording comes from scripts/release-notes.overrides.json (by commit
// hash or version), then a `Release-note:` trailer in the commit body, then the commit subject. See RELEASING.md.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  appEntries, buildEntry, compareVersions, previousTag, releaseTags, renderChangelog, renderReleaseBody, unreviewed,
} from './release-notes-lib.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO_URL = 'https://github.com/kapsikkum/conrod';
const FIRST_TAG = 'v0.6.0';

const args = process.argv.slice(2);
const flag = (name) => {
  const i = args.indexOf(name);
  return i === -1 ? undefined : args[i + 1];
};
const has = (name) => args.includes(name);

function usage(message) {
  if (message) console.error(`${message}\n`);
  console.error(
    'usage: node scripts/release-notes.mjs --unreleased | --version <v> | --from <ref> --to <ref> | --all [--write] [--release <v>] | --audit\n' +
      '       [--body <file>] [--json <file>] [--date YYYY-MM-DD] [--since <tag>]',
  );
  process.exit(1);
}

const git = (...a) => execFileSync('git', ['-c', 'core.quotepath=off', ...a], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }).trim();
const tryGit = (...a) => {
  try { return git(...a); } catch { return null; }
};

const overridesFile = join(root, 'scripts', 'release-notes.overrides.json');
const overrides = existsSync(overridesFile) ? JSON.parse(readFileSync(overridesFile, 'utf8')) : {};

const today = () => new Date().toISOString().slice(0, 10);
const allTags = () => releaseTags(git('tag', '--list', 'v*').split('\n').filter(Boolean));
const tagDate = (tag) => git('log', '-1', '--format=%cs', `${tag}^{commit}`);

/** Non-merge commits in `from..to`, oldest first, as { hash, subject, body }. */
function commitsBetween(from, to) {
  const SEP = '\u001e';
  const FIELD = '\u001f';
  const out = git('log', '--no-merges', '--reverse', `--format=%H${FIELD}%s${FIELD}%b${SEP}`, from ? `${from}..${to}` : to);
  return out
    .split(SEP)
    .map((chunk) => chunk.trim())
    .filter(Boolean)
    .map((chunk) => {
      const [hash, subject, body = ''] = chunk.split(FIELD);
      return { hash, subject, body };
    });
}

const write = (file, content) => {
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, content.endsWith('\n') ? content : `${content}\n`);
};

/** One release: `version` (null for the unreleased work), measured from `from` to `to`. */
function entryFor({ version, from, to, date }) {
  const commits = commitsBetween(from, to);
  const entry = buildEntry({ version: version ?? 'Unreleased', date, commits, overrides });
  if (!version) entry.unreleased = true;
  return { entry, commits };
}

function allEntries() {
  const since = (flag('--since') ?? FIRST_TAG).replace(/^v?/, 'v');
  const tags = allTags();
  const release = flag('--release')?.replace(/^v/, '');
  const entries = [];
  for (const tag of tags) {
    if (compareVersions(tag, since) < 0) continue;
    const version = tag.slice(1);
    entries.push(entryFor({ version, from: previousTag(tags, tag), to: tag, date: tagDate(tag) }).entry);
  }
  const last = tags[tags.length - 1] ?? null;
  const head = release
    ? entryFor({ version: release, from: previousTag(tags, release), to: 'HEAD', date: flag('--date') ?? today() }).entry
    : entryFor({ version: null, from: last, to: 'HEAD' }).entry;
  // Nothing new since the last tag: there is no unreleased section to show.
  if (release || head.highlights || head.sections.length) entries.push(head);
  return entries.reverse();
}

function main() {
  if (has('--audit')) {
    const from = flag('--from') ?? previousTag(allTags(), null);
    const rows = unreviewed(commitsBetween(from, flag('--to') ?? 'HEAD'), overrides);
    if (!rows.length) return console.log(`Every user-facing commit since ${from ?? 'the start'} has reviewed wording.`);
    console.log(`${rows.length} commit(s) since ${from ?? 'the start'} would be worded from their own subject. Add them to scripts/release-notes.overrides.json:\n`);
    for (const { commit, note } of rows) console.log(`${commit.hash.slice(0, 8)}  [${note.section}]  ${commit.subject}`);
    return;
  }

  if (has('--all')) {
    const entries = allEntries();
    const changelog = renderChangelog(entries, { repoUrl: REPO_URL });
    if (has('--write')) {
      write(join(root, 'CHANGELOG.md'), changelog);
      write(join(root, 'frontend', 'src', 'lib', 'whatsNew.json'), `${JSON.stringify(appEntries(entries), null, 2)}\n`);
      console.log(`Wrote CHANGELOG.md (${entries.length} sections) and frontend/src/lib/whatsNew.json (${appEntries(entries).length} releases).`);
    } else {
      process.stdout.write(changelog);
    }
    return;
  }

  const tags = allTags();
  let version = flag('--version')?.replace(/^v/, '');
  let to = flag('--to') ?? 'HEAD';
  let from = flag('--from');
  if (has('--unreleased')) {
    version = undefined;
    from ??= tags[tags.length - 1] ?? null;
  } else if (!version && !from) {
    usage('Say which release: --unreleased, --version <v>, --from/--to, or --all.');
  } else if (!version) {
    version = to.replace(/^v/, '');
    if (to === 'HEAD') usage('--from needs --to <tag> (or --version) so the release has a name.');
  }
  if (from === undefined && version) from = previousTag(tags, version);
  if (from === undefined) from = null;

  const known = version && tags.includes(`v${version}`);
  const date = flag('--date') ?? (version ? (known ? tagDate(`v${version}`) : today()) : undefined);
  // The tag for this very version is the end of its range when it exists and no --to was given.
  if (known && !flag('--to')) to = `v${version}`;
  const { entry } = entryFor({ version: version ?? null, from, to, date });

  const body = flag('--body');
  if (body) write(body, renderReleaseBody(entry));
  const json = flag('--json');
  if (json) write(json, JSON.stringify(entry, null, 2));
  if (!body && !json) {
    const range = `${from ?? 'the start'}..${to}`;
    console.log(`<!-- ${entry.version} (${range}) -->\n${renderReleaseBody(entry)}`);
  } else {
    console.log(`Notes for ${entry.version} (${from ?? 'the start'}..${to}): ${entry.sections.reduce((n, s) => n + s.items.length, 0)} item(s).`);
  }
}

main();
