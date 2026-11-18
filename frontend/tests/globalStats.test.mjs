import test from 'node:test';
import assert from 'node:assert/strict';
import {
  measuredSubjects, sharpRate, keeperRate, globalTiles, completeNote,
  globalGear, globalCameraRows, globalLensRows,
} from '../src/lib/globalStats.mjs';

/** The contract's sample reply: three albums, two of them with complete shot data. */
const stats = {
  albums: 3, complete_albums: 2, frames: 23456, complete_frames: 15000,
  copies_hidden: 120, size_bytes: 123456789,
  cameras: [{ name: 'Canon EOS R7 358', count: 12000 }],
  lenses: [
    { name: 'RF-S 18-150mm', count: 9000 },
    { name: 'EF100-400mm', count: 3000 },
    { name: 'RF24-70mm', count: 2000 },
    { name: '50mm', count: 700 },
    { name: '85mm', count: 200 },
    { name: '24mm', count: 100 },
  ],
  most_used: { camera: 'Canon EOS R7 358', lens: 'RF-S 18-150mm', f_number: 6.3, shutter: 0.008, iso: 100, focal_length: 50 },
  f_numbers: [{ value: 6.3, count: 1149 }],
  shutters: [{ value: 0.008, count: 1223 }],
  isos: [{ value: 100, count: 3327 }],
  focal_lengths: [{ value: 50, count: 2000 }],
  pans: 2673, bursts: 824,
  blurred: 246, soft: 5204, sharp: 3284,
  af: { frames: 23456, missing: 0, off_subject: 291, no_subject: 80, soft: 48 },
  keepers: 1022, rejected: 3,
  shooting_seconds: 72000, average_shoot_seconds: 7200, longest_shoot_seconds: 9000, timed_albums: 10,
};

test('an empty library has no rates, no tiles and no note', () => {
  assert.equal(measuredSubjects({}), 0);
  assert.equal(measuredSubjects(null), 0);
  assert.equal(sharpRate({}), null);
  assert.equal(sharpRate({ sharp: 0, soft: 0, blurred: 0 }), null);
  assert.equal(keeperRate({}), null);
  assert.equal(keeperRate({ frames: 0, keepers: 0 }), null);
  assert.deepEqual(globalTiles({}), []);
  assert.deepEqual(globalTiles({ frames: 0, keepers: 0 }), []);
  assert.deepEqual(globalTiles(null), []);
  assert.equal(completeNote({}), null);
  assert.deepEqual(globalCameraRows({}), []);
  assert.deepEqual(globalLensRows({}), { rows: [], more: 0 });
});

test('the hit rate is the sharp share of the subjects that got a verdict', () => {
  assert.equal(measuredSubjects(stats), 8734);
  assert.equal(sharpRate(stats), 38);
  assert.equal(sharpRate({ sharp: 2, soft: 1, blurred: 0 }), 67);
  assert.equal(sharpRate({ sharp: 1, soft: 2, blurred: 0 }), 33);
  assert.equal(sharpRate({ sharp: 100, soft: 0, blurred: 0 }), 100);
  // Keepers are not the hit rate: a sharp subject rate stands on the subject verdicts alone.
  assert.equal(sharpRate({ sharp: 1, soft: 3, blurred: 0, keepers: 99, frames: 100 }), 25);
});

test('the keeper rate is effective keepers over every frame', () => {
  assert.equal(keeperRate(stats), 4);
  assert.equal(keeperRate({ frames: 3, keepers: 1 }), 33);
  assert.equal(keeperRate({ frames: 10, keepers: 0 }), 0);
  assert.equal(keeperRate({ frames: 0, keepers: 5 }), null);
});

test('the hero tiles read frames, time, hit rate and keepers in that order', () => {
  const tiles = globalTiles(stats);
  assert.deepEqual(tiles.map((tile) => tile.key), ['frames', 'time', 'hit-rate', 'keepers']);
  assert.deepEqual(tiles.map((tile) => tile.label), ['Frames', 'Time shooting', 'Hit rate', 'Keepers']);
  assert.equal(tiles[0].value, '23,456');
  assert.equal(tiles[0].sub, '3 albums · 120 hidden');
  assert.equal(tiles[1].value, '20 h');
  assert.equal(tiles[1].sub, 'avg 2 h per shoot');
  // A long total takes the short minute word so it fits the hero tile.
  assert.equal(
    globalTiles({ ...stats, shooting_seconds: 41136, average_shoot_seconds: 20568 })[1].value,
    '11 h 26 m',
  );
  assert.equal(tiles[2].value, '38%');
  assert.equal(tiles[2].sub, '3,284 of 8,734 subjects sharp');
  assert.equal(tiles[3].value, '1,022');
  assert.equal(tiles[3].sub, '4% of frames kept · 3 rejected');
});

test('a library with no timed album drops the time tile', () => {
  const keys = ['frames', 'hit-rate', 'keepers'];
  assert.deepEqual(globalTiles({ ...stats, timed_albums: 0 }).map((tile) => tile.key), keys);
  assert.deepEqual(globalTiles({ ...stats, shooting_seconds: 0 }).map((tile) => tile.key), keys);
  assert.deepEqual(globalTiles({ ...stats, timed_albums: undefined }).map((tile) => tile.key), keys);
});

test('the frames tile counts albums and only mentions hidden copies when there are some', () => {
  assert.equal(globalTiles({ ...stats, copies_hidden: 0 })[0].sub, '3 albums');
  assert.equal(globalTiles({ ...stats, albums: 1, copies_hidden: 0 })[0].sub, '1 album');
  assert.equal(globalTiles({ ...stats, albums: 0, copies_hidden: 0 })[0].sub, null);
  assert.equal(globalTiles({ ...stats, rejected: 0 })[3].sub, '4% of frames kept');
});

test('the completeness note names the albums that carry gear and exposure data', () => {
  assert.equal(completeNote(stats), 'Gear and exposure from 2 of 3 albums with complete shot data.');
  assert.equal(completeNote({ ...stats, complete_albums: 3 }), null);
  assert.equal(completeNote({ ...stats, complete_albums: 0 }), null);
  assert.equal(completeNote({ albums: 1, complete_albums: 0 }), null);
  assert.equal(completeNote({ albums: 2 }), null);
});

test('gear shares are of the complete albums frames, not the whole library', () => {
  const gear = globalGear(stats);
  assert.equal(gear.frames, 15000);
  assert.deepEqual(gear.most_used, { camera: 'Canon EOS R7 358', lens: 'RF-S 18-150mm' });

  const cameras = globalCameraRows(stats);
  assert.deepEqual(cameras.map((row) => row.name), ['Canon EOS R7 358']);
  assert.equal(cameras[0].share, 80);

  const lenses = globalLensRows(stats);
  assert.equal(lenses.rows.length, 5);
  assert.equal(lenses.more, 1);
  assert.deepEqual(lenses.rows.map((row) => row.name), ['RF-S 18-150mm', 'EF100-400mm', 'RF24-70mm', '50mm', '85mm']);
  assert.equal(lenses.rows[0].share, 60);
  assert.equal(lenses.rows[0].mostUsed, true);
  assert.equal(lenses.rows[1].mostUsed, false);
  // The partial album's frames would have made every bar smaller.
  assert.equal(globalCameraRows({ ...stats, frames: 23456, complete_frames: 0, complete_albums: 0 })[0].share, null);
});
