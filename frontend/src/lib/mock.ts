/* Dev-only engine stand-in: `vite dev` + `?mock=1`. Imported dynamically behind import.meta.env.DEV, so it is not in `dist`.
   Extra params: page=library|scan|ingest|identify|review|known|settings, viewer, popover, scan (a scan is running), profile=<shoot preset id>. */
import { startIngestSim, type IngestSim } from './mockIngest';
import { mockDeciderStatus, startDeciderSim } from './mockDecider';
import type { ShootStats } from './shot.mjs';
import { applyOverrides, defaultSummary, overridesOf } from './presetPolicy.mjs';
import { profileDefaults } from './wizard.mjs';
import type { Bootstrap, DeciderLive, Detection, Frame, GlobalStats, IdentifyItem, IdentifyLive, IdentifyStage, Job, KnownVehicle, Page, PipelineWorker, ScanProgress, ScanStatus, Settings, Status, WorkerLimits } from './types';
import * as aiCard from './aiCard.mjs';
import { nineStages, resolvedSettings, bothLocalSettings, VRAM_SERIAL, VRAM_FITS } from './mockSeeds.mjs';

/** `?mock=1&page=ingest&surface=<state>` paints a static grouped-run snapshot
 *  (§6.8 mock seeds): sub-nodes, paused, skipped, greyed, serial VRAM, a type
 *  suggestion, a failed stage, a stalled worker or a live dropdown. */
const SURFACE_JOB = 3;
const surfaceName = () => params().get('surface');

export type DevStart = {
  page?: Page; jobId?: number; viewer?: boolean; popover?: boolean;
  /** `?lightroom=sent|export|import|imported|stats` opens the sheet or dialog a screenshot needs. */
  lightroom?: 'export' | 'sent' | 'import' | 'imported' | 'stats';
};
const params = () => new URLSearchParams(location.search);
/** Dev QA: `&aikeytest=fail` makes every real AI-service check command refuse
 *  the key/host with a plain reason, so the Test button's failure state can be
 *  screenshotted without an interactive click (pair with `&aitest=1`). */
const keyFails = () => params().get('aikeytest') === 'fail';
/** Dev QA: `&aidl=installed|downloading|failed|unavailable` seeds the built-in
 *  rows' four states. install_models / install_decider run a simulated install
 *  with real byte totals (the engine's own asset sizes), so a Download is only
 *  "done" once the row's files are marked present — never an instant flip. */
const aidl = params().get('aidl');
let visionInstalled = aidl === 'installed';
let decisionInstalled = aidl === 'installed' || mockDeciderStatus().state === 'ready';

/** The engine's real pinned sizes (scripts/assets.json), so mock progress is honest. */
const VISION_TOTAL = 5027784800 + 1159029824;
const DECIDER_TOTAL = 5157837472 + 672423040 + 38090643;
type MockInstall = { id: string; done: number; total: number; error: string | null };
let mockInstall: MockInstall | null = null;
let installTimer: number | null = null;
let dlSeedStarted = false;

function startMockInstall(id: 'builtin-vision' | 'decider', opts: { fail?: boolean; slow?: boolean } = {}) {
  if (installTimer != null) window.clearInterval(installTimer);
  const total = id === 'builtin-vision' ? VISION_TOTAL : DECIDER_TOTAL;
  const failAt = opts.fail ? Math.round(total * 0.3) : null;
  mockInstall = { id, done: 0, total, error: null };
  const step = Math.round(total / (opts.slow ? 400 : 40));
  installTimer = window.setInterval(() => {
    if (!mockInstall) return;
    mockInstall.done = Math.min(total, mockInstall.done + step);
    if (failAt != null && mockInstall.done >= failAt) {
      mockInstall.error = `Not enough disk space for the model: it needs ${(total / 2 ** 30).toFixed(1)} GB free, but only 66 MB is left. Free some space and try again.`;
      if (installTimer != null) window.clearInterval(installTimer);
      installTimer = null;
      return;
    }
    if (mockInstall.done >= total) {
      if (id === 'builtin-vision') visionInstalled = true; else decisionInstalled = true;
      mockInstall = null;
      if (installTimer != null) window.clearInterval(installTimer);
      installTimer = null;
    }
  }, 250);
}

/** The local model rows, shaped exactly like the engine's `models_status`. */
function localRows() {
  const deciderReady = decisionInstalled && mockDeciderStatus().state === 'ready';
  return [
    { id: 'detector', name: 'Detector', file: 'yolo26m-960.onnx', ready: true, downloaded: true, available: true },
    { id: 'faces', name: 'Faces', file: 'face_detection_yunet_2023mar.onnx', ready: true, downloaded: true, available: true },
    { id: 'similarity', name: 'Similarity', file: 'dinov2-small-quantized.onnx', ready: true, downloaded: true, available: true },
    { id: 'plates', name: 'Plates', file: 'yolo-v9-t-640-license-plates-end2end.onnx', ready: true, downloaded: true, available: true },
    { id: 'text', name: 'Text (OCR)', file: 'ch_PP-OCRv4_det_infer.onnx', ready: true, downloaded: true, available: true },
    { id: 'exiftool', name: 'ExifTool', file: 'exiftool', ready: true, downloaded: true, available: true },
    {
      id: 'decider', name: 'Decision model', file: 'decider-flash-v1.2-q8_0.gguf',
      ready: deciderReady, downloaded: decisionInstalled, optional: true, available: true,
      detail: decisionInstalled ? mockDeciderStatus().message : 'Not installed yet.',
    },
    {
      id: 'builtin-vision', name: 'Vision model (built-in)', file: 'Qwen3VL-8B-Instruct-Q4_K_M.gguf',
      ready: visionInstalled, downloaded: visionInstalled, optional: true, available: aidl !== 'unavailable',
      detail: visionInstalled ? 'On this PC.' : aidl === 'unavailable' ? 'Not available yet: no pinned source to download it from.' : 'Not downloaded yet (5.8 GB).',
    },
    {
      id: 'quality', name: 'Quality', file: 'nima-mobilenet-technical.onnx',
      ready: false, downloaded: false, optional: true, available: false,
      detail: 'Not available yet: no pinned source to download it from.',
    },
  ];
}

/** `?mock=1&page=ingest` (or `page=scan`) plays a ~80 s ingest of album 3: scan lanes, group, pick, vision, done. `page=identify` plays only the vision phase.
    Tuning: `&speed=4` hurries it, `&at=30` starts 30 s in (scan 0-61, group 61-64, pick 64-67, vision 67-89, done after 90), `&paused` starts it paused, `&frames=` `&cars=` size it, `&theme=light`. */
