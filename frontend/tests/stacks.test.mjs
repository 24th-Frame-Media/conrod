import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildStacks, decisionOf, keeperIds, keepersOf, nearestVisible, nextUndecided, reviewOrder,
  scrollTopFor, softBursts, stackLayout, stepFlat, stepInStack, stepRow, stepStack, suggestionSizes, visibleRows,
} from '../src/lib/stacks.mjs';

const f = (id, burst, extra = {}) => ({ id, path: `D:\\IMG_${String(id).padStart(4, '0')}.CR3`, burst_key: burst, rejected: 0, picked: 0, manual_stars: null, rating: 0.5, ...extra });
const factsOf = (spec = {}) => new Map(Object.entries(spec).map(([id, v]) => [Number(id), v]));
const ids = (list) => list.map((x) => x.id);

// Burst 1: 1..4 (engine pick on 2), burst 2: 5..6, frame 7 has no burst.
const frames = [f(1, 1), f(2, 1), f(3, 1), f(4, 1), f(5, 2), f(6, 2), f(7, null)];
const facts = factsOf({ 2: { pick: true }, 5: { pick: true } });

test('keeper is the manual pick if any, else the engine pick, never a rejected frame', () => {
  const burst = frames.slice(0, 4);
  assert.deepEqual(ids(keepersOf(burst, facts)), [2]);
  const picked = burst.map((x) => (x.id === 3 ? { ...x, picked: 1 } : x));
  assert.deepEqual(ids(keepersOf(picked, facts)), [3], 'a manual pick replaces the engine pick');
  const rejectedPick = burst.map((x) => (x.id === 2 ? { ...x, rejected: 1 } : x));
  assert.deepEqual(ids(keepersOf(rejectedPick, facts)), [], 'rejecting the engine pick leaves no keeper');
  const two = burst.map((x) => (x.id === 1 || x.id === 4 ? { ...x, picked: 1 } : x));
  assert.deepEqual(ids(keepersOf(two, facts)), [1, 4], 'several manual picks are all keepers');
});

test('decisions: manual reject beats pick, pick beats auto-cull, otherwise undecided', () => {
  assert.equal(decisionOf(f(1, 1, { rejected: 1, picked: 1 }), undefined), 'rejected');
  assert.equal(decisionOf(f(1, 1, { picked: 1 }), { cull: 'soft' }), 'picked');
  assert.equal(decisionOf(f(1, 1), { cull: 'soft' }), 'rejected');
  assert.equal(decisionOf(f(1, 1), { pick: true }), 'undecided');
});

test('keeperIds works burst by burst over the whole album, so a filter cannot change it', () => {
  assert.deepEqual([...keeperIds(frames, facts)].sort(), [2, 5]);
  const marked = frames.map((x) => (x.id === 4 ? { ...x, picked: 1 } : x));
  assert.deepEqual([...keeperIds(marked, facts)].sort(), [4, 5]);
});

test('a burst with nothing above two stars has no keeper, and a fair frame or a hand star settles it', () => {
  const soft = [f(1, 9, { rating: 0.66 }), f(2, 9, { rating: 0.67 })];
  assert.deepEqual(softBursts(soft, factsOf()).get(9), { size: 2, best: 2 }, 'best of two stars: no keeper');
  assert.equal(softBursts([f(1, 9, { rating: 0.66 }), f(2, 9, { rating: 0.75 })], factsOf()).size, 0, 'one fair frame is enough to pick from');
  assert.equal(softBursts(soft, factsOf({ 1: { pick: true } })).size, 0, 'a keeper removes the note');
  assert.equal(softBursts([f(1, 9, { rating: 0.4, manual_stars: 3 }), f(2, 9, { rating: 0.4 })], factsOf()).size, 0, 'your own three stars count');
  assert.equal(softBursts([f(1, null, { rating: 0.4 })], factsOf()).size, 0, 'a lone frame is not a burst');
});

test('a suggestion carries the size of the burst it heads, and stops being a suggestion once you pick', () => {
  const burst = [f(1, 1), f(2, 1), f(3, 1)];
  const sizes = suggestionSizes(burst, factsOf({ 1: { pick: true } }));
  assert.equal(sizes.get(1), 3, 'best of three frames');
  assert.equal(sizes.size, 1);

  const picked = burst.map((x) => (x.id === 2 ? { ...x, picked: 1 } : x));
  assert.equal(suggestionSizes(picked, factsOf({ 1: { pick: true } })).size, 0, 'a manual pick settles the burst');

  const rejectedLeader = burst.map((x) => (x.id === 1 ? { ...x, rejected: 1 } : x));
  assert.equal(suggestionSizes(rejectedLeader, factsOf({ 1: { pick: true } })).size, 0, 'a rejected pick suggests nothing');

  const twoBursts = suggestionSizes(frames, facts);
  assert.equal(twoBursts.get(2), 4);
  assert.equal(twoBursts.get(5), 2);
  assert.equal(twoBursts.get(7), undefined, 'a lone frame is not a burst to choose within');
});

