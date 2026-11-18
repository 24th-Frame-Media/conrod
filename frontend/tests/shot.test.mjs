import test from 'node:test';
import assert from 'node:assert/strict';
import {
  formatShutter, formatAperture, formatFocal, formatBytes, formatEv, formatDistance, formatTaken,
  shotRows, statSections,
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

const stats = {
  frames: 6912, copies_hidden: 2437, size_bytes: Math.round(24.3 * 1024 ** 2), backfilled: 0,
  cameras: [
    { name: 'Canon EOS R7', count: 5000 },
    { name: 'Canon EOS R5', count: 1000 },
    { name: 'Canon EOS 5D', count: 600 },
    { name: 'Canon EOS M50', count: 312 },
  ],
  lenses: [
    { name: 'EF100-400mm', count: 5000 },
    { name: 'RF24-70mm', count: 1912 },
  ],
  most_used: { camera: 'Canon EOS R7', lens: 'EF100-400mm', f_number: 5.6, shutter: 0.001, iso: 400, focal_length: 400 },
  f_numbers: [{ value: 5.6, count: 3120 }, { value: 4, count: 2010 }],
  shutters: [{ value: 0.001, count: 2400 }, { value: 0.00125, count: 1200 }],
  isos: [{ value: 100, count: 3000 }, { value: 6400, count: 900 }, { value: 400, count: 3012 }],
  focal_lengths: [{ value: 100, count: 1000 }, { value: 400, count: 5000 }],
  pans: 12, bursts: 824,
  blurred: 246, soft: 5204, sharp: 3284,
  af: { frames: 6912, missing: 10, off_subject: 123, no_subject: 80, soft: 0 },
  keepers: 1023, rejected: 123,
  stars: [{ value: 0, count: 500 }, { value: 3, count: 120 }, { value: 5, count: 40 }, { value: 4, count: 300 }],
  taken_from: 1760000000, taken_to: 1760007200,
};

const rowsOf = (title) => Object.fromEntries(statSections(stats).find((s) => s.title === title).rows.map((r) => [r.label, r.value]));

test('the card groups gear, exposure and the shoot, formatted for reading', () => {
  const sections = statSections(stats);
  assert.deepEqual(sections.map((s) => s.title), ['Gear', 'Exposure', 'Shoot']);

  assert.deepEqual(rowsOf('Gear'), {
    Cameras: 'Canon EOS R7 × 5,000 · Canon EOS R5 × 1,000 · 2 more',
    Lenses: 'EF100-400mm × 5,000 · RF24-70mm × 1,912',
    'Most used': 'Canon EOS R7 · EF100-400mm',
  });
  assert.deepEqual(rowsOf('Exposure'), {
    Aperture: 'ƒ/5.6',
    Shutter: '1/1000 s',
    ISO: '400',
    'Focal length': '400 mm',
    'ISO range': '100–6,400',
    'Focal range': '100–400 mm',
  });
  const lines = Object.fromEntries(sections.find((s) => s.title === 'Exposure').lines.map((l) => [l.label, l.value]));
  assert.deepEqual(lines, {
    'f-stops': 'ƒ/5.6 × 3,120 · ƒ/4 × 2,010',
    shutters: '1/1000 × 2,400 · 1/800 × 1,200',
  });
  assert.deepEqual(rowsOf('Shoot'), {
    Frames: '6,912 · 2,437 hidden',
    Size: '24.3 MB',
    Duration: '2 h',
    Bursts: '824',
    Pans: '12',
    Subjects: '246 blurred · 5,204 soft · 3,284 sharp',
    'AF misses': '213',
    Keepers: '1,023 kept · 123 rejected',
    Stars: '★5 40 · ★4 300 · ★3 120',
  });
});

test('the top two names carry a count of the rest, and no tail when there are only two', () => {
  const two = statSections({ ...stats, cameras: stats.cameras.slice(0, 2) });
  assert.equal(two.find((s) => s.title === 'Gear').rows[0].value, 'Canon EOS R7 × 5,000 · Canon EOS R5 × 1,000');
  const three = statSections({ ...stats, cameras: stats.cameras.slice(0, 3) });
  assert.equal(three.find((s) => s.title === 'Gear').rows[0].value, 'Canon EOS R7 × 5,000 · Canon EOS R5 × 1,000 · 1 more');
});

test('empty or partially filled statistics keep the card small rather than printing blanks', () => {
  assert.deepEqual(statSections(null), []);
  assert.deepEqual(statSections({}), []);
  const bare = statSections({ frames: 4, taken_from: null, taken_to: null, most_used: { lens: null } });
  assert.deepEqual(bare, [{ key: 'shoot', title: 'Shoot', rows: [{ label: 'Frames', value: '4' }] }]);
  // No lens read: the most-used row still names the camera.
  const cameraOnly = statSections({ frames: 1, most_used: { camera: 'Canon EOS R7', lens: null } });
  assert.deepEqual(cameraOnly[0].rows[0], { label: 'Most used', value: 'Canon EOS R7' });
});
