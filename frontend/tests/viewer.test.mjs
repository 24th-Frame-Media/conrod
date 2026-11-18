import test from 'node:test';
import assert from 'node:assert/strict';
import { afBox, afLabel } from '../src/lib/viewer.mjs';

test('no AF box means no box and no label', () => {
  assert.equal(afBox(null), null);
  assert.equal(afBox({}), null);
  assert.equal(afBox({ af_x1: 0.5, af_y1: 0.5, af_x2: null, af_y2: 0.6 }), null);
  assert.equal(afLabel(null), null);
  assert.equal(afLabel({ id: 1 }), null);
  assert.equal(afLabel({ af_x1: 0.5, af_y1: 0.5, af_x2: null, af_y2: 0.6 }), null);
});

test('an AF box reads as its centre in rounded percentages', () => {
  const frame = { af_x1: 0.52, af_y1: 0.54, af_x2: 0.6, af_y2: 0.62 };
  assert.deepEqual(afBox(frame), [0.52, 0.54, 0.6, 0.62]);
  assert.equal(afLabel(frame), '56% × 58%');
  // A full-frame area focuses on the centre.
  assert.equal(afLabel({ af_x1: 0, af_y1: 0, af_x2: 1, af_y2: 1 }), '50% × 50%');
});

test('the box is clamped to the photo, and an inverted one is dropped', () => {
  assert.deepEqual(afBox({ af_x1: -0.1, af_y1: 0.2, af_x2: 1.2, af_y2: 0.9 }), [0, 0.2, 1, 0.9]);
  assert.equal(afBox({ af_x1: 0.6, af_y1: 0.2, af_x2: 0.4, af_y2: 0.9 }), null);
  assert.equal(afBox({ af_x1: 0.4, af_y1: 0.6, af_x2: 0.5, af_y2: 0.4 }), null);
  assert.equal(afBox({ af_x1: 0.4, af_y1: 0.2, af_x2: 0.4, af_y2: 0.9 }), null);
});
