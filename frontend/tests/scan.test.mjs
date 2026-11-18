import test from 'node:test';
import assert from 'node:assert/strict';
import {
  orderStages, stageStatus, readyQueue, bottleneck, overallFraction, overallEta, waitingText, workersOf, workerCounts, formatDuration, formatRate,
  pushEtaSample, etaStable, etaLabel, etaRemaining, percent, summaryLine, clampWorkers,
} from '../src/lib/scan.mjs';

const st = (id, o = {}) => ({ id, label: id, model: '', device: 'CPU', workers_total: 2, workers_busy: 0, queue_len: 0, done: 0, total: 100, rate_per_s: 0, eta_s: null, ...o });

test('stages are shown in flow order whatever order the engine sends', () => {
  assert.deepEqual(orderStages([st('pick'), st('read'), st('mystery'), st('faces'), st('detect')]).map((s) => s.id), ['read', 'detect', 'faces', 'pick', 'mystery']);
  assert.deepEqual(orderStages(null), []);
});

test('stage status: off, pending, active, done', () => {
  assert.equal(stageStatus(st('read')), 'pending');
  assert.equal(stageStatus(st('read', { workers_busy: 1 })), 'active');
  assert.equal(stageStatus(st('read', { done: 40 })), 'active');
  assert.equal(stageStatus(st('read', { done: 100 })), 'done');
  assert.equal(stageStatus(st('read', { done: 100, workers_busy: 1 })), 'active', 'still busy is not done');
  assert.equal(stageStatus(st('group', { total: 0 })), 'pending', 'an empty stage with workers has not started');
  assert.equal(stageStatus(st('faces', { total: 0, workers_total: 0 })), 'off', 'no workers and nothing to do: not part of this shoot');
  assert.equal(stageStatus(undefined), 'pending');
});

test('the backlog in front of a stage is what upstream finished and it has not taken', () => {
  const o = [st('read', { done: 80 }), st('detect', { done: 30 }), st('sharpness', { done: 30 })];
  assert.equal(readyQueue(o, 0), 0, 'the first stage is a source');
  assert.equal(readyQueue(o, 1), 50);
  assert.equal(readyQueue(o, 2), 0);
  const skip = [st('detect', { done: 60 }), st('faces', { total: 0, workers_total: 0 }), st('identify', { done: 10, total: 40 })];
  assert.equal(readyQueue(skip, 2), 30, 'an off stage is skipped for its upstream');
  assert.equal(readyQueue([st('a', { done: 100 }), st('b', { total: 20, done: 0 })], 1), 20, 'bounded by its own total');
  const wait = [st('identify', { total: 40, done: 10 }), st('group', { total: 100, done: 0 })];
  assert.equal(readyQueue(wait, 1), 0, 'grouping waits for every frame, so nothing queues yet');
  assert.equal(readyQueue([st('identify', { total: 40, done: 40 }), st('group', { total: 100, done: 0 })], 1), 100);
});

test('the bottleneck is the biggest backlog; a tie goes to the slower stage; small backlogs are just flow', () => {
  const big = { total: 1000 };
  const jam = [
    st('read', { ...big, done: 400 }), st('detect', { ...big, done: 120, workers_busy: 2, rate_per_s: 40 }),
    st('sharpness', { ...big, done: 118, workers_busy: 1, rate_per_s: 39 }), st('faces', { ...big, done: 118 }),
  ];
  assert.equal(bottleneck(jam), 'detect');
  assert.equal(bottleneck(jam.map((s) => ({ ...s, done: s.id === 'read' ? 100 : 98 }))), null, 'a backlog of two is not a jam');
  const tie = [st('read', { done: 100 }), st('a', { done: 0, rate_per_s: 50 }), st('b', { done: 0, rate_per_s: 12 })];
  assert.equal(bottleneck([st('read', { done: 100 }), st('detect', { done: 0, rate_per_s: 50 })]), 'detect');
  assert.equal(bottleneck(tie.slice(0, 2)), 'a');
  assert.equal(bottleneck([st('read', { done: 100 }), st('detect', { done: 50, rate_per_s: 40 }), st('sharpness', { done: 0, rate_per_s: 12 })]), 'sharpness', 'the slower of two equal backlogs');
  assert.equal(bottleneck([st('read', { done: 100 }), st('detect', { done: 50, rate_per_s: 40 }), st('sharpness', { done: 0, rate_per_s: 0 })]), 'detect', 'an unknown rate counts as fast');
  assert.equal(bottleneck([st('read', { done: 100 }), st('detect', { done: 0, workers_total: 0, total: 0 })]), null, 'a stage with no workers is not running');
  assert.equal(bottleneck(null), null);
  // at the very start nothing has finished upstream, so nothing is a bottleneck
  assert.equal(bottleneck([st('read'), st('detect'), st('sharpness')]), null);
});

