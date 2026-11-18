import test from 'node:test';
import assert from 'node:assert/strict';
import {
  sharePercent, averageBytes, topN, stackedSegments, shootTiles, cameraRows, lensRows,
  exposureTiles, bucketRows, rangeChip, exposureChips, qualityParts, afParts, shootMiniStats,
  takenRange,
} from '../src/lib/shotStats.mjs';
import { formatAperture, formatShutter, formatBytes } from '../src/lib/shot.mjs';

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
  stars: [{ value: 0, count: 500 }, { value: 3, count: 120 }],
  taken_from: new Date(Date.UTC(2025, 9, 8, 9, 12)).getTime() / 1000,
  taken_to: new Date(Date.UTC(2025, 9, 8, 11, 12)).getTime() / 1000,
};

test('a share is a whole percent of the frames, and nothing when the whole is empty', () => {
  assert.equal(sharePercent(3150, 6912), 46);
  assert.equal(sharePercent(0, 6912), 0);
  assert.equal(sharePercent(9000, 6912), 100);
  assert.equal(sharePercent(-2, 6912), 0);
  assert.equal(sharePercent(10, 0), null);
  assert.equal(sharePercent(null, 6912), null);
  assert.equal(sharePercent(10, undefined), null);
});

test('the average frame size is the album bytes over its frames', () => {
  assert.equal(averageBytes(24 * 1024 ** 2, 24), 1024 ** 2);
  assert.equal(averageBytes(stats.size_bytes, stats.frames), Math.round(stats.size_bytes / stats.frames));
  assert.equal(averageBytes(1024, 0), null);
  assert.equal(averageBytes(-1, 10), null);
  assert.equal(averageBytes(null, 10), null);
});

test('the top entries are sorted by count, with the tail measured not shown', () => {
  const { rows, more } = topN(stats.cameras, 2);
  assert.deepEqual(rows.map((row) => row.name), ['Canon EOS R7', 'Canon EOS R5']);
  assert.equal(more, 2);
  assert.equal(topN(stats.cameras, 10).more, 0);
  // A zero-count entry is not a row, and a shuffled list still reads biggest first.
  const shuffled = [{ name: 'b', count: 2 }, { name: 'none', count: 0 }, { name: 'a', count: 9 }];
  assert.deepEqual(topN(shuffled, 2).rows.map((row) => row.name), ['a', 'b']);
});

test('stacked parts drop the zeros and read as whole percentages of what is left', () => {
  const segments = stackedSegments([
    { key: 'sharp', label: 'Sharp', tone: 'good', count: 100 },
    { key: 'soft', label: 'Soft', tone: 'warn', count: 50 },
    { key: 'blurred', label: 'Blurred', tone: 'bad', count: 0 },
  ]);
  assert.deepEqual(segments.map((s) => s.key), ['sharp', 'soft']);
  assert.deepEqual(segments.map((s) => s.percent), [67, 33]);
  assert.equal(segments[0].countLabel, '100');
  assert.deepEqual(stackedSegments(null), []);
});

test('the hero tiles carry the numbers and their one-line subs', () => {
  const tiles = shootTiles(stats);
  assert.deepEqual(tiles.map((tile) => tile.label), ['Frames', 'Size', 'Duration', 'Keepers']);
  assert.equal(tiles[0].value, '6,912');
  assert.equal(tiles[0].sub, '2,437 hidden');
  assert.equal(tiles[1].value, '24.3 MB');
  assert.equal(tiles[1].sub, `${formatBytes(Math.round(stats.size_bytes / stats.frames))} average`);
  assert.equal(tiles[2].value, '2 h');
  assert.match(tiles[2].sub, /· /);
  assert.match(tiles[2].sub, /–/);
  assert.equal(tiles[3].value, '1,023');
  assert.equal(tiles[3].sub, '123 rejected · 824 bursts');
  // Nothing in, nothing out; a hidden-copy count of zero is not a sub-line.
  assert.deepEqual(shootTiles(null), []);
  assert.equal(shootTiles({ frames: 4, copies_hidden: 0 })[0].sub, null);
});

test('a one-day shoot reads as a compact date and clock range', () => {
  const from = new Date(Date.UTC(2025, 9, 8, 9, 12)).getTime() / 1000;
  const to = new Date(Date.UTC(2025, 9, 8, 15, 51)).getTime() / 1000;
  const sub = takenRange(from, to);
  assert.match(sub, /8 Oct 2025/);
  assert.match(sub, /09:12–15:51/);
  assert.match(sub, /· /);
  assert.match(sub, /–/);
  // Across midnight the date is written once and the clocks keep their range.
  const overnight = takenRange(new Date(Date.UTC(2025, 9, 8, 23, 0)).getTime() / 1000, new Date(Date.UTC(2025, 9, 9, 1, 0)).getTime() / 1000);
  assert.match(overnight, /8–9 Oct 2025/);
  assert.doesNotMatch(overnight, /→/);
  assert.equal(takenRange(null, 1), null);
  assert.equal(takenRange(10, 10), null);
});

