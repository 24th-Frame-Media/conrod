import test from 'node:test';
import assert from 'node:assert/strict';
import {
  makeClock, orderWins, applySnapshot, SOURCE,
} from '../src/lib/snapshotOrder.mjs';
import {
  buildRunSnapshot, activeWorkers, workerLifecycle, STALL_MS,
  runStateWord, toneFor, jobChip, dropdownRows, scanSeedFor,
  stageFailureReason, stageFailed, failedStages, RUN_WORDS,
} from '../src/lib/runSnapshot.mjs';
import { SEEDS, activeWorker, doneWorker, failedWorker, pausedWorker, stalledWorker } from '../src/lib/mockSeeds.mjs';

// ---- Ordering guard (§6.1, audit P2-1 / P2-6) ----

test('makeClock is monotonic even when the wall clock steps backwards', () => {
  const c = makeClock();
  const a = c.now(1000);
  const b = c.now(500); // NTP correction / backward step
  const d = c.now(500);
  assert.equal(a, 1000);
  assert.ok(b > a, 'a backward wall step must not produce a smaller stamp');
  assert.equal(b, 1001);
  assert.ok(d > b, 'every call strictly increases');
  assert.equal(c.peek(), d);
});

test('a slow poll never overwrites a newer pushed live event', () => {
  // Poll requested (at=10) in flight; a pushed live event arrives (at=20);
  // then the poll resolves with its OLD request stamp.
  const poll = { at: 10, source: 'poll' };
  const pushed = { at: 20, source: 'live' };
  assert.equal(applySnapshot(null, pushed), pushed, 'first live event is applied');
  assert.equal(applySnapshot(pushed, poll), pushed, 'the late, stale poll is REJECTED');
  assert.equal(orderWins(pushed, poll), false);
  assert.equal(orderWins(pushed, { at: 21, source: 'live' }), true, 'a genuinely newer event still wins');
});

test('a live event stamped in the SAME millisecond as a poll wins (was a dropped `>`)', () => {
  const poll = { at: 100, source: 'poll' };
  const liveSameMs = { at: 100, source: 'live' };
  assert.equal(SOURCE.live > SOURCE.hydrate && SOURCE.hydrate > SOURCE.poll, true);
  assert.equal(orderWins(poll, liveSameMs), true, 'live beats poll on a tie');
  assert.equal(orderWins(liveSameMs, poll), false, 'a poll cannot beat a live event on a tie');
  assert.equal(orderWins({ at: 100, source: 'hydrate' }, { at: 100, source: 'live' }), true, 'live beats hydration on a tie');
});

test('same source on a tie: the last arrival wins', () => {
  const a = { at: 5, source: 'live', seq: 1 };
  const b = { at: 5, source: 'live', seq: 2 };
  assert.equal(orderWins(a, b), true);
  assert.equal(orderWins(b, a), false);
});

test('an older stamp never wins for any source combination', () => {
  assert.equal(orderWins({ at: 50, source: 'poll' }, { at: 49, source: 'live' }), false);
  assert.equal(applySnapshot({ at: 50, source: 'hydrate' }, { at: 1, source: 'live' }).at, 50);
});

// ---- Worker lifecycle (§6.4) ----

test('a done or failed worker LEAVES the grid; active/paused remain', () => {
  const workers = [activeWorker(), pausedWorker(), stalledWorker(), doneWorker(), failedWorker()];
  const grid = activeWorkers(workers, { now: 60000, stallMs: STALL_MS, cap: 100 });
  const lanes = grid.lanes;
  assert.equal(lanes.some((w) => w.state === 'done' || w.state === 'failed'), false, 'terminal workers are not drawn');
  assert.equal(lanes.length, 3, 'active, paused and stalled remain');
  assert.equal(lanes.some((w) => w.state === 'paused'), true);
  assert.equal(lanes.some((w) => w.state === 'stalled'), true);
});

test('a worker past the last-report window is honestly stalled, never busy', () => {
  const w = workerLifecycle(stalledWorker(), 60000, STALL_MS);
  assert.equal(w.lifecycle, 'stalled');
  assert.equal(w.stalled, true);
  assert.equal(w.step, 'detect', 'keeps its last step');
  const grid = activeWorkers([w], { now: 60000, stallMs: STALL_MS });
  assert.equal(grid.lanes[0].state, 'stalled');
  assert.equal(grid.stalled, 1);
});