test('with workers that carry a frame through every stage, the stage most of them are inside is the bottleneck', () => {
  const flat = [st('read', { done: 50 }), st('detect', { done: 50 }), st('sharpness', { done: 50 })];
  const lane = (stage, i, state = 'busy') => ({ stage, worker_index: i, state, waiting_for: null, file: 'a.CR3', step: 's', ms_in_step: 1 });
  assert.equal(bottleneck(flat, [lane('detect', 0), lane('detect', 1), lane('detect', 2, 'waiting'), lane('read', 3), lane('sharpness', 4)]), 'detect');
  assert.equal(bottleneck(flat, [lane('detect', 0), lane('read', 1)]), null, 'one worker is not a pile-up');
  assert.equal(bottleneck(flat, [lane('detect', 0), lane('detect', 1), lane('read', 2), lane('read', 3)]), null, 'a tie is not a bottleneck');
  assert.equal(bottleneck(flat, [lane('read', 0, 'idle'), lane('read', 1, 'idle')]), null, 'idle workers do not count');
});

test('overall progress and ETA come from every stage, not the one the header names', () => {
  const stages = [
    st('read', { done: 100 }), st('detect', { done: 40, workers_busy: 2, eta_s: 30 }),
    st('sharpness', { done: 20, workers_busy: 1, eta_s: 12 }), st('faces', { total: 0, workers_total: 0 }),
  ];
  assert.equal(overallFraction(stages), (1 + 0.4 + 0.2) / 3, 'an off stage does not drag the bar');
  assert.equal(overallFraction([]), 0);
  assert.equal(overallEta(stages), 30, 'the slowest running stage decides');
  assert.equal(overallEta([...stages, st('identify', { done: 5, workers_busy: 1, eta_s: null })]), null, 'a running stage with no ETA yet means no honest ETA');
  assert.equal(overallEta([st('read', { done: 100 })]), null, 'nothing running');
  assert.equal(overallEta(null), null);
});

test("the engine's weighted overall wins over the per-stage mean, and the fallback stays", () => {
  const stages = [st('read', { done: 100 }), st('detect', { done: 40, workers_busy: 2, eta_s: 30 })];
  const overall = { done: 12, total: 100, fraction: 0.12, rate_per_s: 3.4, eta_s: 42, eta_confident: false };
  assert.equal(overallFraction(stages, overall), 0.12, 'the weighted fraction, not the stage mean');
  assert.equal(overallEta(stages, overall), 42, 'the engine knows about stages that have not started');
  assert.equal(overallFraction(stages), (1 + 0.4) / 2, 'no overall: the old per-stage mean');
  assert.equal(overallEta(stages), 30, 'no overall: the old slowest-stage ETA');
  assert.equal(overallFraction(stages, { fraction: 1.7 }), 1, 'clamped to 0..1');
  assert.equal(overallFraction(stages, { fraction: -2 }), 0);
  assert.equal(overallEta(stages, { eta_s: null }), null, 'an overall without an ETA means unknown, not a guess');
});

test('worker lanes: reasons and counts', () => {
  assert.equal(waitingText({ state: 'idle' }), 'idle');
  assert.equal(waitingText({ state: 'busy' }), 'working');
  assert.equal(waitingText({ state: 'waiting', waiting_for: 'gpu' }), 'waiting for the GPU');
  assert.equal(waitingText({ state: 'waiting', waiting_for: 'ollama' }), 'waiting for Ollama');
  assert.equal(waitingText({ state: 'waiting', waiting_for: 'lock' }), 'waiting for a lock');
  assert.equal(waitingText({ state: 'waiting', waiting_for: 'paused' }), 'paused');
  assert.equal(waitingText({ state: 'waiting', waiting_for: null }), 'waiting');
  const workers = [
    { stage: 'detect', worker_index: 1, state: 'waiting', waiting_for: 'gpu' }, { stage: 'detect', worker_index: 0, state: 'busy' },
    { stage: 'read', worker_index: 0, state: 'idle' },
  ];
  assert.deepEqual(workersOf(workers, 'detect').map((w) => w.worker_index), [0, 1]);
  assert.deepEqual(workerCounts(st('detect', { workers_total: 2, workers_busy: 1 }), workers), { busy: 1, waiting: 1, idle: 0 });
  assert.deepEqual(workerCounts(st('faces', { workers_total: 3, workers_busy: 1 }), workers), { busy: 1, waiting: 0, idle: 2 }, 'no lanes yet: fall back to the stage totals');
});

