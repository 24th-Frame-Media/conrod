import assert from 'node:assert/strict';
import test from 'node:test';
import { tasteOf, tasteOrder, tasteStacks } from '../src/lib/taste.mjs';

const f = (id, taste) => (taste === undefined ? { id } : { id, taste });

test('tasteOrder puts the highest score first, unscored frames last', () => {
  const out = tasteOrder([f(1, 0.2), f(2), f(3, 0.9), f(4, 0.5)]);
  assert.deepEqual(out.map((x) => x.id), [3, 4, 1, 2]);
  assert.deepEqual(tasteOrder([]), []);
});

test('tasteOrder ties break by id, so the order is stable across reloads', () => {
  assert.deepEqual(tasteOrder([f(9, 0.4), f(5, 0.4)]).map((x) => x.id), [5, 9]);
});

test('tasteOf ignores a missing, null or non-finite score', () => {
  assert.equal(tasteOf(f(1)), null);
  assert.equal(tasteOf({ id: 1, taste: null }), null);
  assert.equal(tasteOf({ id: 1, taste: Number.NaN }), null);
  assert.equal(tasteOf({ id: 1, taste: 0 }), 0);
});

test('tasteStacks orders frames inside a burst and bursts by their lead', () => {
  const stacks = [
    { key: 'b1', lead: f(1, 0.1), frames: [f(2, 0.6), f(1, 0.1), f(3, 0.4)] },
    { key: 'b2', lead: f(4, 0.8), frames: [f(4, 0.8), f(5, 0.2)] },
    { key: 'b3', lead: f(6), frames: [f(6)] },
  ];
  const out = tasteStacks(stacks);
  assert.deepEqual(out.map((s) => s.key), ['b2', 'b1', 'b3'], 'lead taste decides the burst order');
  assert.deepEqual(out[1].frames.map((x) => x.id), [2, 3, 1], 'inside the burst, taste decides');
  assert.deepEqual(out[0].frames.map((x) => x.id), [4, 5]);
  assert.equal(out[1].lead.id, 1, 'the lead (the keeper, when there is one) does not move');
  // The input is left alone: the grid rebuilds from it on the next render.
  assert.deepEqual(stacks[0].frames.map((x) => x.id), [2, 1, 3]);
});