export function devStart(): DevStart {
  const pages: Record<string, Page> = { library: 'Library', scan: 'Scan', ingest: 'Scan', identify: 'Identify', review: 'Review', known: 'Known vehicles', settings: 'Settings' };
  const page = pages[params().get('page') ?? ''];
  const lightroom = params().get('lightroom');
  return {
    page, jobId: page === 'Review' || page === 'Identify' ? 1 : page === 'Scan' ? 3 : undefined, viewer: params().has('viewer'), popover: params().has('popover'),
    lightroom: lightroom === 'export' || lightroom === 'sent' || lightroom === 'import' || lightroom === 'imported' || lightroom === 'stats' ? lightroom : undefined,
  };
}

/* Plain, invented vehicle names for the dev mock — no real teams or clients. */
const CARS: [string, string, string, string, string, string][] = [
  ['Ford', 'Mustang GT', 'red', 'Trackside Motorsport', '17', 'TSM17'],
  ['Chevrolet', 'Camaro ZL1', 'dodgerblue', 'Apex Racing', '88', 'APX88'],
  ['Toyota', 'GR Supra', 'white', 'Club Competitors', '5', 'CLB5'],
  ['Porsche', '911 GT3 R', 'gold', 'Garage 23', '911', 'G23911'],
  ['Mazda', 'MX-5', 'silver', 'Privateer', '23', 'PRV23'],
  ['BMW', 'M4 GT3', 'darkorange', 'Northside Racing', '7', 'NRT7'],
];
const W = 6000, H = 4000;
const COMP_MODES = ['centre', 'thirds', 'phi'] as const;

/** One frame whose recorded size is landscape, as some cameras report it. */
const TALL_FRAME = { id: 1000, width: 6000, height: 4000 };

/** Real example frames (docs/images/benchmark_*.jpg) copied into public/mock/. The dev mock
 *  shows these instead of drawn placeholders, so its covers and grids read like a real library. */
const COVERS = ['/mock/gemini.jpg', '/mock/mustang.jpg', '/mock/datsun.jpg'];
const photo = (k: number): string => COVERS[((k % COVERS.length) + COVERS.length) % COVERS.length];

type Db = { jobs: Job[]; frames: Frame[]; dets: Detection[]; known: KnownVehicle[]; settings: Settings; ticks: number; identifySkipped: boolean };
let cache: Db | undefined;

function makeDb(): Db {
  const frames: Frame[] = [], dets: Detection[] = [];
  const count = Math.max(8, Number(params().get('frames')) || 64);
  for (let i = 0; i < count; i += 1) {
    const [make, model, colour, team, no, plate] = CARS[i % CARS.length];
    // Burst 2 is all soft (two stars or less) and has no engine pick: the grid must
    // say "No keeper" there instead of proposing the least-bad frame.
    const soft = Math.floor(i / 5) === 2;
    const id = 1000 + i, rating = soft ? 0.6 + ((i * 7) % 6) / 100 : 0.5 + ((i * 37) % 48) / 100;
    const cx = 0.38 + ((i * 13) % 25) / 100, s = 0.5 + ((i * 7) % 20) / 100;
    const read = i % 4 !== 3;
    // The first frame is the tall one: a tall photo with a recorded shape that does not match it.
    const tall = id === TALL_FRAME.id;
    const fw = tall ? TALL_FRAME.width : W;
    const fh = tall ? TALL_FRAME.height : H;
    // Composition: frame 0 opens with a busy background on a pan, frame 3 is the clean pan the strip marks.
    const clutter = i === 3 ? 0.18 : i % 3 === 0 ? 0.85 + ((i * 11) % 9) / 100 : 0.4 + ((i * 17) % 24) / 100;
    frames.push({
      id, path: `D:\\Photos\\2026\\10\\Challenge Bathurst\\IMG_${4200 + i}.CR3`, status: i > count - 4 ? 'pending' : 'done',
      thumb_path: photo(i), preview_path: null, width: fw, height: fh, rating,
      manual_stars: soft ? null : i % 9 === 4 ? 5 : i % 11 === 6 ? 2 : null, rejected: i % 13 === 8 ? 1 : 0, picked: i % 17 === 5 ? 1 : 0, burst_key: Math.floor(i / 5), error: null,
      moments: i % 7 === 3 ? JSON.stringify({ backend: 'decider', strength: 0.9, readings: [{ tag: 'tyre_smoke', p: 0.93, calibrated: true }, { tag: 'flames', p: 0.62, calibrated: true }] }) : null,
      comp_placement: 0.52 + ((i * 23) % 42) / 100, comp_mode: COMP_MODES[i % 3],
      comp_balance_x: ((i * 13 + 15) % 40 - 20) / 100, comp_balance_y: ((i * 19 + 10) % 40 - 20) / 100,
      comp_symmetry: ((i * 29) % 80) / 100, comp_clutter: clutter,
      comp_lines: i % 5, comp_subject_share: 0.18 + ((i * 13) % 45) / 100,
    });
    dets.push({
      id: 5000 + i, image_id: id, cls: 'car', x1: (cx - s / 2) * fw, y1: 0.38 * fh, x2: (cx + s / 2) * fw, y2: 0.74 * fh,
      sharpness: rating, panning: i % 3 === 0 ? 1 : 0, number: read ? no : null, plate: read ? plate : null,
      cull_reason: rating < 0.6 ? 'Motion blur' : null, rating, rating_verdict: rating >= 0.8 ? 'good' : rating >= 0.6 ? 'fair' : 'poor', sharp_end: i % 3 === 0 ? 'subject' : 'even',
      attributes: JSON.stringify(read ? { make, model, colour, team, race_number: no, plate } : {}), burst_pick: soft ? 0 : i % 5 === 0 ? 1 : 0, region_type: 'vehicle',
      read_at: read ? 1789000000 : null, reviewed: 0,
    });
  }
  const jobs: Job[] = [
    { id: 1, label: 'Challenge Bathurst', root: 'D:\\Photos\\2026\\10\\Challenge Bathurst', status: 'done', total: 1284, done: 1284, created_at: 1789000000 },
    { id: 2, label: 'Island Classic', root: 'D:\\Photos\\2026\\08\\Island Classic', status: 'done', total: 612, done: 612, created_at: 1786000000 },
    { id: 3, label: 'Hillclimb Sprint Day', root: 'D:\\Photos\\2026\\09\\Hillclimb Sprint', status: 'scanning', total: 120, done: 62, created_at: 1789900000 },
    { id: 4, label: 'Summer Twilight Meet', root: 'D:\\Photos\\2026\\07\\Twilight Meet', status: 'stopped', total: 340, done: 118, created_at: 1783000000 },
  ];
  // QA only: ?mock=1&empty seeds no albums so the empty state can be screenshotted.
  if (params().has('empty')) jobs.length = 0;
  // `&profile=<preset id>` is the album's shoot type (so a rodeo run words its
  // flowchart from the rodeo preset), not only the global default.
  const prof = params().get('profile');
  if (prof) for (const job of jobs) job.scan_profile = prof;
  const known: KnownVehicle[] = CARS.map(([make, model, colour, team, race_number, plate]) => ({ plate, make, model, colour, team, race_number }));
  // `&ai=empty|configured|decision|local` seed the two-zone Photo helpers card's
  // screenshot states: nothing set up, cloud key saved, decision panel, local models.
  const ai = params().get('ai');
  const openCodeKey = 'oc_sk_live_7f3a';
  const servicesForAi = () => {
    // QA: `?jev=1` seeds OpenCode enabled with its JEV decision model picked —
    // the exact state that used to read "not installed" while moments were
    // actively judged by the cloud service. Pair with `&decider=install` to see
    // a not-installed local model feeding the DECIDER node.
    if (params().get('jev') === '1') return {
      vlm_services: [{ id: 'vlm-opencode', provider: 'opencode', model: 'glm-5.3-flash', host: '', api_key: openCodeKey, enabled: true }],
      moments_services: [{ id: 'moments-opencode', provider: 'opencode', model: 'jev-1.13', host: '', api_key: openCodeKey, enabled: true }],
      use_decider: false,
    };
    // `&svc=opencode,gemini` (or `all`) seeds the AI services cards directly,
    // so each "service added" screenshot opens straight from a URL.
    const svcParam = params().get('svc');
    if (svcParam) {
      const types = svcParam === 'all' ? aiCard.ADD_ORDER : svcParam.split(',').filter((t) => aiCard.ADD_ORDER.includes(t));
      let s: any = { vlm_services: [], moments_services: [], use_decider: false };
      types.forEach((t, i) => {
        s = aiCard.addService(s, t, {
          hostIndex: i + 1,
          id: t === 'ollama' ? `ollama-${i + 1}` : undefined,
          url: t === 'ollama' ? 'http://localhost:11434' : undefined,
        });
      });
      const think = params().get('aithink') === '1';
      s.vlm_services = s.vlm_services.map((r: any) => ({ ...r, api_key: r.provider === 'ollama' ? '' : 'sk_mock_key', reasoning: think }));
      s.moments_services = s.moments_services.map((r: any) => ({ ...r, api_key: 'sk_mock_key', reasoning: think }));
      return s;
    }
    if (ai === 'empty') return { vlm_services: [], moments_services: [], use_decider: false };
    if (ai === 'local') return {
      vlm_services: [],
      moments_services: [{ id: 'moments-local', provider: 'in_process', model: 'in_process', host: '', api_key: '', enabled: true }],
      use_decider: true,
    };
    return {
      vlm_services: [{ id: 'vlm-opencode', provider: 'opencode', model: 'glm-5.3-flash', host: '', api_key: openCodeKey, enabled: true }],
      moments_services: [{ id: 'moments-opencode', provider: 'opencode', model: 'jev-1.13', host: '', api_key: openCodeKey, enabled: true }],
      use_decider: false,
    };
  };
  const settings: Settings = {
    scan_profile: params().get('profile') ?? 'motorsport', detect_conf: 0.35, min_box_fraction: 0.04, max_vehicles_per_frame: 8, include_cars: true, include_bikes: true, include_trucks: false,
    sharp_at: 0.8, blurred_below: 0.5, blur_strictness: 'moderate', entry_list: '', auto_reject_below_stars: 0, cull_blurred: true, burst_gap: 1.5, read_plates: true, read_numbers: true, read_text: true,
    use_vlm: true, vlm_provider: 'ollama', vlm_model: 'qwen2.5vl:7b', vlm_host: 'http://localhost:11434', vlm_extra_hosts: '', vlm_api_key: '', vlm_timeout: 120,
    ...(servicesForAi() as unknown as Settings),
    // `&aidl=installed` switches the local decision model on, so an installed
    // and active model shows its switch on (the seeded, downloaded state).
    ...(aidl === 'installed' ? { use_decider: true } : {}),
    // Moment + vision wiring the two-zone card writes back to the engine.
    moments_strategy: 'round_robin', moments_vlm_fallback: true, moments_threshold: 0.85,
    vlm_strategy: 'round_robin', vlm_reasoning: false, use_quality_score: true, include_people: true,
    workflow_mode: 'both', identify_scope: 'keepers',
    // B5: the per-stage switches / concurrency seats B1 + B3 added.
    identify_ocr: true, identify_vision_workers: 2, identify_decision_workers: 1,
    use_known_vehicles: true, normalise_names: true, write_rating: true, write_label: true, overwrite_rating: false, overwrite_label: false, write_caption: true,
    overwrite_caption: false, write_plate_keyword: true, write_sidecar_for_raw: true, keyword_prefix: 'conrod', decider_quant: 'auto', close_to_tray: false,
  };
  return { jobs, frames, dets, known, settings, ticks: 0, identifySkipped: false };
}

