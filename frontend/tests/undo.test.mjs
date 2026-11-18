import test from 'node:test';
import assert from 'node:assert/strict';
import { capture, record, redo, undo } from '../src/lib/undo.mjs';

test('the inverse only restores the keys the change touched', () => {
  const frame = { id: 7, manual_stars: 3, rejected: 0, picked: 1 };
  assert.deepEqual(capture(frame, { rejected: true }), { rejected: false });
  assert.deepEqual(capture(frame, { picked: false }), { picked: true });
  assert.deepEqual(capture(frame, { stars: null, rejected: false, picked: false }), { stars: 3, rejected: false, picked: true });
  assert.deepEqual(capture(null, { stars: 2 }), { stars: null });
});

test('undo and redo walk the same entries back and forth', () => {
  const a = { id: 1, restore: { rejected: false }, after: { rejected: true } };
  const b = { id: 2, restore: { picked: false }, after: { picked: true } };
  let past = record(record([], a), b);
  let future = [];
  const u1 = undo(past, future);
  assert.equal(u1.entry.id, 2);
  ({ past, future } = u1);
  const u2 = undo(past, future);
  assert.equal(u2.entry.id, 1);
  ({ past, future } = u2);
  assert.equal(undo(past, future), null, 'nothing left to undo');
  const r1 = redo(past, future);
  assert.equal(r1.entry.id, 1);
  ({ past, future } = r1);
  assert.equal(future.length, 1);
  assert.equal(redo(past, []), null);
});

test('the history is bounded and keeps the newest', () => {
  let past = [];
  for (let i = 0; i < 130; i += 1) past = record(past, { id: i, restore: {}, after: {} }, 100);
  assert.equal(past.length, 100);
  assert.equal(past[past.length - 1].id, 129);
  assert.equal(past[0].id, 30);
});