test('a recent worker is working, a paused worker is paused (never stalled)', () => {
  assert.equal(workerLifecycle(activeWorker(), 60000, STALL_MS).lifecycle, 'working');
  const paused = workerLifecycle(pausedWorker(), 60000, STALL_MS);
  assert.equal(paused.lifecycle, 'paused');
  assert.equal(paused.stalled, false, 'a parked worker is not stalled');
  // Slow is not dead: a worker inside the window is never flagged stalled.
  const within = workerLifecycle({ ...activeWorker(), last_report_at: 60000 - STALL_MS + 1 }, 60000, STALL_MS);
  assert.equal(within.lifecycle, 'working');
});

test('legacy workers without last_report_at are never falsely stalled', () => {
  const legacy = { stage: 'read', worker_index: 0, state: 'busy', step: 'read', waiting_for: null };
  assert.equal(workerLifecycle(legacy, 1e12, STALL_MS).lifecycle, 'working');
});

test('run end / a terminal run clears every stale lane', () => {
  const grid = activeWorkers([activeWorker(), stalledWorker()], { now: 60000, runActive: true });
  assert.equal(grid.lanes.length, 2);
  const ended = activeWorkers([activeWorker(), stalledWorker()], { now: 60000, runActive: false });
  assert.deepEqual(ended.lanes, [], 'run end clears stale lanes');
});

test('a stage transition clears old-stage lanes: only the CURRENT snapshot is drawn', () => {
  const prev = [activeWorker(), { ...activeWorker(), lane: 5, stage: 'detect' }];
  const next = [{ ...activeWorker(), lane: 0, stage: 'vision' }]; // engine moved on
  const a = activeWorkers(prev, { now: 60000 });
  const b = activeWorkers(next, { now: 60000 });
  assert.equal(a.lanes.length, 2);
  assert.deepEqual(b.lanes.map((w) => w.stage), ['vision'], 'the old detect lane did not survive');
});

test('mock seed stalledWorker builds a grid with exactly one honest stalled lane', () => {
  const snap = buildRunSnapshot(SEEDS.stalledWorker());
  const grid = activeWorkers(snap.workers, { now: snap.now, stallMs: snap.stallMs, runActive: snap.runActive, cap: 100 });
  assert.equal(grid.stalled, 1);
  assert.equal(grid.lanes.some((w) => w.state === 'done' || w.state === 'failed'), false);
});

// ---- Errors are never silent (§6.6) ----

test('a null-job error shows on the empty Scan page but NEVER on another album', () => {
  const input = SEEDS.nullJobError();
  const seed = input.seed;
  assert.equal(seed.jobId, null);
  assert.equal(scanSeedFor(seed, null), seed, 'shown where the import failed (no album)');
  assert.equal(scanSeedFor(seed, 8), null, 'the null-job error must not match album 8');
  assert.equal(buildRunSnapshot(input).error, seed);
  assert.equal(buildRunSnapshot(SEEDS.nullJobErrorOnOtherAlbum()).error, null, 'album B never shows album A failure');
});

test('an error with a job id is bound only to that exact album', () => {
  const seed = { jobId: 5, title: 'Failed', message: 'why' };
  assert.equal(scanSeedFor(seed, 5), seed);
  assert.equal(scanSeedFor(seed, 6), null);
  assert.equal(scanSeedFor(seed, null), null);
  assert.equal(scanSeedFor(null, 5), null);
});

test('a failed stage yields a plain reason; a healthy stage does not', () => {
  const snap = buildRunSnapshot(SEEDS.failedStage());
  assert.equal(snap.failedStages.length, 1);
  assert.equal(snap.failedStages[0].id, 'vision');
  assert.equal(snap.failedStages[0].reason, 'The vision service stopped answering.');
  assert.equal(stageFailureReason({ id: 'read' }), '');
  assert.equal(stageFailed({ state: 'failed', reason: 'x' }), true);
  assert.equal(stageFailed({ state: 'running' }), false);
  assert.equal(failedStages([]).length, 0);
  assert.match(stageFailureReason({ state: 'failed' }), /stopped with an error/);
});

// ---- One status vocabulary + dropdown live update (§6.7) ----

test('one canonical status vocabulary for every chip, badge and node', () => {
  assert.equal(runStateWord('running'), 'Running');
  assert.equal(runStateWord('paused'), 'Paused');
  assert.equal(runStateWord('queued'), 'Queued');
  assert.equal(runStateWord('stopped'), 'Stopped');
  assert.equal(runStateWord('error'), 'Failed');
  assert.equal(runStateWord('done'), 'Done');
  assert.equal(runStateWord('scanning'), 'Stopped', 'a stored scanning record with no active slot is honest');
  assert.equal(runStateWord('nonsense'), 'Idle');
  assert.equal(toneFor('running'), 'running');
  assert.equal(toneFor('error'), 'error');
  assert.equal(toneFor('done'), 'done');
  assert.equal(Object.keys(RUN_WORDS).length > 8, true);
});

