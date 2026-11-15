// Pure logic for the scan screen: which stage is the bottleneck, whether an ETA can be trusted, and the words around them.

/** The stages in flow order; the engine's own order is used when it sends something else. */
export const STAGE_ORDER = ['read', 'detect', 'sharpness', 'faces', 'identify', 'group', 'pick'];

/** Stage nodes left to right: known stages by flow order, then any unknown ones as the engine sent them. */
export function orderStages(stages) {
  const list = Array.isArray(stages) ? stages : [];
  const rank = (id) => { const i = STAGE_ORDER.indexOf(id); return i < 0 ? STAGE_ORDER.length : i; };
  return list.map((s, i) => [s, i]).sort((a, b) => rank(a[0].id) - rank(b[0].id) || a[1] - b[1]).map((p) => p[0]);
}

const BARRIER = new Set(['group', 'pick']);

/**
 * 'off' for a stage this shoot does not run (no workers, nothing to do); 'done' once every item has passed and
 * nothing is in flight; 'active' while anything is moving or has moved; else 'pending'.
 */
export function stageStatus(stage) {
  if (!stage) return 'pending';
  const total = stage.total ?? 0, done = stage.done ?? 0;
  if ((stage.workers_total ?? 0) === 0 && total === 0 && done === 0) return 'off';
  if (total > 0 && done >= total && (stage.workers_busy ?? 0) === 0) return 'done';
  if ((stage.workers_busy ?? 0) > 0 || done > 0) return 'active';
  return 'pending';
}

/**
 * Items waiting at a stage: what the stage upstream has finished that this one has not picked up yet.
 * The engine's `queue_len` is everything left, which is the same for every stage at the start; this is the
 * backlog actually standing in front of the stage. The first stage is a source and never has one.
 */
export function readyQueue(ordered, index) {
  const stage = ordered[index];
  if (!stage || (stage.total ?? 0) <= 0) return 0;
  let prev = null;
  for (let i = index - 1; i >= 0 && !prev; i -= 1) if (stageStatus(ordered[i]) !== 'off') prev = ordered[i];
  if (!prev) return 0;
  // Grouping and picking need every frame, so nothing waits for them until the stage before has finished.
  if (BARRIER.has(stage.id)) return (prev.done ?? 0) >= (prev.total ?? 0) ? Math.max(0, stage.total - (stage.done ?? 0)) : 0;
  return Math.max(0, Math.min(stage.total, prev.done ?? 0) - (stage.done ?? 0));
}

/**
 * The stage holding the scan back, as a stage id or null.
 * First choice: the biggest backlog standing in front of a running stage (a tie goes to the slower stage); a
 * backlog under `minQueue` is just flow. Second choice, for workers that carry a frame through several stages:
 * the stage that most workers are inside right now (at least two, and more than any other).
 */
export function bottleneck(stages, workers, minQueue = 3) {
  const ordered = orderStages(stages);
  let best = null, bestQueue = 0;
  ordered.forEach((s, i) => {
    if (stageStatus(s) === 'off' || stageStatus(s) === 'done' || (s.workers_total ?? 0) <= 0) return;
    const queue = readyQueue(ordered, i);
    if (queue < minQueue) return;
    const rate = s.rate_per_s > 0 ? s.rate_per_s : Infinity, bestRate = best && best.rate_per_s > 0 ? best.rate_per_s : Infinity;
    if (!best || queue > bestQueue || (queue === bestQueue && rate < bestRate)) { best = s; bestQueue = queue; }
  });
  if (best) return best.id;
  const counts = new Map();
  for (const w of Array.isArray(workers) ? workers : []) if (w.state !== 'idle') counts.set(w.stage, (counts.get(w.stage) ?? 0) + 1);
  const ranked = [...counts.entries()].filter(([id]) => ordered.some((s) => s.id === id)).sort((a, b) => b[1] - a[1]);
  return ranked.length && ranked[0][1] >= 2 && (ranked.length === 1 || ranked[0][1] > ranked[1][1]) ? ranked[0][0] : null;
}

/**
 * 0..1 across every stage this scan runs, for the bar under the header. The engine's `overall.fraction` already
 * weights each stage by its cost per item, so it is preferred; the stage mean below is the fallback for a payload
 * from before it existed (or a mock without one).
 */
export function overallFraction(stages, overall) {
  if (overall && Number.isFinite(overall.fraction)) return Math.max(0, Math.min(1, overall.fraction));
  return derivedFraction(stages);
}

/** The old per-stage mean, kept for payloads with no `overall`. */
function derivedFraction(stages) {
  const real = (Array.isArray(stages) ? stages : []).filter((s) => stageStatus(s) !== 'off');
  if (!real.length) return 0;
  const sum = real.reduce((n, s) => n + ((s.total ?? 0) > 0 ? Math.min(1, (s.done ?? 0) / s.total) : 0), 0);
  return sum / real.length;
}

/**
 * The time left for the whole scan: the engine's `overall.eta_s` when present (a null there means it cannot say),
 * else the slowest running stage's own ETA, and null while any running stage has no ETA of its own yet.
 */
export function overallEta(stages, overall) {
  if (overall) return Number.isFinite(overall.eta_s) ? overall.eta_s : null;
  return derivedEta(stages);
}