test('durations and rates read plainly', () => {
  assert.equal(formatDuration(0.2), '1 s');
  assert.equal(formatDuration(42.4), '42 s');
  assert.equal(formatDuration(125), '2 min 05 s');
  assert.equal(formatDuration(4320), '1 h 12 min');
  assert.equal(formatDuration(null), '');
  assert.equal(formatDuration(-1), '');
  assert.equal(formatRate(42.3), '42/s');
  assert.equal(formatRate(3.44), '3.4/s');
  assert.equal(formatRate(0), '');
  assert.equal(formatRate(null), '');
});

test('the ETA says estimating until the projected finish settles', () => {
  let h = [];
  assert.equal(etaStable(h), false);
  assert.deepEqual(pushEtaSample(h, 0, null, 10, 100), [], 'no ETA, no history');
  assert.deepEqual(pushEtaSample([{ finish: 1, now: 1 }], 5, 30, 0, 100), [], 'no progress yet resets');
  // a steady estimate: finish at ~60 s while time passes
  for (let i = 0; i < 6; i += 1) h = pushEtaSample(h, i * 250, 60 - i * 0.25, 10 + i, 100);
  assert.equal(etaStable(h), true);
  // an estimate that keeps jumping
  let wild = [];
  for (let i = 0; i < 8; i += 1) wild = pushEtaSample(wild, i * 250, i % 2 ? 300 : 20, 10 + i, 100);
  assert.equal(etaStable(wild), false);
  // a machine getting slower mid-scan: the finish drifts later but stays within half the time left
  let drift = [];
  for (let i = 0; i < 10; i += 1) drift = pushEtaSample(drift, i * 1000, 240 + i * 6, 100 + i * 10, 300);
  assert.equal(etaStable(drift), true, 'a drifting estimate is still an estimate');
  // the smoothed readout is the median finish, not the newest sample
  assert.ok(etaRemaining(drift) > 200);
  assert.equal(etaRemaining([], 0), null);
  // history is bounded
  let long = [];
  for (let i = 0; i < 40; i += 1) long = pushEtaSample(long, i * 250, 60 - i * 0.25, 10 + i, 100);
  assert.equal(long.length, 12);
  assert.equal(etaLabel(60, false), 'estimating…');
  assert.equal(etaLabel(null, true), 'estimating…');
  assert.equal(etaLabel(125, true), 'about 2 min 05 s');
  assert.equal(etaLabel(0.4, true), 'almost done');
});

test('percent and the summary line', () => {
  assert.equal(percent(5, 0), 0);
  assert.equal(percent(150, 100), 100);
  assert.equal(percent(1, 3), 33);
  assert.equal(
    summaryLine(3061, { bursts: 412, keepers: 286, soft: 640, closed_eyes: 12 }),
    '3,061 frames · 412 bursts · 286 suggested keepers · 640 soft · 12 closed eyes',
  );
  assert.equal(summaryLine(1, { bursts: 1, keepers: 1, soft: 0, closed_eyes: 0 }), '1 frame · 1 burst · 1 suggested keeper · 0 soft');
  assert.equal(summaryLine(0, undefined), '0 frames · 0 bursts · 0 suggested keepers · 0 soft');
});

test('worker counts stay inside the pool limits', () => {
  const limit = { current: 4, min: 1, max: 8, default: 4, live: false };
  assert.equal(clampWorkers(12, limit), 8);
  assert.equal(clampWorkers(0, limit), 1);
  assert.equal(clampWorkers(3.6, limit), 4);
  assert.equal(clampWorkers('abc', limit), 4);
});
