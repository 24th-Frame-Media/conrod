import test from 'node:test';
import assert from 'node:assert/strict';
import {
  MOCK_STEPS, STARTUP_LIMIT_MS, STARTUP_MAX_VISIBLE, mockStartup, modelParts, newerStartup, normalizeStartup,
  sizeText, startupCount, startupFraction, startupHeadline, startupSettled, visibleSteps,
} from '../src/lib/startup.mjs';

const step = (id, o = {}) => ({ step: id, label: `Step ${id}`, detail: null, error: null, model: null, ...o });
const snap = (o = {}) => ({ ...step('b'), done: 1, total: 5, ready: false, completed: [step('a')], ...o });

test('the bar is determinate once the total is known, and clamped', () => {
  assert.equal(startupFraction(snap()), 0.2);
  assert.equal(startupFraction(snap({ done: 9, total: 5 })), 1);
  assert.equal(startupFraction(snap({ total: 0 })), null);
  assert.equal(startupFraction(null), null);
  assert.equal(startupCount(snap({ done: 2, total: 5 })), '2 / 5');
  assert.equal(startupCount(snap({ total: 0 })), '');
});

test('the headline is the running step, or Starting before the engine says anything', () => {
  assert.equal(startupHeadline(snap()), 'Step b');
  assert.equal(startupHeadline(null), 'Starting');
});

test('only the newest few finished steps show, oldest first, and an error turns one amber', () => {
  const completed = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map((id) => step(id));
  const lines = visibleSteps(snap({ completed }));
  assert.equal(lines.length, STARTUP_MAX_VISIBLE);
  assert.deepEqual(lines.map((l) => l.key), ['c', 'd', 'e', 'f', 'g']);
  assert.deepEqual(visibleSteps(snap({ completed }), 2).map((l) => l.key), ['f', 'g']);
  const mixed = visibleSteps(snap({ completed: [step('a', { detail: '3 albums' }), step('b', { error: 'Could not check: disk' })] }));
  assert.deepEqual(mixed.map((l) => [l.tone, l.detail]), [['ok', '3 albums'], ['warn', 'Could not check: disk']]);
  assert.deepEqual(visibleSteps(null), []);
});

test('a model is named by its plain name, file and size; no file means no model line', () => {
  assert.deepEqual(modelParts({ name: 'Car and person detector', file: 'yolo11s-960.onnx', bytes: 38_400_000 }),
    { name: 'Car and person detector', file: 'yolo11s-960.onnx', size: '38.4 MB' });
  assert.deepEqual(modelParts({ name: '', file: 'x.onnx' }), { name: '', file: 'x.onnx', size: '' });
  assert.equal(modelParts({ name: 'Detector' }), null);
  assert.equal(modelParts(null), null);
  assert.equal(sizeText(512), '512 B');
  assert.equal(sizeText(6_500_000_000), '6.5 GB');
  assert.equal(sizeText(120_000_000), '120 MB');
  assert.equal(sizeText(0), '');
  assert.equal(sizeText(undefined), '');
});

test('the engine payload is normalised, and junk is rejected', () => {
  const raw = {
    step: 'models', label: 'Checking models', detail: null, model: { name: 'Faces', file: 'faces.onnx', bytes: 5 }, error: null,
    done: 2, total: 5, ready: false, completed: [{ step: 'library', label: 'Opening your library', detail: '3 albums', model: null, error: null }, { nope: true }],
  };
  const s = normalizeStartup(raw);
  assert.equal(s.step, 'models');
  assert.equal(s.model.file, 'faces.onnx');
  assert.equal(s.completed.length, 1);
  assert.equal(s.completed[0].detail, '3 albums');
  assert.equal(normalizeStartup(null), null);
  assert.equal(normalizeStartup({ label: '  ' }), null);
  assert.equal(normalizeStartup('x'), null);
  assert.equal(normalizeStartup({ label: 'Ready', done: -3, total: 'x', completed: 'no' }).total, 0);
});

test('an older snapshot never replaces a newer one, whichever way they arrive', () => {
  const early = snap({ completed: [step('a')] });
  const later = snap({ completed: [step('a'), step('b')] });
  const done = snap({ ready: true, completed: [step('a'), step('b'), step('c')] });
  assert.equal(newerStartup(null, early), early);
  assert.equal(newerStartup(later, early), later);
  assert.equal(newerStartup(early, later), later);
  assert.equal(newerStartup(done, later), done);
  assert.equal(newerStartup(later, done), done);
  assert.equal(newerStartup(later, null), later);
});

test('the splash leaves when start-up is done, unreadable, or too slow, and never earlier', () => {
  assert.equal(startupSettled({ status: snap() }), false);
  assert.equal(startupSettled({ status: null }), false);
  assert.equal(startupSettled({ status: snap({ ready: true }) }), true);
  assert.equal(startupSettled({ status: null, unavailable: true }), true);
  assert.equal(startupSettled({ status: snap(), waitedMs: STARTUP_LIMIT_MS - 1 }), false);
  assert.equal(startupSettled({ status: snap(), waitedMs: STARTUP_LIMIT_MS }), true);
});

test('the mock plays the five steps over two seconds, then is ready', () => {
  const at = (ms) => mockStartup(ms);
  assert.equal(at(0).done, 0);
  assert.equal(at(0).label, MOCK_STEPS[0].label);
  assert.equal(at(0).total, MOCK_STEPS.length);
  assert.equal(at(500).completed.length, 1);
  // The held frame (1 s) is on the model check, with a model to show.
  const held = at(1000);
  assert.equal(held.step, 'models');
  assert.equal(held.model.file, 'yolo11s-960.onnx');
  assert.deepEqual(held.completed.map((s) => s.step), ['library', 'albums']);
  const end = at(2000);
  assert.equal(end.ready, true);
  assert.equal(end.done, end.total);
  assert.equal(end.completed.length, MOCK_STEPS.length);
  assert.equal(at(99999).ready, true);
  assert.equal(at(-5).done, 0);
  assert.ok(at(1999).completed.length < at(2000).completed.length);
});
