import test from 'node:test';
import assert from 'node:assert/strict';
import {
  DEFAULT_IDENTIFY_SCOPE, IDENTIFY_SCOPES, IDENTIFY_SCOPE_OPTIONS, everythingCount,
  identifyActionLabel, identifyScopeLabel, identifyScopeOf, keeperCount,
} from '../src/lib/identify.mjs';
import { keeperIds } from '../src/lib/stacks.mjs';
import { frameExcluded } from '../src/review.mjs';

/** The review payload's facts map, cut down to what the keeper set and the counts read (`buildFacts` is TypeScript). */
const factsOf = (frames, rows) => new Map(frames.map((f) => {
  const dets = rows[f.id] ?? [];
  return [f.id, {
    dets,
    pick: dets.some((d) => Boolean(d.burst_pick)) || Boolean(f.burst_pick),
    cull: frameExcluded(f, dets) ? 'Culled · excluded from ML' : null,
  }];
}));
const frame = (id, extra = {}) => ({ id, burst_key: null, burst_pick: null, rejected: 0, picked: 0, manual_stars: null, ...extra });
const det = (extra = {}) => ({
  id: Math.random(), image_id: 0, cls: 'car', region_type: 'vehicle', reviewed: 0, burst_pick: 0,
  bystander: 0, rejected: 0, cull_reason: null, stars: null, ...extra,
});
/** `rv.keepers`: the effective keeper of each burst, which is what Review hands the count. */
const keepersOf = (frames, facts) => keeperIds(frames, facts);

test('the default scope is keepers, and an unknown value falls back to it', () => {
  assert.equal(DEFAULT_IDENTIFY_SCOPE, 'keepers');
  assert.deepEqual(IDENTIFY_SCOPES, ['keepers', 'everything']);
  assert.equal(identifyScopeOf('keepers'), 'keepers');
  assert.equal(identifyScopeOf('everything'), 'everything');
  assert.equal(identifyScopeOf(undefined), 'keepers');
  assert.equal(identifyScopeOf(null), 'keepers');
  assert.equal(identifyScopeOf(''), 'keepers');
  assert.equal(identifyScopeOf('picked'), 'keepers');
  assert.equal(identifyScopeOf('Everything'), 'keepers', 'the stored word is lower case');
  assert.equal(identifyScopeOf(42), 'keepers');
});

test('the two scopes are the same two choices, with their hint', () => {
  assert.deepEqual(IDENTIFY_SCOPE_OPTIONS.map((option) => option.id), ['keepers', 'everything']);
  assert.deepEqual(IDENTIFY_SCOPE_OPTIONS.map((option) => option.label), ['Keepers (suggested)', 'Everything']);
  assert.equal(IDENTIFY_SCOPE_OPTIONS[0].hint, 'The keeper you picked, or the suggested keeper of each burst.');
  assert.equal(IDENTIFY_SCOPE_OPTIONS[1].hint, 'Every subject, including frames you have not kept.');
  assert.equal(identifyScopeLabel('everything'), 'Everything');
  assert.equal(identifyScopeLabel('nonsense'), 'Keepers (suggested)');
});

test('the action is named for the scope, never for the workflow mode', () => {
  assert.equal(identifyActionLabel('keepers', 12), 'Identify keepers (12)');
  assert.equal(identifyActionLabel('everything', 12), 'Identify everything (12)');
  assert.equal(identifyActionLabel(undefined, 3), 'Identify keepers (3)', 'an unknown value is the default');
  assert.equal(identifyActionLabel('everything', 1200), 'Identify everything (1,200)');
  assert.equal(identifyActionLabel('keepers', Number.NaN), 'Identify keepers (0)');
  assert.equal(identifyActionLabel('keepers', undefined), 'Identify keepers (0)');
});

test('the suggested keeper of a burst is counted, a hand star alone is not', () => {
  const frames = [frame(1), frame(2, { manual_stars: 4 }), frame(3)];
  const facts = factsOf(frames, {
    1: [det({ burst_pick: 1 })], // the engine suggests this subject
    2: [det({ stars: 4 })], // hand-starred only: not a keeper
    3: [det()], // plain
  });
  const keepers = keepersOf(frames, facts);

  assert.deepEqual([...keepers].sort(), [1]);
  assert.equal(keeperCount(frames, facts, keepers), 1, 'the starred frame is not the effective keeper');
});

