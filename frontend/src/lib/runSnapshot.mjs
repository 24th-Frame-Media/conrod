// The ONE run snapshot (plan §6.1). Pure, so `node --test` pins it.
//
// Everything a screen draws about a run — flowchart nodes, lane grid, header
// counters, the TitleBar badge and the jobs dropdown — reads from the object
// `buildRunSnapshot` returns. Nothing here invents or defaults a stage, a worker
// or a status: the engine's `scan_status` + live events (hydrate-then-live) are
// the authority, and every stage id the snapshot carries is rendered as-is so
// B1's new OCR/DECISION/VISION stages drop in without a change here.
//
// Cross-cutting concerns that used to be scattered (and disagree) also live
// here: the ordering guard (snapshotOrder.mjs), the worker lifecycle (§6.4),
// error-to-job binding (§6.6) and the shared status vocabulary (§6.7).
import { orderWins, makeClock, applySnapshot } from './snapshotOrder.mjs';
import { scanView } from './scanStatus.mjs';
import { orderStages, stageStatus } from './scan.mjs';
import { selectLanes } from './ingest.mjs';

// Re-export the ordering primitives so consumers/tests need one import.
export { orderWins, makeClock, applySnapshot };

/** Generous last-report window (plan §9): a cloud vision ask can take 30 s+. Slow is not dead. */
export const STALL_MS = 45000;

/** A worker whose engine `state` is terminal has finished and LEAVES the grid. */
const TERMINAL_WORKER = new Set(['done', 'failed']);

/**
 * Normalise one worker to the honest lifecycle the grid may draw (§6.4).
 * Contract B1 emits `state` working | paused | done | failed, `step`,
 * `last_report_at` (epoch ms), `lane`/`worker_index`. Legacy engines emit
 * busy | waiting | idle | paused, which stay working here.
 *  - done / failed  -> lifecycle 'done' | 'failed' (result goes to the feed);
 *  - paused         -> 'paused' (a parked worker is never "stalled");
 *  - any active worker past the generous window -> 'stalled' with its last step;
 *  - otherwise      -> 'working'.
 * Done-vs-stalled is decided by the ENGINE's `state`; the UI only flags stalled
 * past the window, and only when the engine supplies `last_report_at`.
 */
export function workerLifecycle(worker, now = Date.now(), stallMs = STALL_MS) {
  if (!worker) return { state: 'idle', step: null, lifecycle: 'done', stalled: false };
  const state = String(worker.state ?? 'idle').toLowerCase();
  const step = worker.current?.step ?? worker.step ?? null;
  if (TERMINAL_WORKER.has(state)) return { ...worker, step, lifecycle: state, stalled: false };
  if (state === 'paused') return { ...worker, step, lifecycle: 'paused', stalled: false };
  const last = worker.last_report_at;
  const stalled = Number.isFinite(last) && stallMs > 0 && (now - last) > stallMs;
  return { ...worker, step, lifecycle: stalled ? 'stalled' : 'working', stalled };
}

/**
 * The ONLY workers the lane grid draws. Finished workers (done/failed) are
 * dropped — they leave the grid — and every remaining worker is shown on its
 * real current step. A worker past the last-report window is honestly stalled,
 * never busy. `runActive: false` (run end) clears every stale lane; on a stage
 * transition the authoritative `workers` array simply no longer carries the old
 * stage's lanes, so they cannot survive. Returns `{ lanes, hidden, stalled }`.
 *
 * @param {any[]} workers
 * @param {{ now?: number, stallMs?: number, filter?: any, cap?: number, all?: boolean, paused?: boolean, identify?: boolean, runActive?: boolean }} [options]
 */
export function activeWorkers(workers, options = {}) {
  const { now = Date.now(), stallMs = STALL_MS, filter = null, cap = 8, all = false, paused = false, identify = false, runActive = true } = options;
  if (!runActive) return { lanes: [], hidden: 0, stalled: 0 };
  const list = (Array.isArray(workers) ? workers : [])
    .map((w) => workerLifecycle(w, now, stallMs))
    .filter((w) => !TERMINAL_WORKER.has(w.lifecycle))
    // A stalled worker is marked so the lane says STALLED, not LIVE.
    .map((w) => (w.lifecycle === 'stalled' ? { ...w, state: 'stalled' } : w));
  const stalled = list.filter((w) => w.lifecycle === 'stalled').length;
  const picked = selectLanes(list, { filter, cap, all, paused, identify });
  return { lanes: picked.lanes, hidden: picked.hidden, stalled };
}

