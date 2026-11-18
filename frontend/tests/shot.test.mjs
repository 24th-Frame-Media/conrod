import test from 'node:test';
import assert from 'node:assert/strict';
import {
  formatShutter, formatAperture, formatFocal, formatBytes, formatEv, formatDistance, formatTaken,
  shotRows,
} from '../src/lib/shot.mjs';

test('a shutter reads as a fraction, then as seconds', () => {
  assert.equal(formatShutter(0.001), '1/1000 s');
  assert.equal(formatShutter(0.5), '0.5 s');
  assert.equal(formatShutter(1.3), '1.3 s');
  // A file with no exposure time, or a zero left behind by an encoder, has nothing to say.
  assert.equal(formatShutter(0), null);
  assert.equal(formatShutter(undefined), null);
  assert.equal(formatShutter(null), null);
});

test('an aperture, a focal length and a byte count read in camera words', () => {
  assert.equal(formatAperture(5.6), 'ƒ/5.6');
  assert.equal(formatAperture(4), 'ƒ/4');
  assert.equal(formatAperture(null), null);
  assert.equal(formatFocal(400), '400 mm');
  assert.equal(formatFocal(24), '24 mm');
  assert.equal(formatFocal(0), null);
  assert.equal(formatBytes(Math.round(24.3 * 1024 ** 2)), '24.3 MB');
  assert.equal(formatBytes(Math.round(12.4 * 1024 ** 3)), '12.4 GB');
  assert.equal(formatBytes(null), null);
});

test('exposure bias keeps its sign, distance and taken time stay short', () => {
  assert.equal(formatEv(0.3), '+0.3 EV');
  assert.equal(formatEv(0), '0.0 EV');
  assert.equal(formatEv(-0.7), '-0.7 EV');
  assert.equal(formatEv(undefined), null);
  assert.equal(formatDistance(12.5), '12.5 m');
  assert.equal(formatDistance(3), '3 m');
  assert.equal(formatDistance(null), null);
  assert.match(formatTaken(1760000000), /2025/);
  // EXIF time is a wall clock, not an instant: 07:40 UTC is the camera's 07:40, even in Sydney.
  assert.match(formatTaken(Date.UTC(2026, 9, 8, 7, 40) / 1000), /07:40/);
  assert.equal(formatTaken(0), null);
  assert.equal(formatTaken(null), null);
});

test('a frame with no shot data contributes no rows', () => {
  assert.deepEqual(shotRows(null), []);
  assert.deepEqual(shotRows({}), []);
  assert.deepEqual(shotRows({ camera: null, lens: '  ', iso: null, exposure_time: 0, taken_at: 0 }), []);
});

test('shot rows follow the viewer order and drop only absent values', () => {
  const frame = {
    camera: 'Canon EOS R7', lens: 'EF100-400mm f/4.5-5.6L IS II USM',
    focal_length: 400, f_number: 5.6, exposure_time: 0.001, iso: 400, exposure_bias: 0.3,
    focus_distance: 12.5, file_size: Math.round(24.3 * 1024 ** 2), taken_at: 1760000000,
  };
  const rows = shotRows(frame);
  assert.deepEqual(rows.map((r) => r.label), [
    'Camera', 'Lens', 'Focal length', 'Aperture', 'Shutter', 'ISO',
    'Exposure bias', 'Focus distance', 'File size', 'Taken',
  ]);
  assert.deepEqual(rows.slice(0, 9).map((r) => r.value), [
    'Canon EOS R7', 'EF100-400mm f/4.5-5.6L IS II USM', '400 mm', 'ƒ/5.6', '1/1000 s',
    '400', '+0.3 EV', '12.5 m', '24.3 MB',
  ]);
  assert.match(rows[9].value, /2025/);
});