/* ── ingest simulator (see mockIngest.ts): one timeline feeds both the scan and the identify event streams ── */
const listeners = new Set<(p: ScanProgress) => void>();
const identifyListeners = new Set<(p: IdentifyLive) => void>();
let lastProgress: ScanProgress | null = null;
let lastIdentify: IdentifyLive | null = null;
export function subscribeScanProgress(cb: (p: ScanProgress) => void): () => void {
  listeners.add(cb);
  if (lastProgress) cb(lastProgress);
  return () => { listeners.delete(cb); };
}
export function subscribeIdentifyProgress(cb: (p: IdentifyLive) => void): () => void {
  identifyListeners.add(cb);
  if (lastIdentify) cb(lastIdentify);
  return () => { identifyListeners.delete(cb); };
}
const deciderListeners = new Set<(p: DeciderLive) => void>();
let lastDecider: DeciderLive | null = null;
let stopDecider: (() => void) | null = null;
/** The decision model's feed: starts the simulator with the first listener (see mockDecider.ts). */
export function subscribeDeciderProgress(cb: (p: DeciderLive) => void): () => void {
  deciderListeners.add(cb);
  if (lastDecider) cb(lastDecider);
  stopDecider ??= startDeciderSim((p) => { lastDecider = p; for (const fn of deciderListeners) fn(p); }, (k) => mockThumb(k));
  return () => { deciderListeners.delete(cb); };
}
const emitScan = (p: ScanProgress) => { lastProgress = p; for (const cb of listeners) cb(p); };
const emitIdentify = (p: IdentifyLive) => { lastIdentify = p; for (const cb of identifyListeners) cb(p); };

const mockThumb = (k: number): string => photo(k);

