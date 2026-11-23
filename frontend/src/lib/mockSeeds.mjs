// Deterministic dev/test seeds covering every run and surface state.
// Pure data (no engine, no timers) so `node --test` can exercise full run
// snapshots and surface views without Rust.
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

/** Current wire shape for a failed stage: `status: 'failed'` + `error`. */
export const failedStageWire = () => activeStage('identify:vision', { status: 'failed', done: 3, total: 10, errors: 1, error: 'The vision service stopped answering.' });

/** The nine grouped-run stages in flow order, each with an explicit status word. */
export const NINE_IDS = ['read', 'detect', 'sharpness', 'faces', 'identify:ocr', 'identify:decision', 'identify:vision', 'group', 'pick'];
export function nineStages(over = {}) {
  return NINE_IDS.map((id, i) => activeStage(id, {
    status: i < 4 ? 'done' : i === 4 ? 'running' : 'pending',
    done: i < 4 ? 100 : i === 4 ? 40 : 0, total: 100, workers_total: i < 4 ? 0 : 2,
    ...(over[id] ?? {}),
  }));
}

/** Resolved settings for a fully enabled album. */
export function resolvedSettings(over = {}) {
  return {
    identify_ocr: true, read_plates: true, read_numbers: true, read_text: true,
    use_decider: true, moments_backend: 'decider', use_vlm: true,
    scan_profile: 'motorsport', identify_vision_workers: 2, identify_decision_workers: 1,
    ...over,
  };
}

/** VRAM fit fixtures: both models fit concurrently, or only one fits (serial fallback). */
export const VRAM_FITS = () => ({ cardBytes: 12 * 2 ** 30, decisionBytes: 4.8 * 2 ** 30, visionBytes: 6 * 2 ** 30, fitsBoth: true, mode: 'concurrent' });
export const VRAM_SERIAL = () => ({ cardBytes: 8 * 2 ** 30, decisionBytes: 4.8 * 2 ** 30, visionBytes: 6 * 2 ** 30, fitsBoth: false, mode: 'serial' });

/** Settings where both decision and vision models run locally. */
export function bothLocalSettings(over = {}) {
  return resolvedSettings({
    moments_transport: 'in_process',
    moments_services: [{ id: 'moments-local', provider: 'in_process', enabled: true }],
    vlm_services: [{ id: 'vlm-local', provider: 'ollama', enabled: true }],
    use_decider: true, use_vlm: true,
    ...over,
  });
}

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
  // A null-job import failure must not appear on an unrelated album.
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

  // ── Surface state seeds for tests and dev previews ──

  /** The three identify sub-nodes replace the single identify block. */
  subNodes: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, {
      stage: 'identify:ocr', stage_count: 9,
      pipeline: { stages: nineStages(), workers: [activeWorker()] },
    })) },
    hydrate: null, seed: null, jobId: 7,
    settings: resolvedSettings(), album: baseJob(7, 'scanning'), identifyVram: VRAM_FITS(),
    decisionService: 'the local decision model',
  }),
  /** A paused DECISION node (`stage_pause`) with a parked worker. */
  pausedStage: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, {
      stage: 'identify:decision', stage_count: 9,
      pipeline: { stages: nineStages({
        'identify:ocr': { status: 'done', done: 100, total: 100, workers_total: 0 },
        'identify:decision': { status: 'paused', done: 40, total: 100, workers_total: 2, workers_busy: 0 },
      }), workers: [pausedWorker()] },
    })) },
    hydrate: null, seed: null, jobId: 7,
    settings: resolvedSettings(), album: baseJob(7, 'scanning'), identifyVram: VRAM_FITS(),
  }),
  /** A runtime-skipped VISION node (`stage_skip`) with its plain skip consequence. */
  skippedStage: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, {
      stage: 'identify:vision', stage_count: 9,
      pipeline: { stages: nineStages({
        'identify:ocr': { status: 'done', done: 100, total: 100, workers_total: 0 },
        'identify:decision': { status: 'done', done: 100, total: 100, workers_total: 0 },
        'identify:vision': { status: 'skipped', done: 0, total: 0, workers_total: 0 },
      }), workers: [] },
    })) },
    hydrate: null, seed: null, jobId: 7,
    settings: resolvedSettings(), album: baseJob(7, 'scanning'), identifyVram: VRAM_FITS(),
  }),
  /** Disabled steps: OCR, Decision, and Vision all switched off for this album. */
  greyedOff: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, {
      stage: 'group', stage_count: 9,
      pipeline: { stages: nineStages({
        'identify:ocr': { status: 'skipped', done: 0, total: 0, workers_total: 0 },
        'identify:decision': { status: 'skipped', done: 0, total: 0, workers_total: 0 },
        'identify:vision': { status: 'skipped', done: 0, total: 0, workers_total: 0 },
      }), workers: [] },
    })) },
    hydrate: null, seed: null, jobId: 7,
    settings: resolvedSettings({ identify_ocr: false, use_decider: false, moments_backend: 'vlm', use_vlm: false }),
    album: baseJob(7, 'scanning'), identifyVram: VRAM_FITS(),
  }),
  /** Serial VRAM fallback: both models run locally, but only one fits in VRAM at a time. */
  serialVram: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, { pipeline: { stages: nineStages(), workers: [activeWorker()] } })) },
    hydrate: null, seed: null, jobId: 7,
    settings: bothLocalSettings(), album: baseJob(7, 'scanning'), identifyVram: VRAM_SERIAL(),
  }),
  /** Both models local and enough VRAM: the concurrency preference is visible. */
  concurrentVram: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, { pipeline: { stages: nineStages(), workers: [activeWorker()] } })) },
    hydrate: null, seed: null, jobId: 7,
    settings: bothLocalSettings(), album: baseJob(7, 'scanning'), identifyVram: VRAM_FITS(),
  }),
  /** An `auto` album carrying a shoot-type suggestion that has not yet been accepted. */
  typeSuggestion: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning', { scan_profile: 'auto', shoot_type_suggested: 'sport-rodeo', shoot_type_confidence: 0.82 })],
    liveByJob: { 7: live(7, scanProgress(7, { pipeline: { stages: nineStages(), workers: [activeWorker()] } })) },
    hydrate: null, seed: null, jobId: 7,
    settings: resolvedSettings({ scan_profile: 'auto' }),
    album: baseJob(7, 'scanning', { scan_profile: 'auto', shoot_type_suggested: 'sport-rodeo', shoot_type_confidence: 0.82 }),
    identifyVram: VRAM_FITS(),
  }),
  /** Failed stage in current `status`/`error` wire shape, bound to its job. */
  failedStageWire: () => ({
    ...common,
    status: status(7),
    jobs: [baseJob(7, 'scanning')],
    liveByJob: { 7: live(7, scanProgress(7, { pipeline: { stages: [activeStage('read'), failedStageWire()], workers: [activeWorker()] } })) },
    hydrate: null, seed: null, jobId: 7,
    settings: resolvedSettings(), album: baseJob(7, 'scanning'), identifyVram: VRAM_FITS(),
  }),
};
