// Normalized run snapshot built purely from engine status and live progress.
//
// Flowchart nodes, worker lanes, header counters, the TitleBar badge, and the
// jobs dropdown all read from `buildRunSnapshot`. Nothing here invents or
// defaults a stage, worker, or status: `scan_status` plus live events are the
// authority, and stage ids are carried through stage-agnostically.
//
// Also houses cross-cutting snapshot logic: ordering guards (snapshotOrder.mjs),
// worker lifecycle classification, error-to-job binding, and shared status labels.
import { orderWins, makeClock, applySnapshot } from './snapshotOrder.mjs';
import { scanView } from './scanStatus.mjs';
import { orderStages, stageStatus } from './scan.mjs';
import { selectLanes } from './ingest.mjs';
import { buildSurface, stageFailureReason, toneFor } from './surface.mjs';

// Re-export ordering and shared surface primitives so consumers need one import.
export { orderWins, makeClock, applySnapshot, stageFailureReason, toneFor };

/** Generous last-report window before flagging a worker stalled: cloud vision requests can take 30 s+. */
export const STALL_MS = 45000;

/** A worker whose engine `state` is terminal has finished and LEAVES the grid. */
const TERMINAL_WORKER = new Set(['done', 'failed']);

/**
 * Normalise a worker into the lifecycle state the lane grid draws.
 * Current engines emit `state` (working | paused | done | failed), `step`,
 * `last_report_at` (epoch ms), and `lane`/`worker_index`; legacy engines emit
 * busy | waiting | idle | paused, which map to working.
 *  - done / failed  -> lifecycle 'done' | 'failed' (leaves the grid; result goes to the feed);
 *  - paused         -> 'paused' (a parked worker is never "stalled");
 *  - any active worker past the stall window -> 'stalled' with its last step;
 *  - otherwise      -> 'working'.
 * Done-vs-stalled is decided by the engine's `state`; the UI only flags stalled
 * past the window when `last_report_at` is supplied.
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

// ---- Shared status vocabulary for flowchart, badge, and dropdown ----

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

// ---- Error binding and stage failures ----

/**
 * Bind a scan-failure seed to the album on screen. A seed with a null jobId (an
 * import that failed before an album existed) is shown only while no album is
 * selected so it never leaks onto unrelated albums. A seed with an id shows only
 * on that exact album, preventing cross-album banners or misdirected retries.
 */
export function scanSeedFor(seed, jobId) {
  if (!seed) return null;
  if (seed.jobId == null) return jobId == null ? seed : null;
  return seed.jobId === jobId ? seed : null;
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
 * Build the normalized run snapshot from authoritative engine data.
 * `status` is the engine status poll/push; `liveByJob` is the scan-progress
 * event map; `hydrate` is the open album's `scan_status` read; `seed` is a
 * failed `startScan`; `jobId` is the album currently on screen.
 *
 * The active run's view is resolved via `scanView` (hydrate-then-live with the
 * ordering guard). Stages, workers, and counters are taken verbatim from that
 * view's progress so new stages pass through stage-agnostically.
 *
 * `settings` is the album's resolved settings, `album` is its job record
 * (carrying any type suggestion), `identifyVram` is the GPU VRAM fit info,
 * `decisionService` names the serving decision model, and `subject` holds the
 * album's subject words. Together with the snapshot they build `surface`
 * (surface.mjs) for flowchart nodes, disabled steps, controls, VRAM warnings,
 * and type suggestions.
 *
 * @param {{ status?: any, jobs?: any[], liveByJob?: any, hydrate?: any, seed?: any, jobId?: number|null, now?: number, stallMs?: number, settings?: any, album?: any, identifyVram?: any, decisionService?: string|null, subject?: any }} [options]
 */
export function buildRunSnapshot(options = {}) {
  const {
    status = null, jobs = [], liveByJob = {}, hydrate = null, seed = null, jobId = null,
    now = Date.now(), stallMs = STALL_MS, settings = null, album = null,
    identifyVram = null, decisionService = null, subject = null,
  } = options;
  const activeJob = status?.activeJob ?? null;
  const queue = Array.isArray(status?.queue) ? status.queue : [];
  const tasks = Array.isArray(status?.tasks) ? status.tasks : [];
  const paused = tasks.some((t) => t.state === 'paused' && !/^Identifying\b/.test(String(t.label ?? '')));
  const pausedJobIds = paused && activeJob != null ? [activeJob] : [];

  const targetJobId = jobId ?? activeJob;
  const live = targetJobId != null ? liveByJob[targetJobId] : undefined;
  const targetRecord = targetJobId != null ? jobs.find((j) => j.id === targetJobId) ?? null : null;
  const activeJobRecord = activeJob != null ? jobs.find((j) => j.id === activeJob) ?? null : null;
  const view = scanView({ job: targetRecord, live, hydrate, activeJob });
  const progress = view.progress ?? null;

  // Whatever stages the snapshot carries, in flow order; nothing is invented.
  const stages = orderStages(progress?.pipeline?.stages ?? []);
  const workers = Array.isArray(progress?.pipeline?.workers) ? progress.pipeline.workers : [];
  const overall = progress?.overall ?? progress ?? {};
  const counters = {
    done: Number(overall.done ?? progress?.done ?? 0) || 0,
    total: Number(overall.total ?? progress?.total ?? targetRecord?.total ?? 0) || 0,
    fraction: Number.isFinite(overall.fraction) ? Math.max(0, Math.min(1, overall.fraction)) : null,
    stage: progress?.stage ?? null,
  };

  const runActive = view.state === 'running';
  const badge = activeJobRecord
    ? jobChip(activeJobRecord, { activeJob, queue, liveByJob, pausedJobIds })
    : { word: runStateWord(view.state), state: view.state, tone: toneFor(view.state), progress: counters.fraction };
  const base = {
    activeJob,
    paused,
    view,               // { state, source, progress, errors, ... } for the Scan screen
    progress,
    stages,             // stage-agnostic, for the flowchart
    workers,            // authoritative, for activeWorkers()
    counters,           // header counters, one source
    runActive,          // run end / stage transition clears stale lanes
    badge,
    jobs: dropdownRows(jobs, { activeJob, queue, liveByJob, pausedJobIds }),  // TitleBar dropdown
    failedStages: failedStages(stages),  // failed nodes + plain reasons + retry
    error: scanSeedFor(seed, jobId),     // run error banner bound to the correct job
    now,
    stallMs,
  };
  // Build the UI surface model directly from this snapshot.
  base.surface = buildSurface(
    { ...base, paused: targetJobId === activeJob ? paused : false },
    {
      settings, album, identifyVram, decisionService,
      subject: subject ?? undefined, jobId: targetJobId,
    },
  );
  return base;
}

/** Export stageStatus for callers that want the snapshot's per-stage word. */
export { stageStatus, buildSurface };