function identifyItem(k: number, stage: IdentifyStage, startedAt: number): IdentifyItem {
  const [make, model, colour, team, no, plate] = CARS[k % CARS.length];
  return {
    detection_id: 9000 + k, image_id: 1000 + k, path: `D:\\Photos\\2026\\10\\Challenge Bathurst\\IMG_${4200 + k}.CR3`,
    thumb_path: mockThumb(k), preview_path: null, crop_path: mockThumb(k + 2), stage,
    prompt: 'You are reading one subject. Read the race number from the windscreen or side window first, then the door board. Take the make and model from badges or the body shape, and the team from the livery.',
    reply: { number: no, make, model, colour, team, plate: null },
    trace: [
      `read IMG_${4200 + k}.CR3 · RAW preview 1920 px`,
      `crop subject · car ${model}`,
      `vlm 1 ask(s): ${stage === 'vision' ? 'reading the number and make' : 'queued'}`,
      `entry list: 26 candidates · best match #${no}`,
    ],
    number: no, number_source: k % 5 === 0 ? 'ocr+entry' : 'vlm', number_conf: 0.62 + ((k * 7) % 36) / 100,
    plate, make, model, team, colour,
    action: k % 4 === 0 ? ['dust'] : k % 7 === 0 ? ['airborne', 'tyre_smoke'] : [],
    failures: [], ms: 1200 + ((k * 37) % 900), at: startedAt + (k + 1) * 2,
  };
}

/** `?vision=off` (turned off for the album), `settings`, `service` (no service set up) or `lanes`: the identify page plays without the vision model. */
function visionOffParam(word: string | null) {
  return word === 'off' || word === 'album' ? 'album' : word === 'settings' ? 'settings' : word === 'service' || word === 'no_service' ? 'no_service' : word === 'lanes' ? 'lanes' : undefined;
}
let ingest: IngestSim | undefined;
/** `mode: 'ingest'` plays a whole run on album `jobId`; `'identify'` plays only its vision phase. `?at=` jumps ahead, `?paused` freezes it, `?speed=` hurries it. */
function startIngest(db: Db, jobId: number, mode: 'ingest' | 'identify'): void {
  if (ingest && !ingest.finished) return;
  const p = params();
  const total = Math.max(12, Number(p.get('frames')) || 60);
  const cars = p.has('cars') ? Math.max(0, Number(p.get('cars')) || 0) : 12;
  const job = db.jobs.find((j) => j.id === jobId);
  if (mode === 'ingest' && job) { job.status = 'scanning'; job.total = total; job.done = 0; }
  ingest = startIngestSim({
    thumb: mockThumb, item: identifyItem, onScan: emitScan, onIdentify: emitIdentify, speed: () => Number(params().get('speed')) || 1,
    onEnd: (how) => { if (mode === 'ingest' && job) job.status = how === 'done' ? 'done' : 'stopped'; },
  }, { jobId, mode, total, cars, at: Number(p.get('at')) || 0, paused: p.has('paused'), legacy: p.has('legacy'), visionOff: visionOffParam(p.get('vision')) });
  if (mode === 'ingest' && job && ingest.finished) job.status = 'done';
}
const startSim = (db: Db, jobId: number) => startIngest(db, jobId, 'ingest');
const startIdentifySim = (jobId: number) => startIngest(cache ??= makeDb(), jobId, 'identify');

/** Paint a static grouped-run snapshot + resolved settings for the B5 surface
 *  seeds (§6.8). No simulator runs, so every state stays put for screenshots. */
function surfaceSeed(db: Db): void {
  const name = surfaceName() ?? 'subNodes';
  const greyed = name === 'greyed';
  const base = resolvedSettings();
  if (name === 'serial' || name === 'concurrent') Object.assign(base, bothLocalSettings());
  // Greyed steps are OFF for this album: set the switches AFTER any local-model
  // seed so they stay off (§6.3), with no decision/vision service to turn them on.
  if (greyed) Object.assign(base, {
    identify_ocr: false, use_decider: false, use_vlm: false,
    moments_backend: 'vlm', moments_services: [], vlm_services: [],
  });
  (base as unknown as Record<string, unknown>).identifyVram = name === 'serial' ? VRAM_SERIAL() : VRAM_FITS();
  db.settings = base as unknown as Settings;

  const job = db.jobs.find((j) => j.id === SURFACE_JOB);
  if (job) {
    if (name === 'suggestion') {
      job.scan_profile = 'auto';
      (job as unknown as Record<string, unknown>).shoot_type_suggested = 'sport-rodeo';
      (job as unknown as Record<string, unknown>).shoot_type_confidence = 0.82;
    } else {
      delete (job as unknown as Record<string, unknown>).shoot_type_suggested;
    }
    job.status = 'scanning'; job.total = 100; job.done = 40;
  }
  if (name === 'dropdown') {
    db.jobs = [
      { id: SURFACE_JOB, label: 'Hillclimb Sprint Day', root: '', status: 'scanning', total: 100, done: 40, created_at: 1789900000 },
      { id: 8, label: 'Queued album', root: '', status: 'queued', total: 100, done: 0, created_at: 1789900001 },
      { id: 9, label: 'Finished album', root: '', status: 'done', total: 100, done: 100, created_at: 1789900002 },
    ];
  }

  const overrides: Record<string, Record<string, Record<string, unknown>>> = {
    paused: {
      'identify:ocr': { status: 'done', done: 100, total: 100, workers_total: 0 },
      'identify:decision': { status: 'paused', done: 40, total: 100, workers_total: 2, workers_busy: 0 },
    },
    skipped: {
      'identify:ocr': { status: 'done', done: 100, total: 100, workers_total: 0 },
      'identify:decision': { status: 'done', done: 100, total: 100, workers_total: 0 },
      'identify:vision': { status: 'skipped', done: 0, total: 0, workers_total: 0 },
    },
    greyed: {
      'identify:ocr': { status: 'skipped', done: 0, total: 0, workers_total: 0 },
      'identify:decision': { status: 'skipped', done: 0, total: 0, workers_total: 0 },
      'identify:vision': { status: 'skipped', done: 0, total: 0, workers_total: 0 },
    },
    failed: {
      'identify:ocr': { status: 'done', done: 100, total: 100, workers_total: 0 },
      'identify:decision': { status: 'done', done: 100, total: 100, workers_total: 0 },
      'identify:vision': { status: 'failed', done: 3, total: 10, workers_total: 1, error: 'The vision service stopped answering.' },
    },
  };
  const stages = nineStages(overrides[name] ?? {});
  const workers: PipelineWorker[] = name === 'stalled'
    ? [{ stage: 'identify:vision', worker_index: 0, lane: 0, state: 'working', waiting_for: null, file: 'IMG_4201.CR3', step: 'vision', ms_in_step: 60000, last_report_at: 1000, current: null }]
    : [{ stage: 'read', worker_index: 0, lane: 0, state: 'working', waiting_for: null, file: 'IMG_4200.CR3', step: 'read', ms_in_step: 200, last_report_at: Date.now(), current: null }];
  lastProgress = {
    job_id: SURFACE_JOB, state: 'running',
    // Greyed steps are skipped: the run has moved on to Group, not parked on them.
    stage: name === 'greyed' ? 'group' : name === 'paused' ? 'identify:decision' : name === 'failed' ? 'identify:vision' : 'identify:ocr',
    stage_index: 4, stage_count: 9, done: 40, total: 100, rate_per_s: 1.2, eta_s: 50,
    recent: [], found: { subjects: 40, soft: 4, closed_eyes: 1, numbers: 8, plates: 6, bursts: 18, keepers: 12 },
    pipeline: { stages, workers },
    overall: { done: 40, total: 100, fraction: 0.4, rate_per_s: 1.2, eta_s: 50, eta_confident: false },
  };
}