test('camera and lens rows name the biggest first and flag the most-used lens', () => {
  const cameras = cameraRows(stats);
  assert.deepEqual(cameras.map((row) => row.name), ['Canon EOS R7', 'Canon EOS R5', 'Canon EOS 5D', 'Canon EOS M50']);
  assert.equal(cameras[0].countLabel, '5,000');
  assert.equal(cameras[0].share, 72);

  const lenses = lensRows(stats);
  assert.deepEqual(lenses.rows.map((row) => row.name), ['EF100-400mm', 'RF24-70mm']);
  assert.equal(lenses.rows[0].mostUsed, true);
  assert.equal(lenses.rows[1].mostUsed, false);
  assert.equal(lenses.rows[1].share, 28);
  assert.equal(lenses.more, 0);

  const six = lensRows({ ...stats, lenses: [...stats.lenses, { name: '50mm', count: 100 }, { name: '85mm', count: 90 }, { name: '24mm', count: 80 }, { name: '135mm', count: 70 }] });
  assert.equal(six.rows.length, 5);
  assert.equal(six.more, 1);
});

test('exposure tiles read the most-used aperture, shutter, ISO and focal length', () => {
  assert.deepEqual(exposureTiles(stats).map((tile) => tile.label), ['Aperture', 'Shutter', 'ISO', 'Focal length']);
  assert.deepEqual(exposureTiles(stats).map((tile) => tile.value), ['ƒ/5.6', '1/1000 s', '400', '400 mm']);
  assert.deepEqual(exposureTiles({ most_used: { iso: null } }), []);
});

test('bucket rows label the top values, with bars against the top and shares of the frames', () => {
  const apertures = bucketRows(stats.f_numbers, formatAperture, stats.frames);
  assert.deepEqual(apertures.map((row) => row.label), ['ƒ/5.6', 'ƒ/4']);
  assert.deepEqual(apertures.map((row) => row.countLabel), ['3,120', '2,010']);
  assert.deepEqual(apertures.map((row) => row.share), [45, 29]);
  assert.deepEqual(apertures.map((row) => row.bar), [100, 64]);

  const shutters = bucketRows(stats.shutters, formatShutter, stats.frames, 1);
  assert.deepEqual(shutters.map((row) => row.label), ['1/1000 s']);
  assert.equal(shutters[0].share, 35);
  assert.equal(shutters[0].bar, 100);
  // A formatter that cannot read a value drops that row rather than printing a blank.
  assert.deepEqual(bucketRows([{ value: null, count: 2 }], formatAperture, 4), []);
});

test('the ranges become small chips, one value when the shoot never changed it', () => {
  assert.equal(rangeChip(stats.focal_lengths, String), '100–400');
  assert.equal(rangeChip([{ value: 100 }], (v) => String(v)), '100');
  assert.equal(rangeChip([], String), null);
  assert.deepEqual(exposureChips(stats), ['ISO 100–6,400', '100–400 mm']);
  assert.deepEqual(exposureChips({ isos: [{ value: 400, count: 3 }] }), ['ISO 400']);
});

test('quality reads green, amber and red, and the AF split only lists real causes', () => {
  const quality = qualityParts(stats);
  assert.deepEqual(quality.map((segment) => segment.key), ['sharp', 'soft', 'blurred']);
  assert.deepEqual(quality.map((segment) => segment.tone), ['good', 'warn', 'bad']);
  assert.deepEqual(quality.map((segment) => segment.countLabel), ['3,284', '5,204', '246']);
  // Percent of the frames that got a subject verdict, not a hard-coded 100 split.
  assert.equal(quality[0].percent, 38);

  const af = afParts(stats);
  assert.equal(af.misses, 213);
  assert.equal(af.missesLabel, '213');
  assert.deepEqual(af.items.map((item) => item.label), ['off subject', 'no subject', 'no AF']);
  assert.deepEqual(af.items.map((item) => item.countLabel), ['123', '80', '10']);
  assert.equal(afParts({ af: { frames: 0, missing: 3 } }), null);
  assert.equal(afParts(null), null);

  assert.deepEqual(shootMiniStats(stats), [
    { key: 'pans', label: 'Pans', value: '12' },
    { key: 'bursts', label: 'Bursts', value: '824' },
    { key: 'af-misses', label: 'AF misses', value: '213' },
  ]);
});

test('partially filled statistics leave the empty sections empty', () => {
  assert.deepEqual(shootTiles({}), []);
  assert.deepEqual(cameraRows({}), []);
  assert.deepEqual(exposureTiles({}), []);
  assert.deepEqual(qualityParts({}), []);
  assert.deepEqual(shootMiniStats({}), []);
  assert.deepEqual(lensRows({ lenses: [{ name: '  ', count: 3 }] }).rows, []);
});
