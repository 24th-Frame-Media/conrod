import test from 'node:test';
import assert from 'node:assert/strict';
import { exportLine, exportSummary } from '../src/lib/export.mjs';

const frames = [
  { id: 1, rejected: 0 },
  { id: 2, rejected: 0 },
  { id: 3, rejected: 1 },
  { id: 4, rejected: 0 },
];

test('export summary counts keepers, rejects and the rest', () => {
  const s = exportSummary(frames, new Set([1, 4]));
  assert.deepEqual(s, { total: 4, keepers: 2, rejected: 1, undecided: 1 });
});

test('a keeper that was also rejected counts as a keeper, and nothing is counted twice', () => {
  const s = exportSummary(frames, [3]);
  assert.equal(s.keepers + s.rejected + s.undecided, s.total);
  assert.deepEqual(s, { total: 4, keepers: 1, rejected: 0, undecided: 3 });
});

test('empty and missing input are safe', () => {
  assert.deepEqual(exportSummary([], null), { total: 0, keepers: 0, rejected: 0, undecided: 0 });
  assert.deepEqual(exportSummary(undefined, undefined), { total: 0, keepers: 0, rejected: 0, undecided: 0 });
  assert.match(exportLine({ total: 3061, keepers: 1173, rejected: 0, undecided: 1888 }), /3,061 photos · 1,173 keepers/);
});
