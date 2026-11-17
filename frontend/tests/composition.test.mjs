import test from 'node:test';
import assert from 'node:assert/strict';
import { compositionLabels, panComparison, relativeCleaner, stackClutter } from '../src/lib/composition.mjs';

const texts = (frame) => compositionLabels(frame).map((l) => `${l.text} (${l.tone})`);
const ids = (frame) => compositionLabels(frame).map((l) => l.id);

test('a well-framed frame earns every good label, in a stable order', () => {
  const frame = { id: 1, comp_placement: 0.82, comp_mode: 'thirds', comp_balance_x: 0.05, comp_balance_y: -0.02, comp_clutter: 0.2, comp_symmetry: 0.8, comp_lines: 2, comp_subject_share: 0.31 };
  assert.deepEqual(compositionLabels(frame), [
    { id: 'placement', text: 'Thirds', tone: 'good' },
    { id: 'balance', text: 'Balanced', tone: 'good' },
    { id: 'clutter', text: 'Clean background', tone: 'good' },
    { id: 'symmetry', text: 'Symmetric', tone: 'good' },
    { id: 'lines', text: '2 leading lines', tone: 'good' },
  ]);
  assert.deepEqual(ids(frame), ['placement', 'balance', 'clutter', 'symmetry', 'lines']);
});

test('a busy pan reads as off the grid, heavy on both axes and cluttered', () => {
  const pan = { id: 2, comp_placement: 0.31, comp_mode: 'phi', comp_balance_x: -0.4, comp_balance_y: 0.5, comp_clutter: 0.9, comp_symmetry: 0.2, comp_lines: 5, comp_subject_share: 0.12 };
  assert.deepEqual(compositionLabels(pan), [
    { id: 'placement', text: 'Off the grid', tone: 'warn' },
    { id: 'balance-x', text: 'Heavy left', tone: 'ok' },
    { id: 'balance-y', text: 'Bottom heavy', tone: 'ok' },
    { id: 'clutter', text: 'Busy background', tone: 'warn' },
    { id: 'lines', text: 'Cluttered lines', tone: 'warn' },
  ]);
});

test('placement thresholds and modes: 0.75 and 0.4 are inside their band', () => {
  assert.deepEqual(texts({ id: 1, comp_placement: 0.75, comp_mode: 'centre' }), ['Centre (good)']);
  assert.deepEqual(texts({ id: 1, comp_placement: 0.74, comp_mode: 'centre' }), ['Centre (ok)']);
  assert.deepEqual(texts({ id: 1, comp_placement: 0.4, comp_mode: 'phi' }), ['Golden ratio (ok)']);
  assert.deepEqual(texts({ id: 1, comp_placement: 0.39, comp_mode: 'phi' }), ['Off the grid (warn)']);
  assert.deepEqual(texts({ id: 1, comp_placement: 0.9 }), ['Well placed (good)']);
  assert.deepEqual(texts({ id: 1, comp_placement: 0.5 }), ['Framing average (ok)']);
  assert.deepEqual(texts({ id: 1, comp_mode: 'thirds' }), []);
});

test('balance: both axes inside 0.15 is balanced, a leaning axis names its side', () => {
  assert.deepEqual(ids({ id: 1, comp_balance_x: 0.15, comp_balance_y: -0.15 }), ['balance']);
  assert.deepEqual(texts({ id: 1, comp_balance_x: 0.16 }), ['Heavy right (ok)']);
  assert.deepEqual(texts({ id: 1, comp_balance_x: -0.2, comp_balance_y: 0.3 }), ['Heavy left (ok)', 'Bottom heavy (ok)']);
  assert.deepEqual(texts({ id: 1, comp_balance_y: -0.9 }), ['Top heavy (ok)']);
  // A single measured axis inside the dead zone cannot claim the frame is balanced.
  assert.deepEqual(texts({ id: 1, comp_balance_x: 0.1 }), []);
});

test('clutter: 0.55 is clean, 0.85 is busy, the crowded middle earns no chip', () => {
  assert.deepEqual(texts({ id: 1, comp_clutter: 0.55 }), ['Clean background (good)']);
  assert.deepEqual(texts({ id: 1, comp_clutter: 0.56 }), []);
  assert.deepEqual(texts({ id: 1, comp_clutter: 0.84 }), []);
  assert.deepEqual(texts({ id: 1, comp_clutter: 0.85 }), ['Busy background (warn)']);
});

