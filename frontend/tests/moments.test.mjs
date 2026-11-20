import test from 'node:test';
import assert from 'node:assert/strict';
import { describe, labelOf, liftNote, momentsOf, parseVerdict, sureTags, thresholdOf } from '../src/lib/moments.mjs';
import policies from '../src/lib/presetPolicies.mjs';

const stored = JSON.stringify({
  backend: 'decider', strength: 0.9, peak: 0.8,
  readings: [{ tag: 'first_kiss', p: 0.97, calibrated: true }, { tag: 'confetti', p: 0.7, calibrated: true }, { tag: 'vows', p: 0.2, calibrated: true }],
});
const vocab = policies.wedding.moments.vocabulary;

test('sure moments clear the threshold, maybes sit between 0.5 and it, the rest are hidden', () => {
  const m = momentsOf(stored, vocab, 0.85);
  assert.deepEqual(m.sure.map((x) => x.tag), ['first_kiss']);
  assert.deepEqual(m.maybe.map((x) => x.tag), ['confetti']);
  assert.equal(m.peak, true);
  assert.equal(m.sure[0].label, 'First kiss');
  // The album's own bar moves the line.
  assert.deepEqual(momentsOf(stored, vocab, 0.6).sure.map((x) => x.tag), ['first_kiss', 'confetti']);
  assert.deepEqual(momentsOf(stored, vocab, 0.99).sure, []);
  assert.deepEqual(sureTags(stored), ['first_kiss']);
});

test('a broken or missing verdict is empty, never an error', () => {
  for (const bad of [null, undefined, '', 'nope', '[]', '{"readings":"x"}', 5]) {
    const m = momentsOf(bad, vocab);
    assert.deepEqual([m.sure, m.maybe, m.peak], [[], [], false]);
  }
  assert.equal(parseVerdict('{"readings":[{"tag":3,"p":1},{"tag":"a","p":"x"}]}').readings.length, 0);
});

test('thresholds the engine would refuse fall back to the default', () => {
  assert.equal(thresholdOf(0.9), 0.9);
  for (const bad of [0.2, 1.5, NaN, 'x', undefined, null]) assert.equal(thresholdOf(bad), 0.85);
});

test('the lift is explained, and only with ranking on', () => {
  assert.equal(liftNote(stored, vocab, 0.85, true), 'Lifted: first kiss, peak of the burst');
  assert.equal(liftNote(stored, vocab, 0.85, false), '');
  assert.equal(liftNote(JSON.stringify({ readings: [{ tag: 'confetti', p: 0.7 }] }), vocab, 0.85, true), '');
  // A tag from another preset lifts nothing.
  assert.equal(liftNote(JSON.stringify({ readings: [{ tag: 'tyre_smoke', p: 0.99 }] }), vocab, 0.85, true), '');
  // The strongest tag is the one named.
  const two = JSON.stringify({ readings: [{ tag: 'confetti', p: 0.95 }, { tag: 'first_kiss', p: 0.95 }] });
  assert.equal(liftNote(two, vocab, 0.85, true), 'Lifted: first kiss');
});

test('an unverified vision-model reading says so', () => {
  assert.equal(describe({ tag: 'x', label: 'Flames', p: 0.9, calibrated: false }), 'Flames 90% (unverified)');
  assert.equal(describe({ tag: 'x', label: 'Flames', p: 0.96, calibrated: true }), 'Flames 96%');
  assert.equal(labelOf('wheel_lift'), 'Wheel lift');
});
