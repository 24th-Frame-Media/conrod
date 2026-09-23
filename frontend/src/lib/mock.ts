/* Dev-only engine stand-in: `vite dev` + `?mock=1`. Imported dynamically behind import.meta.env.DEV, so it is not in `dist`.
   Extra params: page=library|scan|review|train|known|settings, viewer, popover, scan (a scan is running). */
import type { Bootstrap, Detection, Frame, Job, KnownVehicle, Page, Settings, Status } from './types';

export type DevStart = { page?: Page; jobId?: number; viewer?: boolean; popover?: boolean };
const params = () => new URLSearchParams(location.search);

export function devStart(): DevStart {
  const pages: Record<string, Page> = { library: 'Library', scan: 'Scan', review: 'Review', train: 'Train', known: 'Known vehicles', settings: 'Settings' };
  const page = pages[params().get('page') ?? ''];
  return { page, jobId: page === 'Review' || page === 'Train' ? 1 : undefined, viewer: params().has('viewer'), popover: params().has('popover') };
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
  for (let i = 0; i < 64; i += 1) {
    const [make, model, colour, team, no, plate, hue] = CARS[i % CARS.length];
    const id = 1000 + i, rating = 0.5 + ((i * 37) % 48) / 100;
    const cx = 0.38 + ((i * 13) % 25) / 100, s = 0.5 + ((i * 7) % 20) / 100;
    const read = i % 4 !== 3;
    frames.push({
      id, path: `D:\\Photos\\2026\\09\\Bathurst 1000\\IMG_${4200 + i}.CR3`, status: i > 60 ? 'pending' : 'done',
      thumb_path: scene(hue, cx, s, colour, 420, 280), preview_path: null, width: W, height: H, rating,
      manual_stars: i % 9 === 4 ? 5 : i % 11 === 6 ? 2 : null, rejected: i % 13 === 8 ? 1 : 0, burst_key: Math.floor(i / 5), error: null,
    });
    dets.push({
      id: 5000 + i, image_id: id, cls: 'car', x1: (cx - s / 2) * W, y1: 0.38 * H, x2: (cx + s / 2) * W, y2: 0.74 * H,
      sharpness: rating, panning: i % 3 === 0 ? 1 : 0, number: read ? no : null, plate: read ? plate : null,
      cull_reason: rating < 0.6 ? `Soft: subject sharpness ${rating.toFixed(2)}` : null,
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
    scan_profile: 'motorsport', detect_conf: 0.35, min_box_fraction: 0.04, max_vehicles_per_frame: 8, include_cars: true, include_bikes: true, include_trucks: false,
    sharp_at: 0.8, blurred_below: 0.5, auto_reject_below_stars: 0, cull_blurred: true, burst_gap: 1.5, read_plates: true, read_numbers: true, read_text: true,
    use_vlm: true, vlm_provider: 'ollama', vlm_model: 'qwen2.5vl:7b', vlm_host: 'http://localhost:11434', vlm_extra_hosts: '', vlm_api_key: '', vlm_timeout: 120,
    use_known_vehicles: true, normalise_names: true, write_rating: true, write_label: true, overwrite_rating: false, overwrite_label: false, write_caption: true,
    overwrite_caption: false, write_plate_keyword: true, write_sidecar_for_raw: true, keyword_prefix: 'conrod', close_to_tray: false,
  };
  return { jobs, frames, dets, known, settings, labels: 14, ticks: 0, identifySkipped: false };
}

function status(db: Db): Status {
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
    case 'check_update': return out({current: '0.1.0', newer: false, installable: false});
    case 'summary': return out({images: {scanned: db.frames.length, written: 0, errors: 0}, counts: {numbered: 48, plated: 48, to_review: 12}});
    case 'rename_job': { const job = db.jobs.find(j => j.id === a.jobId); if(job) job.label = String(a.label || 'Album'); return out(null); }
    case 'bulk_edit': { for(const d of db.dets.filter(d => (a.ids as number[]).includes(d.id))) { if('number' in a) d.number = String(a.number); if('reviewed' in a) d.reviewed = a.reviewed ? 1 : 0; } return out({updated: (a.ids as number[]).length}); }
    case 'seed_known': return out({written: db.known.length});
    case 'import_known': return out({written: 1});
    case 'export_known': return out('plate,make,model\nABC123,Ford,Falcon\n');
    case 'import_entries': return out({count: 12});
    case 'bootstrap':
      return out({ jobs: db.jobs, settings: db.settings, status: status(db), models: [{ name: 'Vehicle detector', file: 'yolo.onnx', ready: true }, { name: 'Plate reader', file: 'plates.onnx', ready: true }, { name: 'Vision model', file: 'qwen', ready: false }] } satisfies Bootstrap);
    case 'status': return out(status(db));
    case 'jobs': return out(db.jobs);
    case 'review': return out({ frames: db.frames, detections: db.dets });
    case 'scan': {
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
      if (f) { if ('stars' in a) f.manual_stars = (a.stars as number | null) ?? null; if ('rejected' in a) f.rejected = a.rejected ? 1 : 0; }
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
    case 'save_settings': db.settings = args as Settings; return out(db.settings);
    case 'delete_job': db.jobs = db.jobs.filter((j) => j.id !== a.jobId); return out(null);
    case 'cancel_operation': db.identifySkipped = true; return out(null);
    default: return out(null);
  }
}
