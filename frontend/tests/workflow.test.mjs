import test from 'node:test';
import assert from 'node:assert/strict';
import {
  DEFAULT_WORKFLOW, WORKFLOW_OPTIONS, identifyActionLabel, pipelineSteps,
  workflowIncludesCull, workflowIncludesIdentify, workflowLabel, workflowModeOf,
} from '../src/lib/workflow.mjs';

const done = { scanned: true, identified: true, identifying: false, scanning: false };
const scanning = { scanned: false, identified: false, identifying: false, scanning: true };
const keys = (mode, state) => pipelineSteps(mode, state).map((step) => step.key);

test('the default mode is both, and an unknown value falls back to it', () => {
  assert.equal(DEFAULT_WORKFLOW, 'both');
  assert.equal(workflowModeOf('both'), 'both');
  assert.equal(workflowModeOf('cull'), 'cull');
  assert.equal(workflowModeOf('identify'), 'identify');
  assert.equal(workflowModeOf(undefined), 'both');
  assert.equal(workflowModeOf(null), 'both');
  assert.equal(workflowModeOf(''), 'both');
  assert.equal(workflowModeOf('xmp'), 'both');
  assert.equal(workflowModeOf(42), 'both');
});

test('the picker words are the same three in one order', () => {
  assert.deepEqual(WORKFLOW_OPTIONS.map((option) => option.id), ['both', 'cull', 'identify']);
  assert.deepEqual(WORKFLOW_OPTIONS.map((option) => option.label), ['Identify + cull', 'Cull only', 'Identify only']);
  assert.equal(workflowLabel('cull'), 'Cull only');
  assert.equal(workflowLabel('nonsense'), 'Identify + cull');
});

test('both runs Scanned, Culled and Identified', () => {
  assert.deepEqual(keys('both', done), ['scan', 'cull', 'identify']);
  assert.deepEqual(pipelineSteps('both', done), [
    { key: 'scan', label: 'Scanned', done: true, active: false },
    { key: 'cull', label: 'Culled', done: true, active: false },
    { key: 'identify', label: 'Identified', done: true, active: false },
  ]);
});

test('cull only has no Identified step, identify only has no Culled step', () => {
  assert.deepEqual(keys('cull', done), ['scan', 'cull']);
  assert.deepEqual(keys('identify', done), ['scan', 'identify']);
  assert.equal(pipelineSteps('cull', done).some((step) => step.key === 'identify'), false);
  assert.equal(pipelineSteps('identify', done).some((step) => step.key === 'cull'), false);
});

test('the steps follow real data, not job.status: a running scan only lights Scanned', () => {
  assert.deepEqual(pipelineSteps('both', scanning), [
    { key: 'scan', label: 'Scanned', done: false, active: true },
    { key: 'cull', label: 'Culled', done: false, active: false },
    { key: 'identify', label: 'Identified', done: false, active: false },
  ]);
});

test('culling settles with the scan, identification ticks only once every subject is read', () => {
  const scanned = { scanned: true, identified: false, identifying: false, scanning: false };
  assert.deepEqual(pipelineSteps('both', scanned), [
    { key: 'scan', label: 'Scanned', done: true, active: false },
    { key: 'cull', label: 'Culled', done: true, active: false },
    { key: 'identify', label: 'Identified', done: false, active: false },
  ]);
  const identifying = { scanned: true, identified: false, identifying: true, scanning: false };
  assert.deepEqual(pipelineSteps('identify', identifying), [
    { key: 'scan', label: 'Scanned', done: true, active: false },
    { key: 'identify', label: 'Identified', done: false, active: true },
  ]);
});

test('the step keys carry no number, so filtering never renames one', () => {
  for (const mode of ['both', 'cull', 'identify']) {
    for (const step of pipelineSteps(mode, done)) assert.equal(/\d/.test(step.key), false);
  }
});

test('the identify action is named for the mode, and absent in cull-only', () => {
  assert.equal(identifyActionLabel('both', 12), 'Identify kept photos (12)');
  assert.equal(identifyActionLabel('identify', 12), 'Identify all photos (12)');
  assert.equal(identifyActionLabel('cull', 12), null);
  assert.equal(identifyActionLabel('nonsense', 3), 'Identify kept photos (3)', 'unknown reads as both');
  assert.equal(identifyActionLabel('identify', 1200), 'Identify all photos (1,200)');
});

test('each mode says which halves of the pipeline it runs', () => {
  assert.deepEqual([workflowIncludesCull('both'), workflowIncludesIdentify('both')], [true, true]);
  assert.deepEqual([workflowIncludesCull('cull'), workflowIncludesIdentify('cull')], [true, false]);
  assert.deepEqual([workflowIncludesCull('identify'), workflowIncludesIdentify('identify')], [false, true]);
});
