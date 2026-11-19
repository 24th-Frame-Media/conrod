/* Dev-only engine stand-in: `vite dev` + `?mock=1`. Imported dynamically behind import.meta.env.DEV, so it is not in `dist`.
   Extra params: page=library|scan|ingest|identify|review|train|known|settings, viewer, popover, scan (a scan is running), profile=<shoot preset id>. */
import { startIngestSim, type IngestSim } from './mockIngest';
import type { ShootStats } from './shot.mjs';
import type { Bootstrap, Detection, Frame, GlobalStats, IdentifyItem, IdentifyLive, IdentifyStage, Job, KnownVehicle, Page, ScanProgress, Settings, Status, WorkerLimits } from './types';

export type DevStart = {
  page?: Page; jobId?: number; viewer?: boolean; popover?: boolean;
  /** `?lightroom=sent|export|import|imported|stats` opens the sheet or dialog a screenshot needs. */
  lightroom?: 'export' | 'sent' | 'import' | 'imported' | 'stats';
};
const params = () => new URLSearchParams(location.search);

/** `?mock=1&page=ingest` (or `page=scan`) plays a ~80 s ingest of album 3: scan lanes, group, pick, vision, done. `page=identify` plays only the vision phase.
    Tuning: `&speed=4` hurries it, `&at=30` starts 30 s in (scan 0-61, group 61-64, pick 64-67, vision 67-89, done after 90), `&paused` starts it paused, `&frames=` `&cars=` size it, `&theme=light`. */
export function devStart(): DevStart {
  const pages: Record<string, Page> = { library: 'Library', scan: 'Scan', ingest: 'Scan', identify: 'Identify', review: 'Review', train: 'Train', known: 'Known vehicles', settings: 'Settings' };
  const page = pages[params().get('page') ?? ''];
  const lightroom = params().get('lightroom');
  return {
    page, jobId: page === 'Review' || page === 'Train' || page === 'Identify' ? 1 : page === 'Scan' ? 3 : undefined, viewer: params().has('viewer'), popover: params().has('popover'),
    lightroom: lightroom === 'export' || lightroom === 'sent' || lightroom === 'import' || lightroom === 'imported' || lightroom === 'stats' ? lightroom : undefined,
  };
}

const CARS: [string, string, string, string, string, string, number][] = [
  ['Ford', 'Mustang GT', 'red', 'Dick Johnson Racing', '17', 'DJR17', 8],
  ['Chevrolet', 'Camaro ZL1', 'dodgerblue', 'Triple Eight', '88', 'TE88Z', 215],
  ['Toyota', 'GR Supra', 'white', 'Team Toyota', '5', 'GR5FST', 30],
  ['Porsche', '911 GT3 R', 'gold', 'Manthey', '911', 'MNT911', 45],
  ['Mazda', 'MX-5', 'silver', 'Independent', '23', 'MZD23', 200],
  ['BMW', 'M4 GT3', 'darkorange', 'Walkinshaw', '7', 'BMW777', 280],
];
const W = 6000, H = 4000;
const COMP_MODES = ['centre', 'thirds', 'phi'] as const;

