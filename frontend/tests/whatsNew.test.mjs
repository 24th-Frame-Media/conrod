import assert from 'node:assert/strict';
import test from 'node:test';
import {
  compareVersions, formatDate, isVersion, orderEntries, readSeen, shouldShowWhatsNew, splitEntries, writeSeen,
} from '../src/lib/whatsNew.mjs';

/** A key/value store stand-in, so tests never touch the real localStorage. */
function fakeStorage(initial = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (key) => (data.has(key) ? data.get(key) : null),
    setItem: (key, value) => { data.set(key, value); },
  };
}

const entry = (version, extra = {}) => ({ version, sections: [], ...extra });

test('compareVersions orders by number, then puts a pre-release before its stable release', () => {
  assert.ok(compareVersions('1.0.0', '1.0.1') < 0);
  assert.ok(compareVersions('1.1.0', '1.0.9') > 0);
  assert.ok(compareVersions('1.0.0-beta.2', '1.0.0-beta.10') < 0);
  assert.ok(compareVersions('1.0.0-beta.9', '1.0.0') < 0);
  assert.equal(compareVersions('1.0.0', '1.0.0'), 0);
  assert.equal(compareVersions('1.0.0-beta.1', '1.0.0-beta.1'), 0);
});

test('isVersion only accepts an X.Y.Z shape, with or without a leading v', () => {
  assert.ok(isVersion('1.0.0'));
  assert.ok(isVersion('v1.0.0-beta.3'));
  assert.ok(!isVersion('Unreleased'));
  assert.ok(!isVersion(undefined));
  assert.ok(!isVersion(42));
});

test('orderEntries drops the unreleased entry and sorts released ones newest first', () => {
  const entries = [entry('0.6.0'), entry('Unreleased', { unreleased: true }), entry('1.0.0-beta.2'), entry('1.0.0-beta.10'), entry('not-a-version')];
  assert.deepEqual(orderEntries(entries).map((e) => e.version), ['1.0.0-beta.10', '1.0.0-beta.2', '0.6.0']);
  assert.deepEqual(orderEntries(null), []);
});

test('shouldShowWhatsNew opens once per version bump: unseen build, newer than last seen, never a downgrade', () => {
  const entries = [entry('1.0.0'), entry('1.1.0')];
  assert.equal(shouldShowWhatsNew({ current: '1.1.0', lastSeen: null, entries }), true);
  assert.equal(shouldShowWhatsNew({ current: '1.1.0', lastSeen: '1.0.0', entries }), true);
  assert.equal(shouldShowWhatsNew({ current: '1.0.0', lastSeen: '1.1.0', entries }), false);
  assert.equal(shouldShowWhatsNew({ current: '1.1.0', lastSeen: '1.1.0', entries }), false);
  assert.equal(shouldShowWhatsNew({ current: null, lastSeen: null, entries }), false);
  // The running build has no entry of its own: nothing to show.
  assert.equal(shouldShowWhatsNew({ current: '1.2.0', lastSeen: '1.0.0', entries }), false);
});

test('splitEntries: fresh is newest-first since lastSeen (at least the newest), earlier is the rest', () => {
  const entries = [entry('1.0.0'), entry('1.1.0'), entry('1.2.0')];
  const { fresh, earlier } = splitEntries({ entries, current: '1.2.0', lastSeen: '1.0.0' });
  assert.deepEqual(fresh.map((e) => e.version), ['1.2.0', '1.1.0']);
  assert.deepEqual(earlier.map((e) => e.version), ['1.0.0']);

  // Nothing ever seen: only the newest release counts as fresh.
  const first = splitEntries({ entries, current: '1.2.0', lastSeen: null });
  assert.deepEqual(first.fresh.map((e) => e.version), ['1.2.0']);
  assert.deepEqual(first.earlier.map((e) => e.version), ['1.1.0', '1.0.0']);

  // No known current version (a browser run): everything released counts, newest first.
  const browser = splitEntries({ entries, current: null, lastSeen: null });
  assert.deepEqual(browser.fresh.map((e) => e.version), ['1.2.0']);
  assert.deepEqual(browser.earlier.map((e) => e.version), ['1.1.0', '1.0.0']);

  // A build older than the newest release never shows notes ahead of itself.
  const older = splitEntries({ entries, current: '1.1.0', lastSeen: null });
  assert.deepEqual(older.fresh.map((e) => e.version), ['1.1.0']);
  assert.deepEqual(older.earlier.map((e) => e.version), ['1.0.0']);
});

test('readSeen/writeSeen round-trip through a key/value store and never move backwards', () => {
  const storage = fakeStorage();
  assert.equal(readSeen(storage), null);
  assert.equal(writeSeen(storage, '1.0.0'), '1.0.0');
  assert.equal(readSeen(storage), '1.0.0');
  assert.equal(writeSeen(storage, '1.1.0'), '1.1.0');
  assert.equal(writeSeen(storage, '1.0.0'), '1.1.0'); // a downgrade does not move the mark back
  assert.equal(readSeen(storage), '1.1.0');
});

test('readSeen degrades to never-seen when storage throws or is missing', () => {
  const broken = {
    getItem() { throw new Error('blocked'); },
    setItem() { throw new Error('blocked'); },
  };
  assert.equal(readSeen(broken), null);
  assert.equal(readSeen(null), null);
  assert.equal(readSeen(undefined), null);
});

test('writeSeen still returns the new version (for in-memory state) when persisting it fails', () => {
  const broken = { getItem() { throw new Error('blocked'); }, setItem() { throw new Error('blocked'); } };
  assert.equal(writeSeen(broken, '1.0.0'), '1.0.0');
  // No storage at all (optional chaining, not a throw) still returns the computed version.
  assert.equal(writeSeen(null, '1.0.0'), '1.0.0');
  // With no version to remember, the previous mark (or null) comes back unchanged.
  assert.equal(writeSeen(fakeStorage({ 'conrod:whats-new-seen': '1.0.0' }), null), '1.0.0');
});

test('formatDate turns YYYY-MM-DD into a short, human date and rejects anything else', () => {
  assert.equal(formatDate('2026-11-20'), '20 Nov 2026');
  assert.equal(formatDate('2026-01-05'), '5 Jan 2026');
  assert.equal(formatDate(undefined), null);
  assert.equal(formatDate('not a date'), null);
});
