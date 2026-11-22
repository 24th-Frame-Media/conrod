import test from 'node:test';
import assert from 'node:assert/strict';
import { openJobPage } from '../src/lib/navigation.mjs';
import { scanView, stateFromJob } from '../src/lib/scanStatus.mjs';

const job = (id, status, extra = {}) => ({ id, status, total: 100, done: 40, label: `Album ${id}`, ...extra });
const snap = (id, state = 'running') => ({ job_id: id, state, stage: 'read', done: 40, total: 100 });
const hydrate = (status, at) => ({ status, at });

test('a card opens the worker screen while its scan is live, waiting, stopped or failed', () => {
  assert.equal(openJobPage(job(1, 'done'), 1), 'Scan', 'the running album opens its worker screen');
  assert.equal(openJobPage(job(1, 'queued'), 2), 'Scan');
  assert.equal(openJobPage(job(1, 'scanning'), null), 'Scan', 'a scan that lost its slot still opens where it stopped');
  assert.equal(openJobPage(job(1, 'stopped'), null), 'Scan');
  assert.equal(openJobPage(job(1, 'error'), null), 'Scan');
  assert.equal(openJobPage(job(1, 'done'), null), 'Review');
  assert.equal(openJobPage(job(1, 'done_errors'), null), 'Review', 'the retry lives in Review');
  assert.equal(openJobPage(job(1, 'indexed'), null), 'Review');
  assert.equal(openJobPage(undefined, null), 'Review');
});

test('a live event newer than the entry read is what the screen draws', () => {
  const view = scanView({ job: job(3, 'scanning'), live: { progress: snap(3), at: 200 }, hydrate: hydrate({ state: 'running', progress: snap(3) }, 100), activeJob: 3 });
  assert.equal(view.source, 'live');
  assert.equal(view.state, 'running');
  assert.equal(view.progress.stage, 'read');
});

test('the entry read beats a live event that arrived before it', () => {
  const view = scanView({ job: job(3, 'scanning'), live: { progress: snap(3), at: 100 }, hydrate: hydrate({ state: 'running', progress: snap(3) }, 200), activeJob: 3 });
  assert.equal(view.source, 'status');
  assert.equal(view.state, 'running');
  assert.ok(view.progress);
});

test('a live event for another album is never drawn', () => {
  const view = scanView({ job: job(4, 'stopped'), live: { progress: snap(4), at: 200 }, hydrate: null, activeJob: 9 });
  assert.equal(view.source, 'job');
  assert.equal(view.state, 'stopped');
  assert.equal(view.progress, null);
  assert.equal(view.errors, 0);
});

test('a queued album says where it is in line', () => {
  const view = scanView({ job: job(5, 'queued'), hydrate: hydrate({ state: 'queued', progress: null, queue_position: 2, errors: 0, error_samples: [], resume_stage: null }, 100), activeJob: 3 });
  assert.equal(view.state, 'queued');
  assert.equal(view.queuePosition, 2);
  assert.equal(view.progress, null);
  assert.equal(view.terminal, false);
});

test('a running snapshot the engine no longer holds reads as stopped', () => {
  const view = scanView({ job: job(6, 'scanning'), live: { progress: snap(6), at: 50 }, hydrate: hydrate({ state: 'running', progress: snap(6) }, 100), activeJob: null });
  assert.equal(view.state, 'stopped', 'a crash or a finished run offers the resume, not a frozen bar');
});

test('a finished album is summarised from its record, with its unreadable frames', () => {
  const finished = job(7, 'done_errors', { errors: 2, error_samples: ['a.cr3: unsupported RAW'] });
  const view = scanView({ job: finished, hydrate: hydrate({ state: 'done_errors', progress: null, queue_position: null, errors: 2, error_samples: finished.error_samples, resume_stage: 'scan' }, 100), activeJob: null });
  assert.equal(view.state, 'done_errors');
  assert.equal(view.progress, null);
  assert.equal(view.errors, 2);
  assert.deepEqual(view.errorSamples, ['a.cr3: unsupported RAW']);
  assert.equal(view.resumeStage, 'scan');
  assert.equal(view.terminal, true);
});

test('the album record classifies its own state when nothing live answers', () => {
  assert.equal(stateFromJob(job(1, 'indexed')), 'stopped', 'an indexed album still has the cull to run');
  assert.equal(stateFromJob(job(1, 'done')), 'done');
  assert.equal(stateFromJob(job(1, 'scanning')), 'stopped');
  assert.equal(stateFromJob(null), 'idle');
});
