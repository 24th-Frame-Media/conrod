import test from 'node:test';
import assert from 'node:assert/strict';
import { applyDecision, lowStarBadge, reviewedStacks, stackDecision } from '../src/lib/decisions.mjs';

const f = (id, extra = {}) => ({ id, picked: 0, rejected: 0, ...extra });
const isUndecided = (frame) => !frame.picked && !frame.rejected;

test('keep clears a reject, cull clears a picked keeper, clear resets both', () => {
  assert.deepEqual(applyDecision(f(1, { rejected: 1 }), 'keep'), { picked: true, rejected: false });
  assert.deepEqual(applyDecision(f(1, { picked: 1 }), 'cull'), { rejected: true, picked: false });
  assert.deepEqual(applyDecision(f(1, { picked: 1, rejected: 1 }), 'clear'), { picked: false, rejected: false });
  assert.deepEqual(applyDecision(null, 'keep'), { picked: true, rejected: false }, 'the frame is only context');
});

test('B keeps exactly one frame and culls every other frame of the stack', () => {
  const changes = stackDecision([f(1), f(2), f(3), f(4)], 2);
  assert.equal(changes.length, 4);
  assert.deepEqual(changes.filter((c) => c.values.picked).map((c) => c.id), [2]);
  assert.deepEqual(changes.find((c) => c.id === 2).values, { picked: true, rejected: false });
  assert.deepEqual(changes.filter((c) => c.values.rejected).map((c) => c.id), [1, 3, 4]);
});

test('a stack decision includes frames already culled or kept, with idempotent values', () => {
  const changes = stackDecision([f(1, { rejected: 1 }), f(2, { picked: 1 }), f(3)], 3);
  assert.deepEqual(changes.map((c) => c.id), [1, 2, 3], 'nothing is skipped');
  assert.deepEqual(changes[0].values, { rejected: true, picked: false });
  assert.deepEqual(changes[1].values, { rejected: true, picked: false });
  assert.deepEqual(changes[2].values, { picked: true, rejected: false });
});

test('Shift+B culls every frame of the stack', () => {
  const changes = stackDecision([f(1), f(2, { rejected: 1 }), f(3, { picked: 1 })], 'cull');
  assert.deepEqual(changes, [
    { id: 1, values: { rejected: true, picked: false } },
    { id: 2, values: { rejected: true, picked: false } },
    { id: 3, values: { rejected: true, picked: false } },
  ]);
});

test('reviewed stacks count a mixed stack as open and a lone decided frame as done', () => {
  const stacks = [
    { frames: [f(1, { picked: 1 }), f(2, { rejected: 1 })] },
    { frames: [f(3), f(4, { rejected: 1 })] },
    { frames: [f(5, { rejected: 1 })] },
    { frames: [f(6)] },
    { frames: [] },
  ];
  assert.deepEqual(reviewedStacks(stacks, isUndecided), { reviewed: 3, total: 5 });
  assert.deepEqual(reviewedStacks([], isUndecided), { reviewed: 0, total: 0 });
});

test('the low-star badge appears below four stars, including a lone frame, and not at four', () => {
  assert.equal(lowStarBadge(2), 'No 4★+');
  assert.equal(lowStarBadge(3), 'No 4★+');
  assert.equal(lowStarBadge(0), 'No 4★+');
  assert.equal(lowStarBadge(null), 'No 4★+');
  assert.equal(lowStarBadge(4), null);
  assert.equal(lowStarBadge(5), null);
});