test('stacks are built from the filtered list: counts, order and lead follow the filter, the total stays', () => {
  const keepers = keeperIds(frames, facts);
  const all = buildStacks(frames, { facts, keepers });
  assert.deepEqual(all.map((s) => s.frames.length), [4, 2, 1]);
  assert.deepEqual(ids(all.map((s) => s.lead)), [2, 5, 7]);

  const visible = frames.filter((x) => x.id !== 2 && x.id !== 6);
  const filtered = buildStacks(visible, { facts, keepers, all: frames });
  assert.deepEqual(filtered.map((s) => s.frames.length), [3, 1, 1]);
  assert.deepEqual(filtered.map((s) => s.total), [4, 2, 1]);
  assert.deepEqual(ids(filtered[0].keepers), [], 'the hidden keeper is not among the visible frames');
  assert.notEqual(filtered[0].lead.id, 2, 'the lead is always a visible frame');
  assert.ok(visible.some((x) => x.id === filtered[0].lead.id));
});

test('stack keys do not change with the filter, so an open stack stays open', () => {
  const keepers = keeperIds(frames, facts);
  const keys = (list) => buildStacks(list, { facts, keepers, all: frames }).map((s) => s.key);
  assert.deepEqual(keys(frames), ['b1', 'b5', 'b7']);
  assert.deepEqual(keys(frames.filter((x) => x.id !== 1 && x.id !== 5)), ['b1', 'b5', 'b7']);
});

test('a stack lists its frames as shot and leads with the keeper; without a keeper it leads with the best frame that is not rejected', () => {
  const noKeeper = [f(1, 1, { rating: 0.4 }), f(2, 1, { rating: 0.9, rejected: 1 }), f(3, 1, { rating: 0.7 })];
  const [stack] = buildStacks(noKeeper, { facts: factsOf(), keepers: new Set() });
  assert.deepEqual(ids(stack.frames), [1, 2, 3]);
  assert.equal(stack.lead.id, 3);
  assert.deepEqual(ids(reviewOrder(stack)), [3, 1, 2]);
  const [handled] = buildStacks([f(1, 1, { manual_stars: 2 }), f(2, 1, { rating: 0.99 })], { facts: factsOf(), keepers: new Set() });
  assert.equal(handled.lead.id, 1, 'manual stars outrank the measured rating');
});

test('dupe stacks need two or more frames in the whole album, not just in the filtered list', () => {
  const list = [f(1, 1, { dupe_key: 9 }), f(2, 2, { dupe_key: 9 }), f(3, 3, { dupe_key: 10 })];
  const stacks = buildStacks(list.slice(0, 1), { facts: factsOf(), keepers: new Set(), all: list, dupes: true });
  assert.deepEqual(stacks.map((s) => s.frames.length), [1]);
  assert.equal(stacks[0].total, 2);
  assert.equal(buildStacks(list, { facts: factsOf(), keepers: new Set(), dupes: true }).length, 1);
});

test('left/right stay in the burst, up/down and Tab go between bursts and land on the lead, edges do not move', () => {
  const stacks = buildStacks(frames, { facts, keepers: keeperIds(frames, facts) });
  assert.equal(stepInStack(stacks, 2, 1), 3);
  assert.equal(stepInStack(stacks, 4, 1), 4);
  assert.equal(stepInStack(stacks, 1, -1), 1);
  assert.equal(stepInStack(stacks, 1, Number.POSITIVE_INFINITY), 4);
  assert.equal(stepInStack(stacks, 99, 1), null);
  assert.equal(stepStack(stacks, 3, 1), 5, 'next burst lands on its keeper');
  assert.equal(stepStack(stacks, 6, -1), 2, 'previous burst lands on its keeper');
  assert.equal(stepStack(stacks, 7, 1), 7, 'last burst: stay');
  assert.equal(stepStack(stacks, 1, -1), 1, 'first burst: stay');
  assert.equal(stepStack(stacks, null, 1), 2);
  assert.equal(stepStack(stacks, null, -1), 7);
  assert.equal(stepStack([], 1, 1), null);
});