test('a manual pick overrides the suggestion: the picked frame counts, the suggested one does not', () => {
  const frames = [frame(1, { burst_key: 1 }), frame(2, { burst_key: 1, picked: 1 })];
  const facts = factsOf(frames, {
    1: [det({ burst_pick: 1 })], // the suggestion the pick replaced
    2: [det(), det()], // the pick's frame carries two subjects
  });
  const keepers = keepersOf(frames, facts);

  assert.deepEqual([...keepers], [2], 'the pick wins over the engine suggestion');
  assert.equal(keeperCount(frames, facts, keepers), 2, 'every subject on the picked frame counts');
  assert.equal(keeperCount([frames[0]], facts, keepers), 0, 'the suggested frame it replaced is not read');
});

test('a frame-level pick keeps its whole frame; a subject-less frame has nothing to read', () => {
  const frames = [frame(1, { burst_key: 1, burst_pick: 1 }), frame(2, { burst_key: 2 })];
  const facts = factsOf(frames, { 1: [det(), det()], 2: [] });
  const keepers = keepersOf(frames, facts);

  assert.equal(keeperCount(frames, facts, keepers), 2, 'both subjects of the picked frame');
});

test('a reviewed or excluded subject on a keeper frame is not pending', () => {
  const frames = [frame(1), frame(2, { rejected: 1 })];
  const facts = factsOf(frames, {
    1: [
      det({ burst_pick: 1 }), // the one subject left to read
      det({ burst_pick: 1, reviewed: 1 }),
      det({ burst_pick: 1, bystander: 1 }),
      det({ burst_pick: 1, rejected: 1 }),
      det({ burst_pick: 1, cull_reason: 'blurred' }),
    ],
    2: [det({ burst_pick: 1 })], // a rejected frame is never a keeper
  });
  const keepers = keepersOf(frames, facts);

  assert.deepEqual([...keepers], [1]);
  assert.equal(keeperCount(frames, facts, keepers), 1);
});

test('a frame whose whole burst was soft has no keeper to read', () => {
  const frames = [frame(1, { burst_pick: 1 })];
  const facts = factsOf(frames, { 1: [det({ cull_reason: 'blurred' })] });
  const keepers = keepersOf(frames, facts);

  assert.equal(keeperCount(frames, facts, keepers), 0);
});

test('everything counts every unreviewed vehicle subject, culls and rejects included', () => {
  const frames = [frame(1), frame(2, { rejected: 1 })];
  const facts = factsOf(frames, {
    1: [
      det(), // plain
      det({ cull_reason: 'blurred' }), // an automatic cull the engine still reads
      det({ reviewed: 1 }), // already read
      det({ bystander: 1 }), // a bystander is skipped
      det({ region_type: 'person' }), // not a subject
    ],
    2: [
      det({ rejected: 1 }), // a manual reject is still read
      det({ region_type: null }), // no region reads as a vehicle, as the grid does
      det({ region_type: 'face' }),
    ],
  });
  assert.equal(everythingCount(frames, facts), 4);
});

test('the two scopes differ on a culled keeper: keepers skips it, everything reads it', () => {
  const frames = [frame(1), frame(2)];
  const facts = factsOf(frames, { 1: [det({ burst_pick: 1, cull_reason: 'blurred' })], 2: [det()] });
  const keepers = keepersOf(frames, facts);

  assert.equal(keeperCount(frames, facts, keepers), 0);
  assert.equal(everythingCount(frames, facts), 2);
});

test('everything ignores the keeper flags, so it is never smaller than the keeper set', () => {
  const frames = [frame(1, { burst_pick: 1 }), frame(2)];
  const facts = factsOf(frames, { 1: [det({ burst_pick: 1 }), det()], 2: [det()] });
  const keepers = keepersOf(frames, facts);

  assert.equal(keeperCount(frames, facts, keepers), 2);
  assert.equal(everythingCount(frames, facts), 3);
});

test('an explicit keeper set overrides the suggestion: every unreviewed subject on those frames counts', () => {
  const frames = [frame(1), frame(2, { picked: 1 })];
  const facts = factsOf(frames, { 1: [det({ burst_pick: 1 })], 2: [det(), det()] });
  assert.equal(keeperCount(frames, facts, new Set([1])), 1, 'the suggested frame only');
  assert.equal(keeperCount(frames, facts, new Set([2])), 2, 'a caller-supplied set reads all of frame 2');
});

test('an empty album counts zero in either scope', () => {
  assert.equal(keeperCount([], new Map(), new Set()), 0);
  assert.equal(everythingCount([], new Map()), 0);
});
