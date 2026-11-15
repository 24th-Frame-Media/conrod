import test from 'node:test';
import assert from 'node:assert/strict';
import { albumSetting, defaultPrefs, normalisePrefs, parsePrefs, prefsKey } from '../src/lib/album.mjs';

test('stored view prefs are repaired: unknown values fall back, legacy view ids map to a current one', () => {
  assert.deepEqual(normalisePrefs(null), defaultPrefs);
  assert.deepEqual(normalisePrefs({ grouping: 'flat', sort: 'best', view: 'rejected', minStars: 3 }), { grouping: 'flat', sort: 'best', view: 'rejected', minStars: 3 });
  assert.equal(normalisePrefs({ view: 'picks' }).view, 'keepers');
  assert.equal(normalisePrefs({ view: 'kept' }).view, 'keepers');
  assert.equal(normalisePrefs({ view: 'stacks' }).view, 'all');
  assert.equal(normalisePrefs({ grouping: 'nonsense', sort: 7, minStars: 9 }).grouping, 'bursts');
  assert.equal(normalisePrefs({ minStars: 9 }).minStars, 0);
});

test('prefs survive a JSON round trip and bad JSON means defaults; keys are per album', () => {
  const prefs = { grouping: 'dupes', sort: 'worst', view: 'undecided', minStars: 2 };
  assert.deepEqual(parsePrefs(JSON.stringify(prefs)), prefs);
  assert.deepEqual(parsePrefs('{oops'), defaultPrefs);
  assert.deepEqual(parsePrefs(null), defaultPrefs);
  assert.notEqual(prefsKey(1), prefsKey(2));
});

test('an album setting is the album override, else the global default', () => {
  const job = { settings_json: JSON.stringify({ read_numbers: false }) };
  assert.equal(albumSetting(job, 'read_numbers', { read_numbers: true }), false);
  assert.equal(albumSetting(job, 'read_plates', { read_plates: true }), true);
  assert.equal(albumSetting({ settings_json: 'broken' }, 'read_plates', { read_plates: false }), false);
  assert.equal(albumSetting(undefined, 'read_plates', { read_plates: true }), true);
});