test('navigation never steps into hidden frames', () => {
  const visible = frames.filter((x) => ![2, 3].includes(x.id));
  const stacks = buildStacks(visible, { facts, keepers: keeperIds(frames, facts), all: frames });
  const reachable = new Set(visible.map((x) => x.id));
  for (const start of reachable) for (const d of [-1, 1]) {
    assert.ok(reachable.has(stepInStack(stacks, start, d)));
    assert.ok(reachable.has(stepStack(stacks, start, d)));
  }
  assert.equal(stepInStack(stacks, 1, 1), 4);
});

test('flat view: left/right one frame, up/down one row, short last row reachable, no wrap', () => {
  const list = Array.from({ length: 10 }, (_, i) => f(i + 1, null));
  assert.equal(stepFlat(list, 3, 1), 4);
  assert.equal(stepFlat(list, 10, 1), 10);
  assert.equal(stepRow(list, 2, 1, 4), 6);
  assert.equal(stepRow(list, 7, 1, 4), 10, 'row 2 is short: down from the row above lands on the last frame');
  assert.equal(stepRow(list, 9, 1, 4), 9, 'already on the last row');
  assert.equal(stepRow(list, 6, -1, 4), 2);
  assert.equal(stepRow(list, 2, -1, 4), 2);
  assert.equal(stepRow(list, 99, 1, 4), 1);
});

test('next undecided skips decided frames, wraps for N and stops at the end for auto-advance', () => {
  const list = [f(1, 1), f(2, 1, { picked: 1 }), f(3, 1, { rejected: 1 }), f(4, 1), f(5, 2)];
  const undecided = (x) => decisionOf(x, undefined) === 'undecided';
  assert.equal(nextUndecided(list, 1, undecided), 4);
  assert.equal(nextUndecided(list, 5, undecided), 1, 'N wraps');
  assert.equal(nextUndecided(list, 5, undecided, { wrap: false }), null, 'auto-advance stays at the end');
  assert.equal(nextUndecided(list, 4, undecided, { dir: -1 }), 1);
  assert.equal(nextUndecided([f(1, 1, { picked: 1 })], 1, undecided), null);
  assert.equal(nextUndecided(list, 99, undecided), 1);
});

test('when a filter hides the selection, the nearest visible neighbour is chosen, forward first', () => {
  const prev = [1, 2, 3, 4, 5];
  assert.equal(nearestVisible(prev, new Set([1, 4, 5]), 2), 4);
  assert.equal(nearestVisible(prev, new Set([1, 2]), 4), 2);
  assert.equal(nearestVisible(prev, new Set(), 3), null);
  assert.equal(nearestVisible(prev, new Set([1]), 99), null);
});

test('virtual stack layout: expanded strip pushes later rows down and only touching rows mount', () => {
  const plain = stackLayout({ count: 100, cols: 4, rowH: 200, gap: 20, pad: 10 });
  assert.equal(plain.rows, 25);
  assert.equal(plain.rowTop(3), 10 + 3 * 220);
  assert.equal(plain.height, 10 + 24 * 220 + 200 + 10);
  const open = stackLayout({ count: 100, cols: 4, rowH: 200, gap: 20, pad: 10, expandedIndex: 5, expandedH: 240 });
  assert.equal(open.expandedRow, 1);
  assert.equal(open.expandedTop, open.rowTop(1) + 220);
  assert.equal(open.rowTop(2), plain.rowTop(2) + 260);
  assert.equal(open.height, plain.height + 260);
  const [first, last] = visibleRows(plain, 0, 600, 0);
  assert.equal(first, 0);
  assert.ok(last <= 3 && last >= 2);
  const [a, b] = visibleRows(plain, 2000, 600, 1);
  assert.ok(a >= 7 && b <= 14, `${a}..${b}`);
  assert.deepEqual(visibleRows(stackLayout({ count: 0, cols: 4, rowH: 200, gap: 20, pad: 10 }), 0, 600), [0, 0]);
});

test('scroll target only moves when the stack is out of view and never jumps to the top', () => {
  const layout = stackLayout({ count: 100, cols: 4, rowH: 200, gap: 20, pad: 10 });
  assert.equal(scrollTopFor(layout, 5, 0, 600, 10), null);
  assert.equal(scrollTopFor(layout, 40, 0, 600, 10), layout.rowTop(10) + 200 - 600 + 10);
  assert.equal(scrollTopFor(layout, 0, 3000, 600, 10), 0);
  assert.equal(scrollTopFor(layout, 20, 800, 600, 10), null);
  const open = stackLayout({ count: 100, cols: 4, rowH: 200, gap: 20, pad: 10, expandedIndex: 40, expandedH: 700 });
  assert.equal(scrollTopFor(open, 40, 0, 600, 10), open.rowTop(10) - 10, 'row + strip taller than the view: align the row to the top');
});
