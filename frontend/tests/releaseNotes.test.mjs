import assert from 'node:assert/strict';
import test from 'node:test';
import {
  appEntries, buildEntry, classify, isEmpty, normalise, overrideFor, parseSubject, previousTag,
  releaseTags, renderBody, renderChangelog, renderReleaseBody, sentence, unreviewed,
} from '../../scripts/release-notes-lib.mjs';

const commit = (hash, subject, body = '') => ({ hash, subject, body });

test('parseSubject reads a conventional commit, scope and bang optional', () => {
  assert.deepEqual(parseSubject('feat(review): add a thing'), { type: 'feat', scope: 'review', text: 'add a thing' });
  assert.deepEqual(parseSubject('fix!: breaking change'), { type: 'fix', scope: '', text: 'breaking change' });
  assert.equal(parseSubject('not conventional at all'), null);
});

test('sentence capitalises, drops trailing PR refs and always ends with punctuation', () => {
  assert.equal(sentence('add a thing (#123)'), 'Add a thing.');
  assert.equal(sentence('already punctuated!'), 'Already punctuated!');
  assert.equal(sentence('  extra   spaces   (#1) (#2)'), 'Extra spaces.');
});

test('classify groups by conventional type: feat -> New, perf/security -> Improved, fix -> Fixed', () => {
  assert.deepEqual(classify(commit('a1b2c3d', 'feat: add a thing')), [{ section: 'New', text: 'add a thing', reviewed: false }]);
  assert.deepEqual(classify(commit('a1b2c3d', 'perf: faster thing')), [{ section: 'Improved', text: 'faster thing', reviewed: false }]);
  assert.deepEqual(classify(commit('a1b2c3d', 'security: safer thing')), [{ section: 'Improved', text: 'safer thing', reviewed: false }]);
  assert.deepEqual(classify(commit('a1b2c3d', 'fix: broken thing')), [{ section: 'Fixed', text: 'broken thing', reviewed: false }]);
});

test('classify drops noise: chore/ci/test/docs/refactor types, dev scopes and dev-only wording', () => {
  assert.deepEqual(classify(commit('a1', 'chore: bump deps')), []);
  assert.deepEqual(classify(commit('a2', 'ci: fix workflow')), []);
  assert.deepEqual(classify(commit('a3', 'test: add a test')), []);
  assert.deepEqual(classify(commit('a4', 'docs: typo')), []);
  assert.deepEqual(classify(commit('a5', 'refactor: tidy up')), []);
  assert.deepEqual(classify(commit('a6', 'feat(ci): a new workflow step')), []);
  assert.deepEqual(classify(commit('a7', 'feat(scripts): a release helper')), []);
  assert.deepEqual(classify(commit('a8', 'fix: refactored the parser for clarity')), []);
  assert.deepEqual(classify(commit('a9', 'fix: bumped a dependency past an advisory')), []);
});

test('a Release-note trailer overrides the subject, and "skip" leaves the commit out', () => {
  assert.deepEqual(
    classify(commit('b1', 'feat: add a thing', 'Release-note: A much better description.')),
    [{ section: 'New', text: 'A much better description.', reviewed: true }],
  );
  assert.deepEqual(classify(commit('b2', 'feat: add a thing', 'Release-note: skip')), []);
  // A trailer on a commit that would otherwise be dropped (dev scope) still counts: hand-written wins.
  assert.deepEqual(
    classify(commit('b3', 'feat(ci): secretly user-facing', 'Release-note: Shows up anyway.')),
    [{ section: 'New', text: 'Shows up anyway.', reviewed: true }],
  );
});

test('overrideFor matches by hash prefix (7+ chars) and wins over the trailer and the subject', () => {
  const overrides = { commits: { abc1234: 'Plain English text', def5678: 'skip', aaa1111: { section: 'Fixed', text: 'Moved section' } } };
  assert.deepEqual(
    classify(commit('abc1234ff', 'feat: add a thing', 'Release-note: ignored, override wins'), overrides),
    [{ section: 'New', text: 'Plain English text', reviewed: true }],
  );
  assert.deepEqual(classify(commit('def5678aa', 'feat: add a thing'), overrides), []);
  assert.deepEqual(classify(commit('aaa1111bb', 'feat: add a thing'), overrides), [{ section: 'Fixed', text: 'Moved section', reviewed: true }]);
  // Too short a key (under 7 characters) never matches, even as a prefix.
  assert.equal(overrideFor({ ab1234: 'x' }, 'ab1234567', 'New'), null);
});

test('normalise handles a bare string, "skip", an object and an array of either', () => {
  assert.deepEqual(normalise('Some text', 'New'), [{ section: 'New', text: 'Some text' }]);
  assert.deepEqual(normalise('skip', 'New'), []);
  assert.deepEqual(normalise({ section: 'Fixed', text: 'A fix' }, 'New'), [{ section: 'Fixed', text: 'A fix' }]);
  assert.deepEqual(normalise({ section: 'Nonsense', text: 'Falls back' }, 'New'), [{ section: 'New', text: 'Falls back' }]);
  assert.deepEqual(
    normalise([{ section: 'New', text: 'One' }, 'Two', 'skip'], 'Improved'),
    [{ section: 'New', text: 'One' }, { section: 'Improved', text: 'Two' }],
  );
  assert.deepEqual(normalise(null, 'New'), []);
});

