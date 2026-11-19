import test from 'node:test';
import assert from 'node:assert/strict';
import {
  ageLabel, lightroomError, lightroomExportLine, lightroomImportLine, lightroomImportPercent,
  packageLine, pendingLine, pendingPackages, percentLabel,
} from '../src/lib/lightroom.mjs';

test('a percentage is a whole percent, a 0..1 share is read as one, and null stays null', () => {
  assert.equal(percentLabel(72), '72%');
  assert.equal(percentLabel(0.61), '61%');
  assert.equal(percentLabel(0), '0%');
  assert.equal(percentLabel(100), '100%');
  assert.equal(percentLabel(120), '100%');
  assert.equal(percentLabel(null), null);
  assert.equal(percentLabel(undefined), null);
});

test('the export result says how many frames went and which package waits', () => {
  assert.equal(
    lightroomExportLine({ package: 'bathurst-1000-1763600000', photos: 64 }),
    '64 frames for Lightroom · package bathurst-1000-1763600000, waiting for the plugin',
  );
  assert.equal(lightroomExportLine({ photos: 1 }), '1 frames for Lightroom, waiting for the plugin');
  assert.equal(lightroomExportLine(null), '0 frames for Lightroom, waiting for the plugin');
});

test('the import summary carries all three counts, and the percentage only when compared', () => {
  const result = { matched: 58, unmatched: 6, missing: 11, percent: 72 };
  assert.equal(lightroomImportLine(result), '58 matched · 6 unmatched · 11 frames with no catalog entry');
  assert.equal(lightroomImportLine({ ...result, missing: 1 }), '58 matched · 6 unmatched · 1 frame with no catalog entry');
  assert.equal(lightroomImportPercent(result), '72% matched your picks');
  assert.equal(lightroomImportPercent({ ...result, percent: null }), null);
  assert.equal(lightroomImportLine(null), '0 matched · 0 unmatched · 0 frames with no catalog entry');
});

test('an age reads in the unit that keeps it short', () => {
  assert.equal(ageLabel(30), 'just now');
  assert.equal(ageLabel(420), '7 min ago');
  assert.equal(ageLabel(7200), '2 h ago');
  assert.equal(ageLabel(259200), '3 d ago');
  assert.equal(ageLabel(-1), null);
  assert.equal(ageLabel(null), null);
});

test('waiting packages accept an array or a wrapper and the field names older writers used', () => {
  const rows = [
    { name: 'bathurst-1000-1763600000', album: 'Bathurst 1000', photos: 64, age_s: 420 },
    { package: 'phillip-island.json', label: 'Phillip Island GP', count: 37, age: 7200 },
  ];
  const expected = [
    { name: 'bathurst-1000-1763600000', album: 'Bathurst 1000', photos: 64, age: 420 },
    { name: 'phillip-island.json', album: 'Phillip Island GP', photos: 37, age: 7200 },
  ];
  assert.deepEqual(pendingPackages(rows), expected);
  assert.deepEqual(pendingPackages({ packages: rows }), expected);
  assert.deepEqual(pendingPackages(null), []);
  assert.deepEqual(pendingPackages({ packages: [null, 'nope'] }), []);
  assert.deepEqual(pendingPackages([{}]), [{ name: 'package', album: null, photos: null, age: null }]);
});

test('a package row drops what it does not know', () => {
  assert.equal(packageLine({ name: 'x', album: 'Bathurst 1000', photos: 64, age: 420 }), 'Bathurst 1000 · 64 photos · 7 min ago');
  assert.equal(packageLine({ name: 'x', photos: 1 }), '1 photo');
  assert.equal(packageLine({ name: 'x' }), '');
  assert.equal(pendingLine([]), 'No packages waiting');
  assert.equal(pendingLine([1]), '1 package waiting');
  assert.equal(pendingLine([1, 2]), '2 packages waiting');
});

test('an error line never echoes a key', () => {
  assert.equal(lightroomError(new Error('failed on sk-live-ABCDEFGH1234')), 'failed on sk-***');
  assert.equal(lightroomError('bad api_key=abc123secret here'), 'bad api_key=*** here');
  assert.equal(lightroomError(''), 'The Lightroom bridge could not be read.');
  assert.equal(lightroomError(new Error('package.json.tmp is not readable')), 'package.json.tmp is not readable');
});