test('symmetry needs 0.7; leading lines count one to three and warn above three', () => {
  assert.deepEqual(texts({ id: 1, comp_symmetry: 0.69 }), []);
  assert.deepEqual(texts({ id: 1, comp_symmetry: 0.7 }), ['Symmetric (good)']);
  assert.deepEqual(texts({ id: 1, comp_lines: 0 }), []);
  assert.deepEqual(texts({ id: 1, comp_lines: 1 }), ['1 leading line (good)']);
  assert.deepEqual(texts({ id: 1, comp_lines: 2 }), ['2 leading lines (good)']);
  assert.deepEqual(texts({ id: 1, comp_lines: 3 }), ['3 leading lines (good)']);
  assert.deepEqual(texts({ id: 1, comp_lines: 4 }), ['Cluttered lines (warn)']);
  assert.deepEqual(texts({ id: 1, comp_lines: 7 }), ['Cluttered lines (warn)']);
});

test('nothing measured means no labels at all, and nulls are the same as absent', () => {
  assert.deepEqual(compositionLabels({ id: 1 }), []);
  assert.deepEqual(compositionLabels({ id: 1, comp_placement: null, comp_mode: null, comp_balance_x: null, comp_balance_y: null, comp_symmetry: null, comp_clutter: null, comp_lines: null, comp_subject_share: null }), []);
  assert.deepEqual(compositionLabels(null), []);
});

test('panComparison picks the lowest measured clutter, ignores unmeasured frames, and needs a value to name one', () => {
  const frames = [
    { id: 11, comp_clutter: 0.62 },
    { id: 12, comp_clutter: null },
    { id: 13, comp_clutter: 0.24 },
    { id: 14, comp_clutter: 0.41 },
  ];
  assert.deepEqual(panComparison(frames), { id: 13, clutter: 0.24 });
  assert.deepEqual(panComparison([{ id: 1, comp_clutter: null }, { id: 2 }]), null);
  assert.deepEqual(panComparison([]), null);
  // Ties keep the earlier frame, so the marker does not jump between identical pans.
  assert.deepEqual(panComparison([{ id: 1, comp_clutter: 0.3 }, { id: 2, comp_clutter: 0.3 }]), { id: 1, clutter: 0.3 });
});

test('a stack is judged against its own median: on a crowded track only the calm pan is marked', () => {
  const stack = [
    { id: 1, comp_clutter: 0.9 },
    { id: 2, comp_clutter: 0.92 },
    { id: 3, comp_clutter: 0.6 },
  ];
  const { median, cleanestIds } = stackClutter(stack);
  assert.equal(median, 0.9);
  assert.deepEqual(cleanestIds, [3]);
  // 0.9 is the busy pan of the pair: relatively busier than the median, so no chip at all.
  assert.equal(relativeCleaner(median, 0.9), false);
  assert.equal(relativeCleaner(median, 0.6), true);
  assert.equal(relativeCleaner(null, 0.1), false);
  assert.equal(relativeCleaner(0.9, null), false);
});

test('fewer than three measured frames has a median but no cleanest marker', () => {
  assert.deepEqual(stackClutter([{ id: 1, comp_clutter: 0.6 }, { id: 2, comp_clutter: 0.9 }]), { median: 0.75, cleanestIds: [] });
  assert.deepEqual(stackClutter([{ id: 1, comp_clutter: 0.6 }]), { median: 0.6, cleanestIds: [] });
  assert.deepEqual(stackClutter([]), { median: null, cleanestIds: [] });
  assert.deepEqual(stackClutter([{ id: 1, comp_clutter: null }, { id: 2 }]), { median: null, cleanestIds: [] });
  // With two frames nothing is called cleanest, but the calm one still reads as relatively cleaner.
  const two = stackClutter([{ id: 1, comp_clutter: 0.6 }, { id: 2, comp_clutter: 0.9 }]);
  assert.deepEqual(two.cleanestIds, []);
  assert.equal(relativeCleaner(two.median, 0.6), true);
  assert.equal(relativeCleaner(two.median, 0.9), false);
});

test('the cleanest band needs a real gap: a tenth of the median and at least 0.05 below it', () => {
  // A tenth of 0.30 is 0.03, so 0.26 clears it but is only 0.04 below the median: not cleanest.
  assert.deepEqual(stackClutter([{ id: 1, comp_clutter: 0.26 }, { id: 2, comp_clutter: 0.3 }, { id: 3, comp_clutter: 0.34 }]).cleanestIds, []);
  // Four values average the middle pair; both low frames are more than a tenth below 0.5.
  assert.deepEqual(stackClutter([{ id: 1, comp_clutter: 0.2 }, { id: 2, comp_clutter: 0.4 }, { id: 3, comp_clutter: 0.6 }, { id: 4, comp_clutter: 0.8 }]), { median: 0.5, cleanestIds: [1, 2] });
});
