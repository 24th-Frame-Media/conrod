import assert from 'node:assert/strict';
import test from 'node:test';
import { healthStatus, installableModels, modelGroups, modelWord, workLabel } from '../src/lib/health.mjs';

const models = [
  { name: 'Detector', file: 'yolo.onnx', ready: true },
  { name: 'ExifTool', file: 'exiftool', ready: false },
  { name: 'Decision model', file: 'decider', ready: false, optional: true },
  { name: 'Vision model', file: 'vlm', ready: false, detail: 'Ollama unavailable' },
];

test('groups split Conrod models from opt-in models and vision services', () => {
  const groups = modelGroups(models);
  assert.deepEqual(groups.local.map((m) => m.name), ['Detector', 'ExifTool']);
  assert.deepEqual(groups.optional.map((m) => m.name), ['Decision model']);
  assert.deepEqual(groups.vision.map((m) => m.name), ['Vision model']);
});

test('Install missing never offers a vision service or an opt-in model', () => {
  assert.deepEqual(installableModels(models).map((m) => m.name), ['ExifTool']);
  assert.deepEqual(installableModels([models[3], models[2]]), []);
});

test('health badge says Setup needed for a missing required model, not for optional ones', () => {
  const setup = healthStatus({ models });
  assert.equal(setup.label, 'Setup needed');
  assert.equal(setup.tone, 'warn');
  const optionalOnly = healthStatus({ models: [models[0], models[2]] });
  assert.equal(optionalOnly.label, 'Ready');
  assert.equal(optionalOnly.tone, 'ok');
});

test('a configured vision service that is off is amber Ready · vision off', () => {
  const status = healthStatus({ models: [models[0], models[3]] });
  assert.equal(status.label, 'Ready · vision off');
  assert.equal(status.tone, 'warn');
  assert.match(status.detail, /Vision is unavailable/);
  assert.equal(modelWord(models[3]), 'Unavailable');
});

test('vision disabled in Settings is the same amber state, worded as off', () => {
  const status = healthStatus({ models: [models[0], { name: 'Vision model', file: 'vlm', ready: true, detail: 'Disabled in settings' }] });
  assert.equal(status.label, 'Ready · vision off');
  assert.equal(modelWord({ name: 'Vision model', file: 'vlm', ready: true, detail: 'Disabled in settings' }), 'Off in Settings');
});

test('running and paused tasks win over model setup; a failed task is Error', () => {
  const running = healthStatus({ tasks: [{ id: 1, label: 'Scanning Bathurst · album 3', state: 'running', detail: 'IMG_1.CR3' }], models });
  assert.equal(running.label, 'Scanning Bathurst');
  assert.equal(running.tone, 'busy');
  const paused = healthStatus({ tasks: [{ id: 1, label: 'Scanning Bathurst', state: 'paused' }], models });
  assert.equal(paused.label, 'Paused');
  assert.equal(paused.tone, 'warn');
  const failed = healthStatus({ tasks: [{ id: 2, label: 'Grouping', state: 'failed', error: 'disk full' }], models });
  assert.equal(failed.label, 'Error');
  assert.equal(failed.tone, 'error');
});

test('workLabel drops the album suffix and keeps the stage word', () => {
  assert.equal(workLabel('Scanning Bathurst 1000 · album 3'), 'Scanning Bathurst 1000');
  assert.equal(workLabel('Identifying · album 4'), 'Identifying');
  assert.equal(workLabel('Grouping Bathurst'), 'Grouping');
  // A double-encoded middle dot (\u00c2\u00b7) must never leave a stray U+00C2 in the badge.
  assert.equal(workLabel('Rerunning detect \u00c2\u00b7 album 5'), 'Rerunning detect');
});