// ---- One status vocabulary, used by the flowchart, badge and dropdown (§6.7) ----

export const RUN_WORDS = Object.freeze({
  running: 'Running', queued: 'Queued', paused: 'Paused', stopped: 'Stopped',
  failed: 'Failed', error: 'Failed', done: 'Done', done_errors: 'Done',
  // Stored statuses that mean "not actually running any more".
  scanning: 'Stopped', indexed: 'Stopped', incomplete: 'Stopped', idle: 'Idle',
});

/** The one canonical short word for a run state. */
export function runStateWord(state) {
  return RUN_WORDS[String(state ?? '').toLowerCase()] ?? 'Idle';
}

/** The colour/tone class for a state, shared by every chip and node. */
export function toneFor(state) {
  switch (String(state ?? '').toLowerCase()) {
    case 'running': return 'running';
    case 'paused': return 'paused';
    case 'queued': return 'queued';
    case 'failed': case 'error': return 'error';
    case 'done': case 'done_errors': return 'done';
    case 'stopped': case 'scanning': case 'indexed': case 'incomplete': return 'stopped';
    default: return 'idle';
  }
}

/**
 * The live chip + progress for ONE job, derived only from the single snapshot
 * (status.activeJob, queue and the live events). No component invents a default.
 * `progress` is 0..1 or null. Changing a pushed status or a live event changes
 * the returned chip on the next render — the dropdown updates without opening.
 */
export function jobChip(job, { activeJob = null, queue = [], liveByJob = {}, pausedJobIds = [] } = {}) {
  if (!job) return { word: 'Idle', state: 'idle', tone: 'idle', progress: null };
  let state = String(job.status ?? 'idle').toLowerCase();
  if (activeJob === job.id) {
    state = pausedJobIds.includes(job.id) ? 'paused' : 'running';
  } else if ((Array.isArray(queue) ? queue : []).includes(job.id)) {
    state = 'queued';
  }
  let progress = null;
  const live = liveByJob?.[job.id];
  if (activeJob === job.id && live?.progress) {
    const p = live.progress.overall ?? live.progress;
    progress = Number.isFinite(p.fraction) ? Math.max(0, Math.min(1, p.fraction))
      : p.total > 0 ? Math.max(0, Math.min(1, (p.done ?? 0) / p.total)) : null;
  } else if (job.total > 0) {
    progress = Math.max(0, Math.min(1, (job.done ?? 0) / job.total));
  }
  return { word: runStateWord(state), state, tone: toneFor(state), progress };
}

/** Recent/active jobs for the TitleBar dropdown, active first, then queued, then newest. */
export function dropdownRows(jobs, { activeJob = null, queue = [], liveByJob = {}, pausedJobIds = [], limit = 8 } = {}) {
  const list = Array.isArray(jobs) ? jobs : [];
  const rank = (j) => (activeJob === j.id ? 0 : (Array.isArray(queue) && queue.includes(j.id)) ? 1 : 2);
  const sorted = [...list].sort((a, b) => rank(a) - rank(b) || (Number(b.created_at) || 0) - (Number(a.created_at) || 0));
  return sorted.slice(0, limit).map((j) => ({ id: j.id, label: j.label, chip: jobChip(j, { activeJob, queue, liveByJob, pausedJobIds }) }));
}

// ---- Errors are never silent (§6.6) ----

/**
 * Bind a scan-failure seed to the album on screen. A seed with a null jobId (an
 * import that failed BEFORE an album existed) is shown ONLY while no album is
 * selected; it can NEVER match every album (audit 4-fe-state P2-2). A seed with
 * an id shows only on that exact album, so album B never shows album A's banner
 * and "Retry" never replays the wrong folder's args.
 */