/** A landscape with a car in it, as a data URI; car box is x 0.5±s/2, y .40-.74 of the frame. */
function scene(hue: number, cx: number, s: number, paint: string, w: number, h: number): string {
  const x = (cx - s / 2) * w, cw = s * w, y = 0.5 * h, ch = 0.2 * h, r = ch * 0.3;
  const svg = `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 ${w} ${h}'><defs><linearGradient id='g' x1='0' y1='0' x2='0' y2='1'>`
    + `<stop offset='0' stop-color='hsl(${hue} 34% 30%)'/><stop offset='.66' stop-color='hsl(${hue} 22% 14%)'/><stop offset='.66' stop-color='#26262b'/><stop offset='1' stop-color='#111114'/></linearGradient></defs>`
    + `<rect width='${w}' height='${h}' fill='url(#g)'/><rect x='0' y='${h * 0.66}' width='${w}' height='${h * 0.012}' fill='#3a3a42'/>`
    + `<rect x='${x}' y='${y}' width='${cw}' height='${ch}' rx='${r}' fill='${paint}'/>`
    + `<path d='M${x + cw * 0.2} ${y} L${x + cw * 0.3} ${y - ch * 0.5} L${x + cw * 0.72} ${y - ch * 0.5} L${x + cw * 0.86} ${y} Z' fill='${paint}' opacity='.85'/>`
    + `<circle cx='${x + cw * 0.22}' cy='${y + ch}' r='${ch * 0.3}' fill='#0a0a0c'/><circle cx='${x + cw * 0.78}' cy='${y + ch}' r='${ch * 0.3}' fill='#0a0a0c'/></svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

type Db = { jobs: Job[]; frames: Frame[]; dets: Detection[]; known: KnownVehicle[]; settings: Settings; labels: number; ticks: number; identifySkipped: boolean };
let cache: Db | undefined;

function makeDb(): Db {
  const frames: Frame[] = [], dets: Detection[] = [];
  const count = Math.max(8, Number(params().get('frames')) || 64);
  for (let i = 0; i < count; i += 1) {
    const [make, model, colour, team, no, plate, hue] = CARS[i % CARS.length];
    // Burst 2 is all soft (two stars or less) and has no engine pick: the grid must
    // say "No keeper" there instead of proposing the least-bad frame.
    const soft = Math.floor(i / 5) === 2;
    const id = 1000 + i, rating = soft ? 0.6 + ((i * 7) % 6) / 100 : 0.5 + ((i * 37) % 48) / 100;
    const cx = 0.38 + ((i * 13) % 25) / 100, s = 0.5 + ((i * 7) % 20) / 100;
    const read = i % 4 !== 3;
    // Composition: frame 0 opens with a busy background on a pan, frame 3 is the clean pan the strip marks.
    const clutter = i === 3 ? 0.18 : i % 3 === 0 ? 0.85 + ((i * 11) % 9) / 100 : 0.4 + ((i * 17) % 24) / 100;
    frames.push({
      id, path: `D:\\Photos\\2026\\09\\Bathurst 1000\\IMG_${4200 + i}.CR3`, status: i > count - 4 ? 'pending' : 'done',
      thumb_path: scene(hue, cx, s, colour, 420, 280), preview_path: null, width: W, height: H, rating,
      manual_stars: soft ? null : i % 9 === 4 ? 5 : i % 11 === 6 ? 2 : null, rejected: i % 13 === 8 ? 1 : 0, picked: i % 17 === 5 ? 1 : 0, burst_key: Math.floor(i / 5), error: null,
      comp_placement: 0.52 + ((i * 23) % 42) / 100, comp_mode: COMP_MODES[i % 3],
      comp_balance_x: ((i * 13 + 15) % 40 - 20) / 100, comp_balance_y: ((i * 19 + 10) % 40 - 20) / 100,
      comp_symmetry: ((i * 29) % 80) / 100, comp_clutter: clutter,
      comp_lines: i % 5, comp_subject_share: 0.18 + ((i * 13) % 45) / 100,
    });
    dets.push({
      id: 5000 + i, image_id: id, cls: 'car', x1: (cx - s / 2) * W, y1: 0.38 * H, x2: (cx + s / 2) * W, y2: 0.74 * H,
      sharpness: rating, panning: i % 3 === 0 ? 1 : 0, number: read ? no : null, plate: read ? plate : null,
      cull_reason: rating < 0.6 ? 'Motion blur' : null, rating, rating_verdict: rating >= 0.8 ? 'good' : rating >= 0.6 ? 'fair' : 'poor', sharp_end: i % 3 === 0 ? 'subject' : 'even',
      attributes: JSON.stringify(read ? { make, model, colour, team, race_number: no, plate } : {}), burst_pick: soft ? 0 : i % 5 === 0 ? 1 : 0, region_type: 'vehicle',
    });
  }
  const jobs: Job[] = [
    { id: 1, label: 'Bathurst 1000', root: 'D:\\Photos\\2026\\09\\Bathurst 1000', status: 'done', total: 1284, done: 1284, created_at: 1789000000 },
    { id: 2, label: 'Phillip Island GP', root: 'D:\\Photos\\2026\\08\\Phillip Island', status: 'done', total: 612, done: 612, created_at: 1786000000 },
    { id: 3, label: 'Sunday hillclimb', root: 'D:\\Photos\\2026\\09\\Hillclimb', status: 'scanning', total: 120, done: 62, created_at: 1789900000 },
    { id: 4, label: 'Wakefield twilight', root: 'D:\\Photos\\2026\\07\\Wakefield', status: 'stopped', total: 340, done: 118, created_at: 1783000000 },
  ];
  const known: KnownVehicle[] = CARS.map(([make, model, colour, team, race_number, plate]) => ({ plate, make, model, colour, team, race_number }));
  const settings: Settings = {
    scan_profile: params().get('profile') ?? 'motorsport', detect_conf: 0.35, min_box_fraction: 0.04, max_vehicles_per_frame: 8, include_cars: true, include_bikes: true, include_trucks: false,
    sharp_at: 0.8, blurred_below: 0.5, blur_strictness: 'moderate', entry_list: '', auto_reject_below_stars: 0, cull_blurred: true, burst_gap: 1.5, read_plates: true, read_numbers: true, read_text: true,
    use_vlm: true, vlm_provider: 'ollama', vlm_model: 'qwen2.5vl:7b', vlm_host: 'http://localhost:11434', vlm_extra_hosts: '', vlm_api_key: '', vlm_timeout: 120,
    workflow_mode: 'both', identify_scope: 'keepers',
    use_known_vehicles: true, normalise_names: true, write_rating: true, write_label: true, overwrite_rating: false, overwrite_label: false, write_caption: true,
    overwrite_caption: false, write_plate_keyword: true, write_sidecar_for_raw: true, keyword_prefix: 'conrod', close_to_tray: false,
  };
  return { jobs, frames, dets, known, settings, labels: 14, ticks: 0, identifySkipped: false };
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
const emitScan = (p: ScanProgress) => { lastProgress = p; for (const cb of listeners) cb(p); };
const emitIdentify = (p: IdentifyLive) => { lastIdentify = p; for (const cb of identifyListeners) cb(p); };

let thumbs: string[] | undefined;
const mockThumb = (k: number): string => {
  thumbs ??= Array.from({ length: 24 }, (_, i) => { const [, , colour, , , , hue] = CARS[i % CARS.length]; return scene(hue, 0.38 + ((i * 13) % 25) / 100, 0.5 + ((i * 7) % 20) / 100, colour, 420, 280); });
  return thumbs[k % thumbs.length];
};

function identifyItem(k: number, stage: IdentifyStage, startedAt: number): IdentifyItem {
  const [make, model, colour, team, no, plate] = CARS[k % CARS.length];
  return {
    detection_id: 9000 + k, image_id: 1000 + k, path: `D:\\Photos\\2026\\09\\Bathurst 1000\\IMG_${4200 + k}.CR3`,
    thumb_path: mockThumb(k), preview_path: null, crop_path: mockThumb(k + 2), stage,
    prompt: 'You are reading one competition car. Read the race number from the windscreen or side window first, then the door board. Take the make and model from badges or the body shape, and the team from the livery.',
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
  }, { jobId, mode, total, cars, at: Number(p.get('at')) || 0, paused: p.has('paused'), legacy: p.has('legacy') });
  if (mode === 'ingest' && job && ingest.finished) job.status = 'done';
}
const startSim = (db: Db, jobId: number) => startIngest(db, jobId, 'ingest');
const startIdentifySim = (jobId: number) => startIngest(cache ??= makeDb(), jobId, 'identify');

function status(db: Db): Status {
  if (ingest && !ingest.finished) {
    const info = ingest.info(), job = db.jobs.find((j) => j.id === ingest?.jobId);
    const state = info.paused ? 'paused' : 'running';
    if (ingest.mode === 'identify' || info.phase === 'vision') {
      return {
        tasks: [{ id: 11, label: `Identifying · album ${ingest.jobId}`, detail: lastIdentify?.current?.path ?? 'Preparing…', state, done: info.done, total: info.total, eta: null, error: null }],
        log: [`Reading ${job?.label ?? 'album'} vehicles`], activeJob: ingest.mode === 'ingest' ? ingest.jobId : null, operations: [`Identifying:${ingest.jobId}`],
      };
    }
    return { tasks: [{ id: 9, label: `Scanning ${job?.label ?? 'album'}`, detail: info.detail, state, done: info.done, total: info.total, eta: lastProgress?.eta_s ?? null, error: null }], log: ['scan started'], activeJob: ingest.jobId, operations: [] };
  }
  if (params().has('identify') && !db.identifySkipped) {
    db.ticks += 1;
    const done = Math.min(1458, 118 + db.ticks * 7);
    return { tasks: [{ id: 3, label: 'Identifying · album 1', detail: `IMG_${5289 + db.ticks}.CR3 · motorcycle`, state: 'running', done, total: 1458, eta: 420, error: null }], log: [`Reading vehicle ${done}`], activeJob: null, operations: ['Identifying:1'] };
  }
  if (!params().has('scan')) return { tasks: [{ id: 1, label: 'Identify Bathurst 1000', detail: 'Done', state: 'done', done: 300, total: 300, eta: null, error: null }], log: ['12:02:11 scan finished', '12:04:40 identified 300 vehicles'], activeJob: null, operations: [] };
  db.ticks += 1;
  const done = Math.min(120, 62 + db.ticks);
  return {
    tasks: [{ id: 2, label: 'Scanning Sunday hillclimb', detail: `IMG_${3900 + done}.CR3`, state: 'running', done, total: 120, eta: (120 - done) * 1.4, error: null },
      { id: 1, label: 'Identify Bathurst 1000', detail: 'Done', state: 'done', done: 300, total: 300, eta: null, error: null }],
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
    case 'cover': return out({path: db.frames[0]?.thumb_path});
    case 'health': return out([{name: 'Detector', ready: true}, {name: 'Vision model', ready: false, detail: 'Ollama is not running'}]);
    case 'cache_info': return out({total: {files: 64, bytes: 3200000}});
    case 'check_update': return out({ok: true, current: '0.0.0-mock', latest: '0.0.1-mock', newer: true, installable: true});
    case 'ollama_models': return out({
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
      if (provider !== 'openai' && provider !== 'opencode' && provider !== 'opencode-go') {
        return out({ models: [], error: `'${provider}' does not list models; type the model name` });
      }
      const models = provider === 'openai'
        ? [{ id: 'gpt-4o', name: 'gpt-4o' }, { id: 'gpt-4o-mini', name: 'gpt-4o-mini' }, { id: 'o4-mini', name: 'o4-mini' }]
        : [{ id: 'qwen3.8-max', name: 'qwen3.8-max' }, { id: 'deepseek-v4.1-flash', name: 'deepseek-v4.1-flash' }, { id: 'kimi-k2', name: 'kimi-k2' }];
      return out({ models });
    }
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
      const name = `bathurst-1000-${Math.floor(Date.now() / 1000)}`;
      return out({
        package: name,
        photos: db.frames.length,
        path: `C:\\Users\\kapsikkum\\.conrod\\bridge\\out\\${name}.json`,
      });
    }
    case 'lightroom_pending': {
      if (String(a.dir ?? 'in') !== 'in') return out({ packages: [] });
      return out({
        packages: [
          { name: 'bathurst-1000-1763600000', album: 'Bathurst 1000', photos: db.frames.length, age_s: 420 },
          { name: 'phillip-island-gp-1763550000', album: 'Phillip Island GP', photos: 37, age_s: 7200 },
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
      if (['scan', 'ingest'].includes(params().get('page') ?? '')) startSim(db, 3);
      if (params().get('page') === 'identify') startIdentifySim(1);
      return out({ jobs: db.jobs, settings: db.settings, status: status(db), models: [{ name: 'Vehicle detector', file: 'yolo.onnx', ready: true }, { name: 'Plate reader', file: 'plates.onnx', ready: true }, { name: 'Vision model', file: 'qwen', ready: false }] } satisfies Bootstrap);
    case 'status': return out(status(db));
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
    case 'update_job_settings': {
      const job = db.jobs.find((j) => j.id === a.jobId);
      if (job) {
        const patch = (a.patch ?? {}) as Record<string, unknown>;
        job.settings_json = JSON.stringify({ ...JSON.parse(job.settings_json || '{}'), ...patch });
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
        db.jobs.unshift({ id, label, root: a.root, status: 'scanning', total: 0, done: 0, created_at: Math.floor(Date.now() / 1000), scan_profile: String(a.profile ?? 'motorsport') });
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
    case 'preview': return out(scene(20, 0.5, 0.6, 'crimson', 1200, 800));
    case 'known': return out(db.known);
    case 'save_known': { const v = args as KnownVehicle; db.known = [...db.known.filter((k) => k.plate !== v.plate), v].sort((p, q) => p.plate.localeCompare(q.plate)); return out(null); }
    case 'delete_known': db.known = db.known.filter((k) => k.plate !== a.plate); return out(null);
    case 'delete_all_known': { const removed = db.known.length; db.known = []; return out({removed}); }
    case 'training_status': return out({ labels: db.labels, active: false, validation: null });
    case 'train_label': db.labels += 1; return out(null);
    case 'undo_label': db.labels = Math.max(0, db.labels - 1); return out(null);
    case 'train_model': return out({ labels: db.labels, active: true, validation: { exact: 0.62, within_one: 0.93, n: db.labels } });
    case 'forget_model': return out({ labels: db.labels, active: false, validation: null });
    case 'train_taste': return out({ n: 42 });
    case 'save_settings': db.settings = { ...db.settings, ...(args as Settings) }; return out(db.settings);
    case 'stop': if (ingest && !ingest.finished) ingest.stop(); return out(status(db));
    case 'pause': ingest?.setPaused(true); return out(null);
    case 'resume_scan': ingest?.setPaused(false); return out(null);
    case 'delete_job': db.jobs = db.jobs.filter((j) => j.id !== a.jobId); return out(null);
    case 'cancel_operation': db.identifySkipped = true; if (ingest?.mode === 'identify') ingest.stop(); else ingest?.cancelVision(); return out(null);
    default: return out(null);
  }
}