function status(db: Db): Status {
  // B5 surface seeds: a static grouped run, one album active (or a live queue).
  if (surfaceName()) {
    const name = surfaceName();
    return {
      tasks: [{ id: 9, label: `Scanning ${db.jobs.find((j) => j.id === SURFACE_JOB)?.label ?? 'album'}`, detail: '', state: 'running', done: 40, total: 100, eta: null, error: null }],
      log: ['surface seed'], activeJob: SURFACE_JOB, operations: [], queue: name === 'dropdown' ? [8] : [],
    };
  }
  if (ingest && !ingest.finished) {
    const info = ingest.info(), job = db.jobs.find((j) => j.id === ingest?.jobId);
    const state = info.paused ? 'paused' : 'running';
    if (ingest.mode === 'identify' || info.phase === 'vision') {
      return {
        tasks: [{ id: 11, label: `Identifying · album ${ingest.jobId}`, detail: lastIdentify?.current?.path ?? 'Preparing…', state, done: info.done, total: info.total, eta: null, error: null }],
        log: [`Reading ${job?.label ?? 'album'} subjects`], activeJob: ingest.mode === 'ingest' ? ingest.jobId : null, operations: [`Identifying:${ingest.jobId}`],
      };
    }
    return { tasks: [{ id: 9, label: `Scanning ${job?.label ?? 'album'}`, detail: info.detail, state, done: info.done, total: info.total, eta: lastProgress?.eta_s ?? null, error: null }], log: ['scan started'], activeJob: ingest.jobId, operations: [] };
  }
  if (params().has('identify') && !db.identifySkipped) {
    db.ticks += 1;
    const done = Math.min(1458, 118 + db.ticks * 7);
    return { tasks: [{ id: 3, label: 'Identifying · album 1', detail: `IMG_${5289 + db.ticks}.CR3 · motorcycle`, state: 'running', done, total: 1458, eta: 420, error: null }], log: [`Reading vehicle ${done}`], activeJob: null, operations: ['Identifying:1'] };
  }
  if (!params().has('scan')) return { tasks: [{ id: 1, label: 'Identify Challenge Bathurst', detail: 'Done', state: 'done', done: 300, total: 300, eta: null, error: null }], log: ['12:02:11 scan finished', '12:04:40 identified 300 subjects'], activeJob: null, operations: [] };
  db.ticks += 1;
  const done = Math.min(120, 62 + db.ticks);
  return {
    tasks: [{ id: 2, label: 'Scanning Hillclimb Sprint Day', detail: `IMG_${3900 + done}.CR3`, state: 'running', done, total: 120, eta: (120 - done) * 1.4, error: null },
      { id: 1, label: 'Identify Challenge Bathurst', detail: 'Done', state: 'done', done: 300, total: 300, eta: null, error: null }],
    log: ['12:31:02 indexing 120 frames', `12:31:${20 + db.ticks} measured IMG_${3900 + done}.CR3`], activeJob: 3, operations: [],
  };
}