function derivedEta(stages) {
  const running = (Array.isArray(stages) ? stages : []).filter((s) => stageStatus(s) === 'active' && (s.total ?? 0) > (s.done ?? 0));
  if (!running.length) return null;
  let slowest = 0;
  for (const s of running) {
    if (s.eta_s == null || !Number.isFinite(s.eta_s)) return null;
    slowest = Math.max(slowest, s.eta_s);
  }
  return slowest;
}

/** Why a worker is not working, in the words the lane shows. */
export function waitingText(worker) {
  if (!worker || worker.state === 'idle') return 'idle';
  if (worker.state !== 'waiting') return 'working';
  switch (worker.waiting_for) {
    case 'gpu': return 'waiting for the GPU';
    case 'lock': return 'waiting for a lock';
    case 'ollama': return 'waiting for Ollama';
    case 'paused': return 'paused';
    default: return 'waiting';
  }
}

/** Workers of one stage, by index. */
export function workersOf(workers, stageId) {
  return (Array.isArray(workers) ? workers : []).filter((w) => w.stage === stageId).sort((a, b) => a.worker_index - b.worker_index);
}

/** The one-line readout under a node: busy, waiting and idle counts. */
export function workerCounts(stage, workers) {
  const mine = workersOf(workers, stage.id);
  if (!mine.length) return { busy: stage.workers_busy ?? 0, waiting: 0, idle: Math.max(0, (stage.workers_total ?? 0) - (stage.workers_busy ?? 0)) };
  return {
    busy: mine.filter((w) => w.state === 'busy').length,
    waiting: mine.filter((w) => w.state === 'waiting').length,
    idle: mine.filter((w) => w.state === 'idle').length,
  };
}

/** "8s", "2 min 05 s", "1 h 12 min": a duration for an ETA, never finer than the estimate deserves. */
export function formatDuration(seconds) {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return '';
  const s = Math.max(1, Math.round(seconds));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, '0')} s`;
  const m = Math.round(s / 60);
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')} min`;
}

/** Items per second as the screen prints it: "42/s", "3.4/s", or "" while unknown. */
export function formatRate(rate) {
  if (rate == null || !Number.isFinite(rate) || rate <= 0) return '';
  return rate >= 10 ? `${Math.round(rate)}/s` : `${rate.toFixed(1)}/s`;
}

const KEEP = 12;
const MIN_SAMPLES = 6;

/**
 * Remember the engine's ETA guesses, as projected finish times (now + eta), so a wandering estimate can be
 * told from a settled one. Samples with no ETA, or before any progress, reset the history.
 */
export function pushEtaSample(history, nowMs, etaS, done, total) {
  if (etaS == null || !Number.isFinite(etaS) || etaS < 0 || !(done > 0) || !(total > 0)) return [];
  const next = [...(history ?? []), { finish: nowMs + etaS * 1000, now: nowMs }];
  return next.slice(-KEEP);
}

/** True when enough samples agree: the projected finish moved less than a quarter of the time left (or 3 s). */
export function etaStable(history) {
  if (!Array.isArray(history) || history.length < MIN_SAMPLES) return false;
  const finishes = history.map((h) => h.finish);
  const spread = Math.max(...finishes) - Math.min(...finishes);
  const left = Math.max(0, finishes[finishes.length - 1] - history[history.length - 1].now);
  return spread <= Math.max(3000, left * 0.25);
}

/** What the header says under "Time left": "estimating…" until the ETA is stable, then "about 2 min 05 s". */
export function etaLabel(etaS, stable) {
  if (etaS == null || !Number.isFinite(etaS) || !stable) return 'estimating…';
  if (etaS < 2) return 'almost done';
  return `about ${formatDuration(etaS)}`;
}

/** 0..100, never past 100 and never NaN. */
export function percent(done, total) {
  return total > 0 ? Math.max(0, Math.min(100, Math.round((done * 100) / total))) : 0;
}

/** The summary card's line: "3,061 frames · 412 bursts · 286 suggested keepers · 640 soft · 12 closed eyes". */
export function summaryLine(total, found) {
  const n = (v) => (v ?? 0).toLocaleString('en-US');
  const f = found ?? {};
  const noun = (v, one, many) => `${n(v)} ${v === 1 ? one : many}`;
  const parts = [
    noun(total, 'frame', 'frames'),
    noun(f.bursts, 'burst', 'bursts'),
    `${n(f.keepers)} suggested ${f.keepers === 1 ? 'keeper' : 'keepers'}`,
    `${n(f.soft)} soft`,
  ];
  if ((f.closed_eyes ?? 0) > 0) parts.push(`${n(f.closed_eyes)} closed ${f.closed_eyes === 1 ? 'eye' : 'eyes'}`);
  return parts.join(' · ');
}

/** Clamp a worker count into the pool's range, whole numbers only. */
export function clampWorkers(value, limit) {
  const n = Math.round(Number(value));
  if (!Number.isFinite(n)) return limit.current;
  return Math.min(limit.max, Math.max(limit.min, n));
}

/** Which settings key holds each pool, and what the pool does, for the worker panel. */
export const WORKER_POOLS = [
  { id: 'cull', key: 'workers', label: 'Frames', hint: 'The pipeline: read to faces' },
  { id: 'detect', key: 'detect_workers', label: 'Detector', hint: 'Subject detector sessions' },
  { id: 'preview', key: 'preview_workers', label: 'Previews', hint: 'Previews and thumbnails' },
  { id: 'identify', key: 'analysis_workers', label: 'Identify', hint: 'Vision model calls' },
];