export function scanSeedFor(seed, jobId) {
  if (!seed) return null;
  if (seed.jobId == null) return jobId == null ? seed : null;
  return seed.jobId === jobId ? seed : null;
}

/** A plain reason for a stage that failed, or '' when the stage is fine. */
export function stageFailureReason(stage) {
  if (!stage) return '';
  if (stage.state === 'failed' || stage.failed === true) {
    return String(stage.reason ?? stage.error ?? 'This stage stopped with an error.').trim() || 'This stage stopped with an error.';
  }
  return '';
}

export function stageFailed(stage) {
  return Boolean(stageFailureReason(stage));
}

/** Every failed stage as `{ id, label, reason }`, in whatever stage ids the engine sent. */
export function failedStages(stages) {
  return (Array.isArray(stages) ? stages : [])
    .filter(stageFailed)
    .map((s) => ({ id: s.id, label: s.label ?? String(s.id).toUpperCase(), reason: stageFailureReason(s) }));
}

// ---- The single snapshot ----

/**
 * Build the ONE normalized run snapshot from the engine's authoritative data.
 * `status` is the single `status` poll/push (useEngine); `liveByJob` is the
 * scan-progress event map; `hydrate` is the open album's scan_status read;
 * `seed` is a failed `startScan`. `jobId` is the album currently on screen.
 *
 * The active run's view is decided by `scanView` (hydrate-then-live, with the
 * ordering guard). Stages, workers and counters are taken verbatim from that
 * view's progress — stage-agnostic, so new stages need no change here.
 *
 * @param {{ status?: any, jobs?: any[], liveByJob?: any, hydrate?: any, seed?: any, jobId?: number|null, now?: number, stallMs?: number }} [options]
 */
export function buildRunSnapshot(options = {}) {
  const { status = null, jobs = [], liveByJob = {}, hydrate = null, seed = null, jobId = null, now = Date.now(), stallMs = STALL_MS } = options;
  const activeJob = status?.activeJob ?? null;
  const queue = Array.isArray(status?.queue) ? status.queue : [];
  const tasks = Array.isArray(status?.tasks) ? status.tasks : [];
  const paused = tasks.some((t) => t.state === 'paused' && !/^Identifying\b/.test(String(t.label ?? '')));
  const pausedJobIds = paused && activeJob != null ? [activeJob] : [];

  const live = activeJob != null ? liveByJob[activeJob] : undefined;
  const activeJobRecord = activeJob != null ? jobs.find((j) => j.id === activeJob) ?? null : null;
  const view = scanView({ job: activeJobRecord, live, hydrate, activeJob });
  const progress = view.progress ?? null;

  // Whatever stages the snapshot carries, in flow order; nothing is invented.
  const stages = orderStages(progress?.pipeline?.stages ?? []);
  const workers = Array.isArray(progress?.pipeline?.workers) ? progress.pipeline.workers : [];
  const overall = progress?.overall ?? progress ?? {};
  const counters = {
    done: Number(overall.done ?? progress?.done ?? 0) || 0,
    total: Number(overall.total ?? progress?.total ?? activeJobRecord?.total ?? 0) || 0,
    fraction: Number.isFinite(overall.fraction) ? Math.max(0, Math.min(1, overall.fraction)) : null,
    stage: progress?.stage ?? null,
  };

  const runActive = view.state === 'running';
  return {
    activeJob,
    paused,
    view,               // { state, source, progress, errors, ... } for the Scan screen
    progress,
    stages,             // stage-agnostic, for the flowchart
    workers,            // authoritative, for activeWorkers()
    counters,           // header counters, one source
    runActive,          // run end / stage transition clears stale lanes
    badge: { word: runStateWord(view.state), state: view.state, tone: toneFor(view.state), progress: counters.fraction },
    jobs: dropdownRows(jobs, { activeJob, queue, liveByJob, pausedJobIds }),  // TitleBar dropdown
    failedStages: failedStages(stages),  // failed nodes + plain reasons + retry
    error: scanSeedFor(seed, jobId),     // run error banner bound to the correct job
    now,
    stallMs,
  };
}

/** Export stageStatus for callers that want the snapshot's per-stage word. */
export { stageStatus };