export async function mockCall<T>(action: string, args: unknown): Promise<T> {
  const db = (cache ??= makeDb());
  const a = (args ?? {}) as Record<string, unknown>;
  const out = (v: unknown) => v as T;
  switch (action) {
    case 'watch_status': return out({active: false, jobId: null});
    case 'set_watch': return out(a);
    case 'cover': return out({ path: photo((Number(a.jobId) || 1) - 1) });
    case 'models_status': {
      // `&aidl=downloading` opens mid-install; `&aidl=failed` opens on a
      // plain disk-space refusal (the state a real failed install leaves).
      if (!dlSeedStarted) {
        dlSeedStarted = true;
        if (aidl === 'downloading') startMockInstall('builtin-vision', { slow: true });
        if (aidl === 'failed') {
          mockInstall = {
            id: 'builtin-vision', done: Math.round(VISION_TOTAL * 0.3), total: VISION_TOTAL,
            error: 'Not enough disk space for the model: it needs 5.8 GB free, but only 66 MB is left. Free some space and try again.',
          };
        }
      }
      return out({ rows: localRows(), install: mockInstall });
    }
    case 'health': {
      // `&health=real` seeds the same row shape the engine sends (one row per
      // VLM service with file:'vlm', plus the opt-in models) so the activity
      // popover can be screenshotted in its real, split-section state.
      if (params().get('health') === 'real') return out([
        { name: 'Detector', file: 'det.onnx', ready: true },
        { name: 'Text (OCR)', file: 'ocr', ready: true },
        { id: 'vlm-opencode', name: 'OpenCode (pay-as-you-go credits)', file: 'vlm', ready: true, detail: 'glm-5.3-flash available' },
        { id: 'vlm-local', name: 'in_process', file: 'vlm', ready: false, detail: 'Ollama unavailable' },
        { id: 'decider', name: 'Decision model', file: 'decider-flash-v1.2-q8_0.gguf', ready: true, downloaded: true, optional: true, detail: 'Ready' },
        { id: 'quality', name: 'Quality', file: 'nima-mobilenet-technical.onnx', ready: false, downloaded: false, optional: true, available: false, detail: 'Not available yet' },
      ]);
      // Local rows plus one row per configured VLM service (file:'vlm'). The
      // built-in card reads models_status, never this list.
      const services = (db.settings.vlm_services ?? []) as unknown as { id?: string; provider?: string; enabled?: boolean; model?: string }[];
      const serviceRows = services.filter((s) => s.enabled).map((s) => ({
        id: s.id ?? `vlm-${s.provider}`, name: `${s.provider ?? 'vision'} (${s.model ?? 'model'})`, file: 'vlm', ready: true, detail: `${s.model ?? ''} available`,
      }));
      return out([...localRows(), ...serviceRows]);
    }
    case 'decider_status': return out(mockDeciderStatus());
    case 'install_decider':
      startMockInstall('decider', { fail: aidl === 'failed' });
      return out({ operation: 'Installing decision model' });
    case 'install_models': {
      const ids = (a.ids as string[]) ?? [];
      // Exactly the clicked model's assets; the Setup card's empty list means
      // everything missing (here: the built-in vision pair).
      if (!ids.length || ids.includes('Qwen3VL-8B-Instruct-Q4_K_M.gguf')) {
        startMockInstall('builtin-vision', { fail: aidl === 'failed' });
      }
      return out({ operation: 'Installing models' });
    }
    case 'cache_info': return out({total: {files: 64, bytes: 3200000}});
    case 'check_update': return out({ok: true, current: '0.0.0-mock', latest: '0.0.1-mock', newer: true, installable: true});
    case 'ollama_models': return out(keyFails()
      ? { ok: false, host: 'http://localhost:11434', online: false, models: [], error: 'Could not connect to Ollama: connection refused (the host is offline).' }
      : {
        ok: true,
        host: 'http://127.0.0.1:11434',
        online: true,
        models: [
          { name: 'qwen2.5vl:7b', size: 5969245856, vision: true, parameter_size: '8.3B' },
          { name: 'qwen3-vl:8b', size: 6140415879, vision: true, parameter_size: '8.8B' },
          { name: 'gemma4:12b', size: 7556508396, vision: true, parameter_size: '11.9B' },
          { name: 'minicpm-v:8b', size: 5473838466, vision: true, parameter_size: '7.6B' },
        ],
      });
    case 'provider_models': {
      const provider = String(a.provider ?? '');
      if (keyFails()) return out({ models: [], error: 'The service could not list models: the key was rejected.' });
      const type = provider === 'opencode-go' ? 'opencode' : provider;
      const def = aiCard.SERVICE_TYPES[type];
      if (!def) return out({ models: [], error: `'${provider}' does not list models; type the model name` });
      // OpenCode's decisions come from decision_models, so its key list is vision only.
      const ids = type === 'opencode'
        ? def.models.filter((m) => m.img && m.role === 'vision').map((m) => m.id)
        : def.models.filter((m) => m.img).map((m) => m.id);
      return out({ models: ids.map((id) => ({ id, name: id })) });
    }
    case 'decision_models': {
      const provider = String(a.provider ?? '');
      if (provider === 'openrouter') {
        // Backend-owned enumeration, same shape as decision_models:
        // { id/model, sees_photos, note? } — rendered through the same path.
        const ms = aiCard.decisionModels('openrouter').map((m) => ({ id: m.id, name: m.id, sees_photos: true, image: true, unverified: false }));
        return out({ models: ms, freeText: false, note: null });
      }
      const type = provider === 'opencode-go' ? 'opencode' : provider;
      if (type === 'opencode') {
        const ms = aiCard.decisionModels('opencode').map((m) => ({ id: m.id, name: m.id, image: true, unverified: false }));
        return out({ models: ms, freeText: false, note: null });
      }
      return out({ models: [], freeText: true, error: `'${provider}' does not list decision models` });
    }
    case 'openrouter_check': return out(keyFails()
      ? { ok: false, error: 'OpenRouter rejected the key (401). Check the key in your account.' }
      : { ok: true, label: 'Conrod dev key', limit: null, usage: null });
    case 'openCodeCheck': return out(keyFails()
      ? { ok: false, error: 'OpenCode could not reach the Console (network unreachable).' }
      : { ok: true, label: 'OpenCode Console (JEV enabled)' });
    case 'summary': return out({images: {scanned: db.frames.length, written: 0, errors: 0}, counts: {numbered: 48, plated: 48, to_review: 12}});
    case 'shoot_stats': {
      const frames = db.frames.length;
      const subjects = frames * 4;
      const sharp = Math.round(subjects * 0.42);
      const soft = Math.round(subjects * 0.48);
      const from = Date.UTC(2026, 8, 8, 9, 12) / 1000;
      return out({
        frames, copies_hidden: 0, size_bytes: frames * 24_300_000, backfilled: 0,
        cameras: [{ name: 'Canon EOS R7', count: frames }],
        lenses: [
          { name: 'EF100-400mm', count: Math.round(frames * 0.7) },
          { name: 'RF24-70mm', count: Math.round(frames * 0.3) },
        ],
        most_used: { camera: 'Canon EOS R7', lens: 'EF100-400mm', f_number: 5.6, shutter: 0.001, iso: 400, focal_length: 400 },
        f_numbers: [{ value: 5.6, count: Math.round(frames * 0.6) }, { value: 4, count: Math.round(frames * 0.4) }],
        shutters: [{ value: 0.001, count: Math.round(frames * 0.7) }, { value: 0.002, count: Math.round(frames * 0.3) }],
        isos: [{ value: 100, count: Math.round(frames * 0.55) }, { value: 1600, count: Math.round(frames * 0.45) }],
        focal_lengths: [{ value: 400, count: frames }],
        pans: Math.round(frames * 0.3), bursts: Math.round(frames / 5),
        blurred: Math.max(0, subjects - sharp - soft), soft, sharp,
        af: { frames, missing: 0, off_subject: Math.round(frames * 0.05), no_subject: 1, soft: 3 },
        keepers: Math.round(frames * 0.25), rejected: 4,
        stars: [{ value: 3, count: Math.round(frames * 0.4) }, { value: 5, count: Math.round(frames * 0.25) }],
        taken_from: from, taken_to: from + 6 * 3600,
        catalog_match: { matched: Math.round(frames * 0.66), compared: frames, percent: 66 },
      } satisfies ShootStats);
    }
    case 'lightroom_export': {
      const name = `challenge-bathurst-${Math.floor(Date.now() / 1000)}`;
      return out({
        package: name,
        photos: db.frames.length,
        path: `C:\\Users\\you\\.conrod\\bridge\\out\\${name}.json`,
      });
    }
    case 'lightroom_pending': {
      if (String(a.dir ?? 'in') !== 'in') return out({ packages: [] });
      return out({
        packages: [
          { name: 'challenge-bathurst-1763600000', album: 'Challenge Bathurst', photos: db.frames.length, age_s: 420 },
          { name: 'island-classic-1763550000', album: 'Island Classic', photos: 37, age_s: 7200 },
        ],
      });
    }
    case 'lightroom_import': {
      const compared = db.frames.length;
      const matched = Math.round(compared * 0.66);
      return out({ matched, unmatched: compared - matched, missing: 9, percent: Math.round((matched / compared) * 100) });
    }
    case 'rename_job': { const job = db.jobs.find(j => j.id === a.jobId); if(job) job.label = String(a.label || 'Album'); return out(null); }
    case 'bulk_edit': { for(const d of db.dets.filter(d => (a.ids as number[]).includes(d.id))) { if('number' in a) d.number = String(a.number); if('reviewed' in a) d.reviewed = a.reviewed ? 1 : 0; } return out({updated: (a.ids as number[]).length}); }
    case 'seed_known': return out({written: db.known.length});
    case 'import_known': return out({written: 1});
    case 'export_known': return out('plate,make,model\nABC123,Ford,Falcon\n');
    case 'import_entries': return out({count: 12});
    case 'worker_limits': {
      const pool = (key: string, current: number, max: number, def: number) => ({ current: Math.min(max, Math.max(1, Math.round(Number(db.settings[key]) || current))), min: 1, max, default: def, live: false });
      return out({
        cores: 12, preview: pool('preview_workers', 4, 8, 4), cull: pool('workers', 6, 10, 6), detect: pool('detect_workers', 2, 4, 1), identify: pool('analysis_workers', 3, 4, 3),
        note: 'Worker counts apply to the next scan; a running pool cannot be resized.',
      } satisfies WorkerLimits);
    }
    case 'bootstrap':
      if (surfaceName()) surfaceSeed(db);
      else {
        if (['scan', 'ingest'].includes(params().get('page') ?? '')) startSim(db, 3);
        if (params().get('page') === 'identify') startIdentifySim(1);
      }
      // `?dev=1` (which also turns the mock on) renders the whole dev chrome.
      // `&devserver=up|down|reconnecting` forces the live-reload chip's state
      // (connected / bundled-UI fallback / retrying) for QA screenshots.
      return out({
        jobs: db.jobs, settings: db.settings, status: status(db), models: localRows(),
        dev: params().has('dev'), devUrl: 'http://localhost:1420', devServer: params().has('dev') && params().get('devserver') !== 'down',
        devHome: '~/.conrod-dev',
      } satisfies Bootstrap);
    case 'status': return out(status(db));
    case 'scan_status': {
      const id = Number(a.jobId) || 0;
      const job = db.jobs.find((j) => j.id === id);
      const st = status(db);
      const waiting = (st.queue ?? []).indexOf(id);
      const pausedScan = Boolean(ingest && !ingest.finished && st.tasks.some((t) => t.state === 'paused'));
      if (st.activeJob === id) {
        return out({ state: 'running', paused: pausedScan, progress: lastProgress?.job_id === id ? lastProgress : null, queue_position: null, errors: 0, error_samples: [], resume_stage: null } satisfies ScanStatus);
      }
      if (waiting >= 0 || job?.status === 'queued') {
        return out({ state: 'queued', paused: false, progress: null, queue_position: waiting >= 0 ? waiting + 1 : null, errors: 0, error_samples: [], resume_stage: null } satisfies ScanStatus);
      }
      if (job?.status === 'scanning') {
        return out({ state: 'running', paused: false, progress: lastProgress?.job_id === id ? lastProgress : null, queue_position: null, errors: 0, error_samples: [], resume_stage: null } satisfies ScanStatus);
      }
      const state = !job ? 'done'
        : job.status === 'done_errors' || job.status === 'error' ? job.status
          : job.status === 'stopped' || job.status === 'incomplete' || job.status === 'indexed' ? 'stopped' : 'done';
      return out({ state, paused: false, progress: null, queue_position: null, errors: job?.errors ?? 0, error_samples: job?.error_samples ?? [], resume_stage: state === 'done' ? 'done' : 'scan' } satisfies ScanStatus);
    }
    case 'jobs': return out(db.jobs);
    case 'global_stats': {
      const albums = db.jobs.length;
      const frames = db.jobs.reduce((n, job) => n + job.total, 0);
      const complete = Math.max(0, albums - 1);
      const shot = complete > 0 ? Math.round(frames * 0.98) : 0;
      const subjects = Math.round(frames * 3.6);
      const sharp = Math.round(subjects * 0.38);
      const soft = Math.round(subjects * 0.58);
      const shooting = frames * 30;
      return out({
        albums, complete_albums: complete, frames, complete_frames: shot,
        copies_hidden: 2437, size_bytes: frames * 24300000,
        cameras: [
          { name: 'Canon EOS R7', count: Math.round(shot * 0.96) },
          { name: 'Canon EOS R5', count: shot - Math.round(shot * 0.96) },
        ],
        lenses: [
          { name: 'RF-S 18-150mm', count: Math.round(shot * 0.52) },
          { name: 'EF100-400mm', count: Math.round(shot * 0.28) },
          { name: 'RF24-70mm', count: Math.round(shot * 0.12) },
          { name: '50mm', count: Math.round(shot * 0.05) },
          { name: '85mm', count: Math.round(shot * 0.02) },
          { name: '24mm', count: Math.round(shot * 0.01) },
        ],
        most_used: { camera: 'Canon EOS R7', lens: 'RF-S 18-150mm', f_number: 6.3, shutter: 0.008, iso: 800, focal_length: 150 },
        f_numbers: [
          { value: 6.3, count: Math.round(shot * 0.42) },
          { value: 5.6, count: Math.round(shot * 0.31) },
          { value: 4, count: Math.round(shot * 0.18) },
        ],
        shutters: [
          { value: 0.001, count: Math.round(shot * 0.46) },
          { value: 0.002, count: Math.round(shot * 0.33) },
        ],
        isos: [
          { value: 100, count: Math.round(shot * 0.5) },
          { value: 400, count: Math.round(shot * 0.3) },
          { value: 1600, count: Math.round(shot * 0.2) },
        ],
        focal_lengths: [
          { value: 18, count: Math.round(shot * 0.2) },
          { value: 150, count: Math.round(shot * 0.55) },
          { value: 400, count: Math.round(shot * 0.25) },
        ],
        pans: Math.round(frames * 0.28), bursts: Math.round(frames / 8),
        blurred: Math.max(0, subjects - sharp - soft), soft, sharp,
        af: { frames, missing: 0, off_subject: Math.round(frames * 0.04), no_subject: Math.round(frames * 0.01), soft: Math.round(frames * 0.02) },
        keepers: Math.round(frames * 0.15), rejected: 3,
        shooting_seconds: shooting,
        average_shoot_seconds: albums > 0 ? Math.round(shooting / albums) : 0,
        longest_shoot_seconds: Math.round(shooting / 5),
        timed_albums: albums,
      } satisfies GlobalStats);
    }
    case 'preset_policy': {
      const given = (a.overrides ?? {}) as Record<string, boolean>;
      const preset = String(a.preset ?? 'mix');
      // An album's own copy of the photographer's defaults, else what the saved profile gives a new album.
      const user = a.userDefaults ? overridesOf(a.userDefaults) : profileDefaults(db.settings.user_profile as never, preset);
      return out(applyOverrides(defaultSummary(preset), overridesOf(given), user));
    }
    case 'set_feature': {
      const job = db.jobs.find((j) => j.id === a.jobId);
      if (!job) throw new Error('No such album');
      const stored = JSON.parse(job.settings_json || '{}') as Record<string, unknown>;
      const next = overridesOf(stored.overrides);
      if (typeof a.value === 'boolean') next[String(a.feature)] = a.value; else delete next[String(a.feature)];
      job.settings_json = JSON.stringify({ ...stored, overrides: next });
      const lanes = applyOverrides(defaultSummary(job.scan_profile ?? 'motorsport'), next, overridesOf(stored.user_defaults));
      return out({ ok: true, jobId: a.jobId, settings: { overrides: next }, operation: null, needsRescan: false, lanes });
    }
    case 'update_job_settings': {
      const job = db.jobs.find((j) => j.id === a.jobId);
      if (job) {
        const patch = (a.patch ?? {}) as Record<string, unknown>;
        const next = { vlm_inherit: 1, ...JSON.parse(job.settings_json || '{}'), ...patch } as Record<string, unknown>;
        if (next.use_vlm === null) delete next.use_vlm; // null hands the choice back to Settings, as the engine does
        job.settings_json = JSON.stringify(next);
        if (typeof patch.scan_profile === 'string') job.scan_profile = patch.scan_profile;
      }
      return out({ ok: true, jobId: a.jobId });
    }
    case 'review': return out({ frames: db.frames, detections: db.dets });
    case 'identified': {
      const rows = db.dets
        .filter((d) => d.number || d.plate || (d.attributes && d.attributes !== '{}'))
        .map((d) => {
          const f = db.frames.find((x) => x.id === d.image_id);
          return {
            id: d.id, image_id: d.image_id, cls: d.cls, region_type: d.region_type,
            crop_path: d.crop_path, number: d.number, plate: d.plate,
            number_conf: d.number ? 0.9 : null, plate_conf: d.plate ? 0.8 : null,
            attributes: d.attributes, quality: d.quality ?? null,
            reviewed: d.reviewed ?? 0, rejected: d.rejected ?? 0,
            frame_path: f?.path ?? '', frame_thumb: f?.thumb_path ?? null,
          };
        });
      return out({ rows: rows.slice(0, 400), total: rows.length, unreviewed: rows.filter((r) => r.reviewed !== 1).length, shown: Math.min(400, rows.length) });
    }
    case 'scan': {
      if (a.jobId == null && typeof a.root === 'string') {
        const id = Math.max(0, ...db.jobs.map((j) => j.id)) + 1;
        const label = String(a.label || '').trim() || a.root.split(/[\\/]/).filter(Boolean).pop() || 'Album';
        db.jobs.unshift({ id, label, root: a.root, status: 'scanning', total: 0, done: 0, created_at: Math.floor(Date.now() / 1000), scan_profile: String(a.profile ?? 'motorsport'), settings_json: JSON.stringify({ overrides: a.overrides ?? {}, user_defaults: profileDefaults(db.settings.user_profile as never, String(a.profile ?? 'motorsport')) }) });
        if (a.stage === 'all') startSim(db, id);
        return out({ jobId: id });
      }
      const jobId = (a.jobId as number | undefined) ?? 3;
      if (a.jobId != null) {
        for (const f of db.frames.filter((f) => !f.rejected)) f.status = 'done';
        const job = db.jobs.find((j) => j.id === jobId);
        if (job) { job.status = 'done'; job.done = job.total; }
      }
      return out({ jobId });
    }
    case 'identify': {
      startIdentifySim(Number(a.jobId) || 1);
      for (const d of db.dets) {
        const f = db.frames.find((f) => f.id === d.image_id);
        if (f && !f.rejected && !d.rejected && !d.bystander && (!d.cull_reason || d.stars != null || f.manual_stars != null)) d.reviewed = 1;
      }
      return out(null);
    }
    case 'composition': return out({ jobId: (a.jobId as number | undefined) ?? null });
    case 'rerun_stage': {
      const jobId = Number(a.jobId) || 1;
      const stage = String(a.stage ?? 'pick');
      // The identify rerun reads only the subjects nobody has read yet.
      if (stage === 'identify') for (const d of db.dets) if (d.read_at == null) d.read_at = Math.floor(Date.now() / 1000);
      return out({ operation: `Rerunning ${stage}:${jobId}` });
    }
    case 'mark': {
      const f = db.frames.find((x) => x.id === a.imageId);
      if (f) {
        if ('stars' in a) f.manual_stars = (a.stars as number | null) ?? null;
        if ('rejected' in a) { f.rejected = a.rejected ? 1 : 0; if (a.rejected) f.picked = 0; }
        if ('picked' in a) { f.picked = a.picked ? 1 : 0; if (a.picked) f.rejected = 0; }
      }
      for (const d of db.dets.filter((d) => d.image_id === a.imageId)) {
        if ('stars' in a) d.stars = (a.stars as number | null) ?? null;
        if ('rejected' in a) { d.rejected = a.rejected ? 1 : 0; if (!a.rejected) d.cull_reason = null; }
      }
      return out(null);
    }
    case 'edit_detection': {
      const d = db.dets.find((x) => x.id === a.detectionId);
      if (d) { const attrs = JSON.parse(d.attributes || '{}') as Record<string, unknown>; const { detectionId, ...rest } = a; void detectionId; d.attributes = JSON.stringify({ ...attrs, ...rest }); }
      return out(null);
    }
    case 'preview': return out(photo(Number(a.imageId) || 0));
    case 'known': return out(db.known);
    case 'save_known': { const v = args as KnownVehicle; db.known = [...db.known.filter((k) => k.plate !== v.plate), v].sort((p, q) => p.plate.localeCompare(q.plate)); return out(null); }
    case 'delete_known': db.known = db.known.filter((k) => k.plate !== a.plate); return out(null);
    case 'delete_all_known': { const removed = db.known.length; db.known = []; return out({removed}); }
    case 'save_settings': db.settings = { ...db.settings, ...(args as Settings) }; return out(db.settings);
    case 'stop': if (ingest && !ingest.finished) ingest.stop(); return out(status(db));
    case 'pause': ingest?.setPaused(true); return out(null);
    case 'resume_scan': ingest?.setPaused(false); return out(null);
    // B2 per-stage controls: mutate the authoritative stage status so the
    // flowchart (built from the snapshot) updates honestly.
    case 'stage_pause': case 'stage_skip': case 'stage_rerun': {
      const stage = String(a.stage ?? '');
      const word = action === 'stage_pause' ? 'paused' : action === 'stage_skip' ? 'skipped' : 'running';
      if (lastProgress) {
        lastProgress = { ...lastProgress, pipeline: { ...lastProgress.pipeline,
          stages: lastProgress.pipeline.stages.map((s) => s.id === stage ? { ...s, status: word } : s) } };
        for (const cb of listeners) cb(lastProgress);
      }
      return out({ ok: true });
    }
    // B1 type suggestion: one click sets the accepted type; the suggestion hides.
    case 'accept_type_suggestion': {
      const job = db.jobs.find((j) => j.id === Number(a.jobId));
      if (job) (job as unknown as Record<string, unknown>).shoot_type_accepted = String(a.type ?? '');
      return out({ ok: true });
    }
    case 'delete_job': db.jobs = db.jobs.filter((j) => j.id !== a.jobId); return out(null);
    case 'cancel_operation': db.identifySkipped = true; if (ingest?.mode === 'identify') ingest.stop(); else ingest?.cancelVision(); return out(null);
    default: return out(null);
  }
}
