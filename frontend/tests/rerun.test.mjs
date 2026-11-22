import test from 'node:test';
import assert from 'node:assert/strict';
import {
  RERUN_STAGES, anyRerunRunning, rerunDoneText, rerunHint, rerunKey, rerunLabel,
  rerunStageOf, rerunStartedText, runningRerun,
} from '../src/lib/rerun.mjs';

test('the five stages are named in pipeline order', () => {
  assert.deepEqual(RERUN_STAGES, ['detect', 'rate', 'identify', 'moments', 'pick']);
  assert.equal(rerunStageOf('moments'), 'moments');
  assert.equal(rerunStageOf('group'), null);
  assert.equal(rerunStageOf(undefined), null);
});

test('each stage has the plain label the photographer asked for', () => {
  assert.equal(rerunLabel('detect'), 'Find cars again');
  assert.equal(rerunLabel('rate'), 'Check sharpness again');
  assert.equal(rerunLabel('identify'), 'Read cars again');
  assert.equal(rerunLabel('moments'), 'Judge moments again');
  assert.equal(rerunLabel('pick'), 'Pick best of bursts again');
  assert.equal(rerunLabel('nonsense'), 'Run again');
});

test('a shoot with no vehicles reads subjects, not cars', () => {
  assert.equal(rerunLabel('detect', { vehicles: false }), 'Find subjects again');
  assert.equal(rerunLabel('identify', { vehicles: false }), 'Read subjects again');
  assert.equal(rerunLabel('rate', { vehicles: false }), 'Check sharpness again');
  assert.equal(rerunLabel('pick', { vehicles: false }), 'Pick best of bursts again');
});

test('every stage carries a hint that says what stays', () => {
  for (const stage of RERUN_STAGES) assert.ok(rerunHint(stage).length > 0, stage);
  assert.match(rerunHint('rate'), /hand star is never lowered/);
  assert.match(rerunHint('pick'), /hand pick always wins/);
  assert.match(rerunHint('detect'), /reads, stars and marks stay/);
  assert.equal(rerunHint('nonsense'), '');
});

test('the operation key matches the engine and reads back the running stage', () => {
  assert.equal(rerunKey('moments', 5), 'Rerunning moments:5');
  const ops = ['Identifying:5', 'Rerunning moments:5'];
  assert.equal(runningRerun(ops, 5), 'moments');
  assert.equal(runningRerun(ops, 6), null, 'another album is not this one');
  assert.equal(runningRerun(['Identifying:5'], 5), null, 'a first identify is not a rerun');
  assert.equal(runningRerun([], 5), null);
  assert.equal(runningRerun(undefined, 5), null);
  assert.equal(runningRerun(['Rerunning bogus:5'], 5), null, 'an unknown stage is ignored');
});

test('only one rerun at a time: any rerun anywhere blocks the next', () => {
  assert.equal(anyRerunRunning(['Rerunning detect:9']), true);
  assert.equal(anyRerunRunning(['Identifying:9', 'Writing metadata:2']), false);
  assert.equal(anyRerunRunning([]), false);
  assert.equal(anyRerunRunning(undefined), false);
});

test('the done toast carries the engine counts, and identify gets a short line', () => {
  assert.equal(rerunDoneText('moments', 'Judged 214 frames · 3 moments found · 12 keepers'), 'Judged 214 frames · 3 moments found · 12 keepers');
  assert.equal(rerunDoneText('pick', 'Picked 40 keepers from 214 frames across 60 passes'), 'Picked 40 keepers from 214 frames across 60 passes');
  assert.equal(rerunDoneText('identify', 'Picking keepers'), 'Read cars again — done.');
  assert.equal(rerunDoneText('rate', ''), 'Check sharpness again — done.');
  assert.equal(rerunDoneText('detect', null), 'Find cars again — done.');
});

test('the started toast names the stage and points at Activity', () => {
  assert.equal(rerunStartedText('moments'), 'Judge moments again started. Progress in Activity.');
});
