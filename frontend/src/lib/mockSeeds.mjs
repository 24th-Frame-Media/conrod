// Dev/QA seeds for every run state the truthfulness contract cares about
// (plan §6.8). Pure data — no engine, no timers — so `node --test` can build a
// whole run snapshot from each one without Rust. B5 reuses these for the full
// surface; B4 uses them to pin the plumbing.
//
// Wording is deliberately neutral: subject words are derived from the album's
// resolved settings via subjects.mjs, never hardcoded here.

const STALL = 45000;
const NOW = 60000;

export function makeWorker(over = {}) {
  return {
    stage: 'read', worker_index: 0, lane: 0, state: 'working', waiting_for: null,
    file: null, step: 'read', ms_in_step: 0, last_report_at: NOW - 1000, current: null,
    ...over,
  };
}

/** A worker actively on its current step (recent heartbeat). */
export const activeWorker = () => makeWorker({ lane: 0, step: 'detect', last_report_at: NOW - 1000 });
/** Finished: its result belongs in the recent feed, NOT the lane grid. */
export const doneWorker = () => makeWorker({ lane: 2, state: 'done', step: 'persist', last_report_at: NOW - 1000 });
/** Engine-confirmed dead: leaves the grid. */
export const failedWorker = () => makeWorker({ lane: 4, state: 'failed', step: 'vision', last_report_at: NOW - 1000 });
/** Parked by Pause: shown as paused, never busy or stalled. */
export const pausedWorker = () => makeWorker({ lane: 1, state: 'paused', step: 'read', last_report_at: NOW - 1000 });
/** Active but past the generous last-report window: shown honestly as stalled. */
export const stalledWorker = () => makeWorker({ lane: 3, state: 'working', step: 'detect', last_report_at: 1000 });

export const activeStage = (id = 'read', extra = {}) => ({ id, label: id[0].toUpperCase() + id.slice(1), model: '', device: 'CPU', workers_total: 2, workers_busy: 1, queue_len: 0, done: 40, total: 100, errors: 0, rate_per_s: 1.2, eta_s: 50, ...extra });
/** A stage that stopped with an error → failed node + plain reason. */
export const failedStage = () => activeStage('vision', { state: 'failed', done: 3, total: 10, errors: 1, reason: 'The vision service stopped answering.' });

/** An import that failed BEFORE an album existed (jobId null): the null-job case. */
export const nullJobError = () => ({ kind: 'folder_missing', title: 'Conrod lost the folder', message: 'The folder is not there any more. Reconnect the drive and press Resume.', detail: 'ENOENT', resumable: true, jobId: null, label: 'Import', raw: 'ENOENT', args: { root: '' } });

const baseJob = (id, status = 'done', extra = {}) => ({ id, label: `Album ${id}`, root: '', status, total: 100, done: 40, errors: 0, error_samples: [], created_at: 1700000000 + id, ...extra });

function status(activeJob = 7, over = {}) {
  return {
    activeJob, queue: [], log: [], operations: [],
    tasks: [{ id: 1, label: 'Scanning', detail: '', state: 'running', done: 40, total: 100, eta: null, error: null }],
    ...over,
  };
}

function scanProgress(jobId, over = {}) {
  return {
    job_id: jobId, state: 'running', stage: 'read', stage_index: 0, stage_count: 6,
    done: 40, total: 100, rate_per_s: 1.2, eta_s: 50,
    recent: [], found: { subjects: 0, soft: 0, closed_eyes: 0, numbers: 0, plates: 0, bursts: 0, keepers: 0 },
    pipeline: { stages: [activeStage('read')], workers: [] },
    overall: { done: 40, total: 100, fraction: 0.4, rate_per_s: 1.2, eta_s: 50, eta_confident: false },
    ...over,
  };
}

function live(jobId, progress, at = 200) {
  return { progress, stable: false, history: [], at };
}

const common = { now: NOW, stallMs: STALL };

/**
 * Each seed returns a full `buildRunSnapshot` input plus deterministic `now`.
 * Keys: stalledWorker, failedStage, nullJobError, nullJobErrorOnOtherAlbum,
 * dropdownLive, dropdownLiveAdvanced, pausedRun, runEnded.
 */
export const SEEDS = {
  stalledWorker: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, { pipeline: { stages: [activeStage('read'), activeStage('detect')], workers: [activeWorker(), pausedWorker(), stalledWorker(), doneWorker(), failedWorker()] } })) },
    hydrate: null, seed: null, jobId: 7,
  }),
  failedStage: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, { pipeline: { stages: [activeStage('read'), failedStage()], workers: [activeWorker()] } })) },
    hydrate: null, seed: null, jobId: 7,
  }),
  // Shown on the empty Scan page (no album selected).
  nullJobError: () => ({ ...common, status: status(null), jobs: [], liveByJob: {}, hydrate: null, seed: nullJobError(), jobId: null }),
  // The SAME null-job error must NOT match another album (audit P2-2).
  nullJobErrorOnOtherAlbum: () => ({ ...common, status: status(null), jobs: [baseJob(8, 'stopped')], liveByJob: {}, hydrate: null, seed: nullJobError(), jobId: 8 }),
  dropdownLive: () => ({
    ...common,
    status: status(7, { queue: [8] }),
    jobs: [baseJob(7, 'scanning'), baseJob(8, 'queued'), baseJob(9, 'done')],
    liveByJob: { 7: live(7, scanProgress(7, { overall: { done: 60, total: 100, fraction: 0.6, rate_per_s: 1.2, eta_s: 30, eta_confident: false } })) },
    hydrate: null, seed: null, jobId: 7,
  }),
  // A pushed status/live change WITHOUT opening anything: progress moves, active job moves.
  dropdownLiveAdvanced: () => ({
    ...common,
    status: status(8, { queue: [] }),
    jobs: [baseJob(7, 'stopped'), baseJob(8, 'scanning'), baseJob(9, 'done')],
    liveByJob: { 8: live(8, scanProgress(8, { overall: { done: 80, total: 100, fraction: 0.8, rate_per_s: 1.2, eta_s: 10, eta_confident: false } })) },
    hydrate: null, seed: null, jobId: 8,
  }),
  pausedRun: () => ({
    ...common,
    status: status(7, { tasks: [{ id: 1, label: 'Scanning', detail: '', state: 'paused', done: 40, total: 100, eta: null, error: null }] }),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, { pipeline: { stages: [activeStage('read')], workers: [pausedWorker()] } })) },
    hydrate: null, seed: null, jobId: 7,
  }),
  // Run end clears every stale lane (no active job).
  runEnded: () => ({
    ...common,
    status: status(null),
    jobs: [baseJob(7, 'done')],
    liveByJob: { 7: live(7, scanProgress(7, { state: 'done', pipeline: { stages: [activeStage('read')], workers: [activeWorker()] } })) },
    hydrate: null, seed: null, jobId: 7,
  }),
};
