import test from 'node:test';
import assert from 'node:assert/strict';
import { edgeFrameId, nextStackIndex, stepInStack } from '../src/lib/navigation.mjs';

const stack = (key, ids) => ({ key, frames: ids.map((id) => ({ id })) });

// Burst 1: 1..3, an empty stack in the middle, burst 2: 7..8, burst 3: 9.
const stacks = [stack('b1', [1, 2, 3]), stack('b-empty', []), stack('b7', [7, 8]), stack('b9', [9])];
const cross = { crossStacks: true };

test('the next stack skips empty stacks and stops at the ends without wrapping', () => {
  assert.equal(nextStackIndex(stacks, 0, 1), 2);
  assert.equal(nextStackIndex(stacks, 2, 1), 3);
  assert.equal(nextStackIndex(stacks, 3, 1), null, 'the last stack has no next');
  assert.equal(nextStackIndex(stacks, 3, -1), 2);
  assert.equal(nextStackIndex(stacks, 2, -1), 0);
  assert.equal(nextStackIndex(stacks, 0, -1), null, 'the first stack has no previous');
  assert.equal(nextStackIndex(stacks, 1, 1), 2, 'a start inside an empty stack still finds the next');
  assert.equal(nextStackIndex([], 0, 1), null);
});

test('forward from a stack\u2019s last frame moves to the next stack\u2019s first frame', () => {
  const step = stepInStack(stacks, 0, 3, 'forward', cross);
  assert.deepEqual(step, { stackIndex: 2 });
  assert.equal(edgeFrameId(stacks[step.stackIndex], 'forward'), 7);
});

test('back from a stack\u2019s first frame moves to the previous stack\u2019s last frame', () => {
  const step = stepInStack(stacks, 2, 7, 'back', cross);
  assert.deepEqual(step, { stackIndex: 0 });
  assert.equal(edgeFrameId(stacks[step.stackIndex], 'back'), 3);
});

test('inside a stack the arrow keeps shooting order and the stack open', () => {
  assert.deepEqual(stepInStack(stacks, 0, 1, 'forward', cross), { id: 2, expanded: true });
  assert.deepEqual(stepInStack(stacks, 0, 3, 'back', cross), { id: 2, expanded: true });
});

test('with crossStacks off, both edges are the end of the move', () => {
  assert.equal(stepInStack(stacks, 0, 3, 'forward', { crossStacks: false }), null);
  assert.equal(stepInStack(stacks, 2, 7, 'back', { crossStacks: false }), null);
  assert.equal(stepInStack(stacks, 0, 3, 'forward'), null, 'off is the default');
});

test('a frame outside the stack and an empty stack move nowhere', () => {
  assert.equal(stepInStack(stacks, 1, 7, 'forward', cross), null);
  assert.equal(stepInStack(stacks, 0, 99, 'forward', cross), null);
  assert.equal(stepInStack([], 0, 1, 'forward', cross), null);
});
