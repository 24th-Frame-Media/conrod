/* Dev-only engine stand-in: `vite dev` + `?mock=1`. Imported dynamically behind import.meta.env.DEV, so it is not in `dist`.
   Extra params: page=library|scan|review|train|known|settings, viewer, popover, scan (a scan is running), profile=<shoot preset id>. */
import type { Bootstrap, Detection, Frame, Job, KnownVehicle, Page, PipelineStage, PipelineWorker, ScanProgress, ScanRecentFrame, Settings, Status, WorkerLimits } from './types';

export type DevStart = { page?: Page; jobId?: number; viewer?: boolean; popover?: boolean };
const params = () => new URLSearchParams(location.search);

/** `?mock=1&page=scan` plays a ~30 s scan of album 3 through every stage (`&speed=2` to hurry it). */
export function devStart(): DevStart {
  const pages: Record<string, Page> = { library: 'Library', scan: 'Scan', review: 'Review', train: 'Train', known: 'Known vehicles', settings: 'Settings' };
  const page = pages[params().get('page') ?? ''];
  return { page, jobId: page === 'Review' || page === 'Train' ? 1 : page === 'Scan' ? 3 : undefined, viewer: params().has('viewer'), popover: params().has('popover') };
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
    const id = 1000 + i, rating = 0.5 + ((i * 37) % 48) / 100;
    const cx = 0.38 + ((i * 13) % 25) / 100, s = 0.5 + ((i * 7) % 20) / 100;
    const read = i % 4 !== 3;
    frames.push({
      id, path: `D:\\Photos\\2026\\09\\Bathurst 1000\\IMG_${4200 + i}.CR3`, status: i > count - 4 ? 'pending' : 'done',
      thumb_path: scene(hue, cx, s, colour, 420, 280), preview_path: null, width: W, height: H, rating,
      manual_stars: i % 9 === 4 ? 5 : i % 11 === 6 ? 2 : null, rejected: i % 13 === 8 ? 1 : 0, picked: i % 17 === 5 ? 1 : 0, burst_key: Math.floor(i / 5), error: null,
    });
    dets.push({
      id: 5000 + i, image_id: id, cls: 'car', x1: (cx - s / 2) * W, y1: 0.38 * H, x2: (cx + s / 2) * W, y2: 0.74 * H,
      sharpness: rating, panning: i % 3 === 0 ? 1 : 0, number: read ? no : null, plate: read ? plate : null,
      cull_reason: rating < 0.6 ? 'Motion blur' : null, rating, rating_verdict: rating >= 0.8 ? 'good' : rating >= 0.6 ? 'fair' : 'poor', sharp_end: i % 3 === 0 ? 'subject' : 'even',
      attributes: JSON.stringify(read ? { make, model, colour, team, race_number: no, plate } : {}), burst_pick: i % 5 === 0 ? 1 : 0, region_type: 'vehicle',
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
    use_known_vehicles: true, normalise_names: true, write_rating: true, write_label: true, overwrite_rating: false, overwrite_label: false, write_caption: true,
    overwrite_caption: false, write_plate_keyword: true, write_sidecar_for_raw: true, keyword_prefix: 'conrod', close_to_tray: false,
  };
  return { jobs, frames, dets, known, settings, labels: 14, ticks: 0, identifySkipped: false };
}

/* ── scan simulator: a fluid model of the real pipeline, so the scan screen can be walked without the engine ── */
const listeners = new Set<(p: ScanProgress) => void>();
let lastProgress: ScanProgress | null = null;
export function subscribeScanProgress(cb: (p: ScanProgress) => void): () => void {
  listeners.add(cb);
  if (lastProgress) cb(lastProgress);
  return () => { listeners.delete(cb); };
}

type StageDef = { id: string; label: string; model: string; device: string; pool: 'preview' | 'cull' | 'detect' | 'identify' | null; perWorker: number; steps: string[]; stepMs: number };
const STAGE_DEFS: StageDef[] = [
  { id: 'read', label: 'Read', model: 'RAW preview decoder', device: 'CPU', pool: 'preview', perWorker: 20, steps: ['decode preview', 'write thumbnail'], stepMs: 140 },
  { id: 'detect', label: 'Detect', model: 'yolo11s-960.onnx', device: 'DirectML', pool: 'detect', perWorker: 23, steps: ['YOLO inference', 'decode boxes'], stepMs: 300 },
  { id: 'sharpness', label: 'Sharpness', model: 'edge blur · subject crop', device: 'CPU', pool: 'cull', perWorker: 22, steps: ['crop subject', 'edge blur', 'pan check'], stepMs: 120 },
  { id: 'faces', label: 'Faces', model: 'face + eye detector', device: 'CPU', pool: 'cull', perWorker: 23, steps: ['find faces', 'eye state'], stepMs: 150 },
  { id: 'identify', label: 'Identify', model: 'qwen2.5vl:7b via Ollama', device: 'ollama', pool: 'identify', perWorker: 10, steps: ['reading plate', 'vision model', 'race number'], stepMs: 700 },
  { id: 'group', label: 'Group', model: 'EXIF time + similarity', device: 'CPU', pool: null, perWorker: 0, steps: ['grouping bursts'], stepMs: 800 },
  { id: 'pick', label: 'Pick', model: 'keeper ranker', device: 'CPU', pool: null, perWorker: 0, steps: ['ranking keepers'], stepMs: 700 },
];
const ID_RATIO = 0.7;

type Sim = {
  jobId: number; total: number; t: number; last: number; timer: number; finished: boolean;
  done: number[]; rate: number[]; group: number; pick: number; overall: number; overallRate: number;
  size: Record<string, number>; recent: ScanRecentFrame[]; seen: number; bursts: number; keepers: number;
};
let sim: Sim | undefined;
let thumbs: string[] | undefined;
const mockThumb = (k: number): string => {
  thumbs ??= Array.from({ length: 24 }, (_, i) => { const [, , colour, , , , hue] = CARS[i % CARS.length]; return scene(hue, 0.38 + ((i * 13) % 25) / 100, 0.5 + ((i * 7) % 20) / 100, colour, 240, 160); });
  return thumbs[k % thumbs.length];
};

function startSim(db: Db, jobId: number): void {
  if (sim && !sim.finished) return;
  const size = (key: string, fallback: number) => Math.max(1, Math.round(Number(db.settings[key]) || fallback));
  const total = Math.max(60, Number(params().get('frames')) || 1200);
  sim = {
    jobId, total, t: 0, last: performance.now(), timer: 0, finished: false, done: [0, 0, 0, 0, 0], rate: [0, 0, 0, 0, 0], group: 0, pick: 0, overall: 0, overallRate: 0,
    size: { preview: size('preview_workers', 4), cull: size('workers', 6), detect: size('detect_workers', 2), identify: size('analysis_workers', 3), group: 1, pick: 1 }, recent: [], seen: 0, bursts: 0, keepers: 0,
  };
  const job = db.jobs.find((j) => j.id === jobId);
  if (job) { job.status = 'scanning'; job.total = total; job.done = 0; }
  sim.timer = window.setInterval(() => tickSim(db), 250);
  tickSim(db);
}

function tickSim(db: Db): void {
  const s = sim;
  if (!s || s.finished) return;
  const speed = Number(params().get('speed')) || 1;
  const now = performance.now();
  const dt = Math.min(0.5, (now - s.last) / 1000) * speed;
  s.last = now; s.t += dt;
  const idTotal = Math.round(s.total * ID_RATIO), ramp = Math.min(1, 0.2 + s.t / 2.5);
  const totals = [s.total, s.total, s.total, s.total, idTotal];
  const workersOf = (d: StageDef) => (d.pool ? s.size[d.pool] : 1);
  const prevFaces = s.done[3];
  const flowing = [false, false, false, false, false];
  for (let i = 0; i < 5; i += 1) {
    const avail = i === 0 ? s.total : i === 4 ? s.done[3] * ID_RATIO : s.done[i - 1];
    const step = Math.max(0, Math.min(workersOf(STAGE_DEFS[i]) * STAGE_DEFS[i].perWorker * ramp * dt, avail - s.done[i], totals[i] - s.done[i]));
    s.done[i] += step; flowing[i] = step > 0;
    s.rate[i] = s.rate[i] * 0.75 + (dt > 0 ? step / dt : 0) * 0.25;
  }
  const perFrameDone = s.done.every((d, i) => d >= totals[i] - 0.5);
  if (perFrameDone) {
    if (s.group < s.total) { s.group = Math.min(s.total, s.group + (s.total * dt) / 1.6); if (s.group >= s.total) s.bursts = Math.round(s.total / 7.3); }
    else if (s.pick < s.total) { s.pick = Math.min(s.total, s.pick + (s.total * dt) / 1.4); if (s.pick >= s.total) s.keepers = Math.round(s.bursts * 0.7); }
  }
  for (let k = Math.floor(prevFaces); k < Math.floor(s.done[3]); k += 1) {
    const labelled = s.done[4] > 0 && k % 5 < 3, car = CARS[k % CARS.length];
    s.recent.unshift({
      image_id: 1000 + k, thumb_url: mockThumb(k), subject: 'car',
      verdict: k % 5 === 3 ? 'soft' : 'sharp',
      panning: k % 5 !== 3 && k % 7 === 2,
      label: labelled ? (k % 2 ? `#${car[4]}` : car[5]) : null,
    });
  }
  s.recent = s.recent.slice(0, 24);
  const finished = s.pick >= s.total;
  const frac = (s.done.reduce((n, d, i) => n + d / totals[i], 0) + s.group / s.total + s.pick / s.total) / 7;
  const overall = finished ? s.total : Math.floor(frac * s.total);
  s.overallRate = s.overallRate * 0.8 + (dt > 0 ? (overall - s.overall) / dt : 0) * 0.2;
  s.overall = overall;

  const lanes: PipelineWorker[] = [];
  const stages: PipelineStage[] = STAGE_DEFS.map((d, si) => {
    const n = workersOf(d), per = si < 5;
    const stageTotal = per ? totals[si] : s.total;
    const stageDone = per ? s.done[si] : si === 5 ? s.group : s.pick;
    const avail = !per ? s.total : si === 0 ? s.total : si === 4 ? s.done[3] * ID_RATIO : s.done[si - 1];
    const queue = per ? Math.max(0, Math.floor(avail - stageDone)) : 0;
    const phaseOn = !per && perFrameDone && stageDone < s.total && (si === 5 || s.group >= s.total);
    const busyWant = per ? (queue >= 1 ? Math.min(n, queue) : flowing[si] ? 1 : 0) : phaseOn ? 1 : 0;
    let busy = 0;
    for (let w = 0; w < n; w += 1) {
      const idx = Math.floor(stageDone) + w;
      const lane: PipelineWorker = { stage: d.id, worker_index: w, state: 'idle', waiting_for: null, file: null, step: null, ms_in_step: 0 };
      if (w < busyWant) {
        lane.file = `IMG_${4200 + (idx % 3000)}.CR3`;
        lane.step = d.steps[(Math.floor(s.t * 3) + w + si) % d.steps.length];
        lane.ms_in_step = Math.round((s.t * 1000 + w * 173 + si * 61) % d.stepMs);
        if (d.id === 'detect' && w === 1 && Math.floor(s.t * 2) % 5 === 0) { lane.state = 'waiting'; lane.waiting_for = 'gpu'; lane.step = 'queued for DirectML'; lane.ms_in_step += 90; }
        else if (d.id === 'identify' && w === n - 1 && n > 1 && Math.floor(s.t) % 4 === 0) { lane.state = 'waiting'; lane.waiting_for = 'ollama'; lane.step = 'queued for the model'; }
        else { lane.state = 'busy'; busy += 1; }
      }
      lanes.push(lane);
    }
    const rate = per ? s.rate[si] : phaseOn ? s.total / (si === 5 ? 1.6 : 1.4) : 0;
    return { id: d.id, label: d.label, model: d.model, device: d.device, workers_total: n, workers_busy: busy, queue_len: queue, done: Math.floor(stageDone), total: stageTotal, rate_per_s: Math.round(rate * 10) / 10, eta_s: rate > 1 && stageDone < stageTotal ? (stageTotal - stageDone) / rate : null };
  });
  // Like the engine, the header names one stage and carries that stage's own count, rate and ETA.
  const firstOpen = stages.findIndex((st) => st.done < st.total);
  const activeIndex = firstOpen < 0 ? stages.length - 1 : firstOpen, head = stages[activeIndex];
  const found = {
    subjects: Math.floor(s.done[1] * 1.15), soft: Math.floor(s.done[2] * 0.21), closed_eyes: Math.floor(s.done[3] * 0.01),
    numbers: Math.floor(s.done[4] * 0.8), plates: Math.floor(s.done[4] * 0.6), bursts: s.bursts, keepers: s.keepers,
  };
  const overallEta = finished ? null : (s.overallRate > 0 ? Math.max(0, s.total - s.overall) / s.overallRate : null);
  lastProgress = {
    job_id: s.jobId, state: finished ? 'done' : 'running', stage: head.id, stage_index: activeIndex, stage_count: stages.length,
    done: head.done, total: head.total, rate_per_s: head.rate_per_s, eta_s: finished ? null : head.eta_s, recent: s.recent, found, pipeline: { stages, workers: lanes },
    overall: { done: s.overall, total: s.total, fraction: finished ? 1 : frac, rate_per_s: s.overallRate, eta_s: overallEta, eta_confident: finished || s.done[1] >= 20 },
  };
  const job = db.jobs.find((j) => j.id === s.jobId);
  if (job) { job.done = overall; if (finished) job.status = 'done'; }
  if (finished) { s.finished = true; clearInterval(s.timer); }
  for (const cb of listeners) cb(lastProgress);
}

function status(db: Db): Status {
  if (sim && !sim.finished) {
    const job = db.jobs.find((j) => j.id === sim?.jobId);
    return { tasks: [{ id: 9, label: `Scanning ${job?.label ?? 'album'}`, detail: `IMG_${4200 + sim.overall}.CR3`, state: 'running', done: sim.overall, total: sim.total, eta: lastProgress?.eta_s ?? null, error: null }], log: ['scan started'], activeJob: sim.jobId, operations: [] };
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
    case 'summary': return out({images: {scanned: db.frames.length, written: 0, errors: 0}, counts: {numbered: 48, plated: 48, to_review: 12}});
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
      if (params().get('page') === 'scan') startSim(db, 3);
      return out({ jobs: db.jobs, settings: db.settings, status: status(db), models: [{ name: 'Vehicle detector', file: 'yolo.onnx', ready: true }, { name: 'Plate reader', file: 'plates.onnx', ready: true }, { name: 'Vision model', file: 'qwen', ready: false }] } satisfies Bootstrap);
    case 'status': return out(status(db));
    case 'jobs': return out(db.jobs);
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
      for (const d of db.dets) {
        const f = db.frames.find((f) => f.id === d.image_id);
        if (f && !f.rejected && !d.rejected && !d.bystander && (!d.cull_reason || d.stars != null || f.manual_stars != null)) d.reviewed = 1;
      }
      return out(null);
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
    case 'stop': if (sim && !sim.finished) { sim.finished = true; clearInterval(sim.timer); if (lastProgress) { lastProgress = { ...lastProgress, state: 'stopped', eta_s: null }; for (const cb of listeners) cb(lastProgress); } const job = db.jobs.find((j) => j.id === sim?.jobId); if (job) job.status = 'stopped'; } return out(status(db));
    case 'delete_job': db.jobs = db.jobs.filter((j) => j.id !== a.jobId); return out(null);
    case 'cancel_operation': db.identifySkipped = true; return out(null);
    default: return out(null);
  }
}