test('the jobs dropdown lists active/queued/recent jobs with a live chip + progress', () => {
  const snap = buildRunSnapshot(SEEDS.dropdownLive());
  assert.equal(snap.jobs.length, 3);
  const active = snap.jobs.find((j) => j.id === 7);
  const queued = snap.jobs.find((j) => j.id === 8);
  const done = snap.jobs.find((j) => j.id === 9);
  assert.equal(active.chip.word, 'Running');
  assert.equal(active.chip.tone, 'running');
  assert.equal(active.chip.progress, 0.6, 'progress from the live event');
  assert.equal(queued.chip.word, 'Queued');
  assert.equal(done.chip.word, 'Done');
});

test('the dropdown UPDATES on a pushed change without opening anything', () => {
  const before = buildRunSnapshot(SEEDS.dropdownLive());
  const after = buildRunSnapshot(SEEDS.dropdownLiveAdvanced());
  const chip = (snap, id) => snap.jobs.find((j) => j.id === id).chip;
  assert.equal(chip(before, 7).word, 'Running');
  assert.equal(chip(before, 7).progress, 0.6);
  assert.equal(chip(before, 8).word, 'Queued');
  // A pushed status changed the active job (7 -> 8) and a live event moved progress.
  assert.equal(chip(after, 7).word, 'Stopped', 'the old active album no longer claims to run');
  assert.equal(chip(after, 8).word, 'Running', 'the new active album is live');
  assert.equal(chip(after, 8).progress, 0.8);
  assert.equal(after.activeJob, 8);
});

test('jobChip derives progress from the live snapshot and the badge follows the run state', () => {
  const job = { id: 1, label: 'A', status: 'scanning', total: 100, done: 20 };
  const run = jobChip(job, { activeJob: 1, liveByJob: { 1: { progress: { overall: { fraction: 0.42 } } } } });
  assert.equal(run.word, 'Running');
  assert.equal(run.progress, 0.42);
  const stored = jobChip(job, {});
  assert.equal(stored.word, 'Stopped');
  assert.equal(stored.progress, 0.2);
  const rows = dropdownRows([job], { activeJob: 1 });
  assert.equal(rows[0].chip.word, 'Running');
  assert.equal(dropdownRows(null).length, 0);
});

// ---- Single snapshot is stage-agnostic and hydrated-then-live ----

test('buildRunSnapshot carries whatever stages the engine sent (stage-agnostic for B1)', () => {
  const input = SEEDS.failedStage();
  // New sub-stage ids B1 will emit: the snapshot must carry them verbatim.
  input.liveByJob[7].progress.pipeline.stages = [
    { id: 'identify:ocr', label: 'OCR', workers_total: 1, workers_busy: 1, done: 5, total: 10 },
    { id: 'identify:decision', label: 'DECISION', workers_total: 1, workers_busy: 0, done: 0, total: 10 },
    { id: 'identify:vision', label: 'VISION', workers_total: 1, workers_busy: 1, done: 2, total: 10 },
  ];
  const snap = buildRunSnapshot(input);
  assert.deepEqual(snap.stages.map((s) => s.id), ['identify:ocr', 'identify:decision', 'identify:vision']);
  assert.equal(snap.counters.done, 40);
  assert.equal(snap.badge.word, 'Running');
  assert.equal(snap.runActive, true);
});

test('a hydrate-then-live snapshot keeps the live event when it is newer', () => {
  const input = SEEDS.stalledWorker();
  input.hydrate = { status: { state: 'queued', progress: null, paused: false, queue_position: 1, errors: 0, error_samples: [], resume_stage: null }, at: 100, jobId: 7 };
  const snap = buildRunSnapshot(input);
  assert.equal(snap.view.source, 'live');
  assert.equal(snap.view.state, 'running');
});

test('paused run is honest (badge Paused, no stalled flag on parked workers)', () => {
  const snap = buildRunSnapshot(SEEDS.pausedRun());
  assert.equal(snap.paused, true);
  const grid = activeWorkers(snap.workers, { now: snap.now, stallMs: snap.stallMs, paused: true, cap: 100 });
  assert.equal(grid.stalled, 0);
  assert.equal(grid.lanes.length, 1);
});

test('run end produces no active lanes (stale lanes cleared)', () => {
  const snap = buildRunSnapshot(SEEDS.runEnded());
  assert.equal(snap.runActive, false);
  assert.deepEqual(activeWorkers(snap.workers, { now: snap.now, runActive: snap.runActive }).lanes, []);
});