test('buildEntry groups into New/Improved/Fixed, de-dupes identical wording, and keeps overrides.versions extras', () => {
  const commits = [
    commit('c1111111', 'feat: add thing one'),
    commit('c2222222', 'fix: broken thing'),
    // Two different commits whose override text collapses to the same sentence: counted once.
    commit('c3333333', 'feat: add thing two'),
    commit('c4444444', 'feat: add thing two but worded differently'),
    commit('c5555555', 'chore: irrelevant'),
  ];
  const overrides = {
    commits: { c3333333: 'Shared wording.', c4444444: 'Shared wording.' },
    versions: { '1.2.3': { highlights: 'The one-line summary.', add: [{ section: 'Fixed', text: 'A fix with no commit of its own' }] } },
  };
  const entry = buildEntry({ version: '1.2.3', date: '2026-01-01', commits, overrides });
  assert.equal(entry.version, '1.2.3');
  assert.equal(entry.date, '2026-01-01');
  assert.equal(entry.highlights, 'The one-line summary.');
  const byTitle = Object.fromEntries(entry.sections.map((s) => [s.title, s.items]));
  assert.deepEqual(byTitle.New, ['Add thing one.', 'Shared wording.']);
  assert.deepEqual(byTitle.Fixed, ['Broken thing.', 'A fix with no commit of its own.']);
  assert.equal(byTitle.Improved, undefined);
});

test('overrides.versions.date overrides the tag date, and an all-noise range is empty', () => {
  const entry = buildEntry({
    version: '1.2.4', date: '2026-01-01', commits: [], overrides: { versions: { '1.2.4': { date: '2025-12-31' } } },
  });
  assert.equal(entry.date, '2025-12-31');
  assert.ok(isEmpty(entry));

  const noisy = buildEntry({ version: '1.2.5', date: '2026-01-02', commits: [commit('n1', 'chore: housekeeping'), commit('n2', 'ci: pipeline')] });
  assert.ok(isEmpty(noisy));
  assert.equal(renderReleaseBody(noisy), 'Maintenance release: no changes you will notice.');
});

test('an unreleased range (no tag yet) builds the same way, under the literal version "Unreleased"', () => {
  const commits = [commit('u1', 'feat: a new unreleased thing'), commit('u2', 'fix: a recent fix')];
  const entry = buildEntry({ version: 'Unreleased', commits, overrides: { versions: { Unreleased: { highlights: 'Coming soon.' } } } });
  assert.equal(entry.version, 'Unreleased');
  assert.equal(entry.date, undefined);
  assert.equal(entry.highlights, 'Coming soon.');
  assert.deepEqual(entry.sections.map((s) => s.title), ['New', 'Fixed']);
});

test('renderBody orders sections New, Improved, Fixed and renderChangelog wraps every entry', () => {
  const entry = buildEntry({
    version: '1.0.0',
    date: '2026-02-01',
    commits: [commit('r1', 'fix: a fix'), commit('r2', 'feat: a feature'), commit('r3', 'perf: an improvement')],
  });
  const body = renderBody(entry);
  const order = ['### New', '### Improved', '### Fixed'].map((h) => body.indexOf(h));
  assert.ok(order.every((i) => i >= 0));
  assert.ok(order[0] < order[1] && order[1] < order[2]);

  const changelog = renderChangelog([{ ...entry, unreleased: false }], { repoUrl: 'https://example.test/repo' });
  assert.match(changelog, /^# Changelog/);
  assert.match(changelog, /## 1\.0\.0 - 2026-02-01/);
  assert.match(changelog, /releases page.*example\.test\/repo\/releases/);

  const unreleased = buildEntry({ version: 'Unreleased', commits: [] });
  unreleased.unreleased = true;
  const withUnreleased = renderChangelog([unreleased]);
  assert.match(withUnreleased, /## Unreleased\n\n_No changes you will notice\._/);
});

test('releaseTags keeps only vX.Y.Z and vX.Y.Z-pre tags, sorted oldest first', () => {
  const tags = releaseTags(['v1.0.0', 'v0.6.0', 'updater', 'v1.0.0-beta.2', 'v1.0.0-beta.10', 'not-a-tag', 'v0.6.1']);
  assert.deepEqual(tags, ['v0.6.0', 'v0.6.1', 'v1.0.0-beta.2', 'v1.0.0-beta.10', 'v1.0.0']);
});

test('previousTag finds the newest tag strictly older than a version, or null before the first tag', () => {
  const tags = ['v0.6.0', 'v0.7.0', 'v1.0.0-beta.1', 'v1.0.0-beta.2'];
  assert.equal(previousTag(tags, 'v1.0.0-beta.2'), 'v1.0.0-beta.1');
  assert.equal(previousTag(tags, 'v0.6.0'), null);
  assert.equal(previousTag(tags, null), 'v1.0.0-beta.2');
});

test('unreviewed collects only the notes worded straight from a commit subject', () => {
  const commits = [
    commit('v1111111', 'feat: reviewed via override'),
    commit('v2222222', 'feat: not yet reviewed'),
  ];
  const rows = unreviewed(commits, { commits: { v1111111: 'Hand-written wording.' } });
  assert.deepEqual(rows.map((r) => r.commit.hash), ['v2222222']);
  assert.equal(rows[0].note.text, 'not yet reviewed');
});

test('appEntries drops the unreleased entry and any release with nothing to say', () => {
  const released = buildEntry({ version: '1.0.0', date: '2026-01-01', commits: [commit('e1', 'feat: a thing')] });
  const empty = buildEntry({ version: '0.9.9', date: '2025-12-01', commits: [commit('e2', 'chore: nothing user-facing')] });
  const unreleased = { ...buildEntry({ version: 'Unreleased', commits: [commit('e3', 'feat: wip')] }), unreleased: true };
  assert.deepEqual(appEntries([unreleased, released, empty]).map((e) => e.version), ['1.0.0']);
});
