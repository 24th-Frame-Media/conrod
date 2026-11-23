/* Dev-only: the ingest simulator behind `?mock=1&page=ingest`. A timeline of virtual seconds drives six scan lanes through
   read, decode, detect, sharpness, faces and persist (boxes, autofocus and a rating appear as the steps finish), then group, pick
   and three vision workers, then done. The same code plays an identify-only run. It fills the optional fields of
   docs/plans/ingest-live-contract.md, so it is also the living example of that contract. */
import { SCAN_STEPS, stageOfStep } from './ingest.mjs';
import type { AfKind, IdentifyItem, IdentifyLive, IdentifyStage, LaneBox, LaneFrame, LaneStep, PipelineStage, PipelineWorker, ScanProgress, ScanRecentFrame } from './types';

export type IngestDeps = {
  thumb: (k: number) => string;
  /** One car for the vision workers; `stage` is where the model is in it. */
  item: (k: number, stage: IdentifyStage, startedAt: number) => IdentifyItem;
  onScan: (p: ScanProgress) => void;
  onIdentify: (p: IdentifyLive) => void;
  /** Called once when the run is over. */
  onEnd: (how: 'done' | 'stopped') => void;
  speed: () => number;
};
export type IngestOptions = { jobId: number; mode: 'ingest' | 'identify'; total: number; cars: number; at: number; paused: boolean; /** Send only what today's engine sends: no lane frames, no lane ids, no per-frame details. */ legacy?: boolean; /** Play the run with the vision model off, for this reason (?vision=off). */ visionOff?: 'album' | 'settings' | 'no_service' | 'lanes' };
export type IngestInfo = { phase: 'scan' | 'group' | 'pick' | 'vision' | 'done' | 'stopped'; done: number; total: number; paused: boolean; detail: string; eta: number | null };
export type IngestSim = {
  jobId: number; mode: 'ingest' | 'identify'; finished: boolean;
  /** Stop the whole run (the Cancel scan button, or Stop identification in an identify-only run). */
  stop: () => void;
  /** Stop only the vision phase; the scan itself stays done. */
  cancelVision: () => void;
  setPaused: (v: boolean) => void;
  info: () => IngestInfo;
};

const LANES = 6, DETECTORS = 2, FACE_SLOTS = 1, VISION_WORKERS = 3;
const GROUP_S = 3, PICK_S = 3, CAR_S = 5;
const DUR: Record<LaneStep, number> = { read: 0.5, decode: 0.6, detect: 1.2, sharpness: 0.8, faces: 0.9, persist: 0.3 };
const STEP_WORDS: Record<LaneStep, string> = { read: 'read RAW', decode: 'decode preview', detect: 'YOLO inference', sharpness: 'edge blur', faces: 'find faces', persist: 'write thumbnail' };
const STAGE_META = {
  read: { label: 'Read', model: 'RAW preview decoder', device: 'CPU', workers: LANES },
  detect: { label: 'Detect', model: 'yolo11s-960.onnx', device: 'DirectML', workers: DETECTORS },
  sharpness: { label: 'Sharpness', model: 'edge blur · subject crop', device: 'CPU', workers: LANES },
  faces: { label: 'Faces', model: 'face + eye detector', device: 'CPU', workers: FACE_SLOTS },
} as const;
const VISION_MODEL = 'qwen2.5vl:7b';
const clamp01 = (v: number) => Math.max(0, Math.min(1, v));
const name = (k: number) => `IMG_${4200 + k}.CR3`;

type Lane = { id: number; k: number | null; step: LaneStep; start: number; waiting: 'detector' | 'faces' | null };
type Phase = IngestInfo['phase'];

const dur = (k: number, step: LaneStep) => DUR[step] * (0.8 + ((k * 7) % 9) / 20);
/** The car's place in the mock picture (the mock draws frame k as picture k % 24). */
const carBox = (k: number): [number, number, number, number] => {
  const i = k % 24, cx = 0.38 + ((i * 13) % 25) / 100, s = 0.5 + ((i * 7) % 20) / 100;
  return [cx - s / 2, 0.39, cx + s / 2, 0.77];
};
const frameBoxes = (k: number): LaneBox[] => {
  const x2 = carBox(k)[2];
  const boxes: LaneBox[] = [{ cls: 'car', conf: 0.78 + ((k * 11) % 21) / 100, box: carBox(k) }];
  if (k % 5 === 0) boxes.push({ cls: 'person', conf: 0.46 + ((k * 3) % 12) / 100, box: [Math.min(0.9, x2 + 0.03), 0.52, Math.min(0.97, x2 + 0.09), 0.76] });
  return boxes;
};
const frameAf = (k: number): [number, number, number, number] | undefined => {
  if (k % 6 === 5) return undefined;
  const [x1, , x2] = carBox(k), cx = x1 + (x2 - x1) * 0.32;
  return [cx - 0.06, 0.47, cx + 0.06, 0.58];
};
const frameAfKind = (k: number): AfKind => (k % 9 === 4 ? 'grid' : 'point');
const verdictOf = (k: number): 'sharp' | 'soft' | 'blurred' => (k % 11 === 7 ? 'blurred' : k % 5 === 3 ? 'soft' : 'sharp');
const starsOf = (k: number) => { const v = verdictOf(k); return v === 'blurred' ? 1 : v === 'soft' ? 2 : 3 + (k % 3); };
const panOf = (k: number) => verdictOf(k) !== 'blurred' && k % 7 === 2;

/** The scan payload of an engine that does not know the optional fields of docs/plans/ingest-live-contract.md. */
function degrade(p: ScanProgress): ScanProgress {
  return {
    ...p,
    recent: p.recent.map(({ file, stars, boxes, af, af_kind, ...rest }) => { void file; void stars; void boxes; void af; void af_kind; return rest; }),
    pipeline: {
      stages: p.pipeline.stages.map(({ errors, ...s }) => { void errors; return s; }),
      workers: p.pipeline.workers.map(({ lane, current, ...w }) => { void lane; void current; return { ...w, state: w.state === 'paused' ? 'busy' as const : w.state }; }),
    },
  };
}

export function startIngestSim(deps: IngestDeps, opts: IngestOptions): IngestSim {
  const { jobId, mode, total, cars } = opts;
  const sendScan = (p: ScanProgress) => deps.onScan(opts.legacy ? degrade(p) : p);
  const lanes: Lane[] = Array.from({ length: LANES }, (_, id) => ({ id, k: null, step: 'read', start: 0, waiting: null }));
  const recent: ScanRecentFrame[] = [];
  let t = 0, next = 0, finished = 0, rate = 0, paused = opts.paused, groupT = 0, pickT = 0, visionT = 0;
  let phase: Phase = mode === 'identify' ? 'vision' : 'scan';
  let over = false, last = performance.now(), timer = 0;
  const startedAt = Date.now();

  const slotFree = (step: LaneStep) => lanes.filter((l) => l.k != null && l.step === step && !l.waiting).length < (step === 'detect' ? DETECTORS : FACE_SLOTS);
  const enter = (lane: Lane, step: LaneStep) => {
    lane.step = step; lane.start = t; lane.waiting = null;
    if (step === 'detect' && !slotFree('detect')) lane.waiting = 'detector';
    if (step === 'faces' && !slotFree('faces')) lane.waiting = 'faces';
  };
  const finish = (lane: Lane) => {
    const k = lane.k!;
    recent.unshift({
      image_id: 1000 + k, file: name(k), thumb_url: deps.thumb(k), subject: 'car', verdict: verdictOf(k), panning: panOf(k),
      stars: starsOf(k), boxes: frameBoxes(k), af: frameAf(k), af_kind: frameAf(k) ? frameAfKind(k) : undefined, label: null,
    });
    recent.length = Math.min(recent.length, 24);
    finished += 1; lane.k = null;
  };

  /** The vision phase as a pure function of its own clock: worker w holds car w, w + 3, w + 6 … */
  function visionSnap() {
    const items: { k: number; w: number; start: number; end: number }[] = [];
    for (let w = 0; w < VISION_WORKERS; w += 1) {
      for (let n = 0; w + n * VISION_WORKERS < cars; n += 1) {
        const start = w * 0.9 + n * CAR_S;
        items.push({ k: w + n * VISION_WORKERS, w, start, end: start + CAR_S });
      }
    }
    const doneItems = items.filter((i) => i.end <= visionT), flying = items.filter((i) => i.start <= visionT && i.end > visionT);
    const stageOf = (i: { start: number }): IdentifyStage => { const f = (visionT - i.start) / CAR_S; return f < 0.12 ? 'read' : f < 0.25 ? 'crop' : f < 0.82 ? 'vision' : 'merge'; };
    const make = (i: { k: number; w: number }, stage: IdentifyStage): IdentifyItem => {
      const item: IdentifyItem = { ...deps.item(i.k, stage, startedAt / 1000), worker: i.w };
      const lines = stage === 'read' ? 1 : stage === 'crop' ? 2 : stage === 'vision' ? 3 : 4;
      item.trace = (item.trace ?? []).slice(0, lines);
      // The model has not answered until the last step: its question is out, its reply is not.
      // Vision off: the plate and number reads stand, but the model never asks, answers or names a make.
      if (opts.visionOff) { item.prompt = null; item.reply = null; item.make = null; item.model = null; item.team = null; item.colour = null; }
      if (stage === 'read' || stage === 'crop' || stage === 'vision') { item.reply = null; item.number = null; item.make = null; item.model = null; item.team = null; item.colour = null; item.plate = null; }
      return item;
    };
    const active = flying.map((i) => make(i, stageOf(i)));
    const recentCars = doneItems.sort((a, b) => b.end - a.end).slice(0, 16).map((i) => make(i, 'done'));
    return { done: doneItems.length, active, recent: recentCars, stage: active[0]?.stage ?? ('done' as IdentifyStage) };
  }

  const identifySnap = (state: IdentifyLive['state']): IdentifyLive => {
    const v = visionSnap();
    return {
      job_id: jobId, state, done: state === 'done' ? cars : v.done, total: cars, started_at: startedAt / 1000, elapsed_ms: Math.round(visionT * 1000),
      stage: state === 'running' ? v.stage : 'done', model: opts.visionOff ? null : VISION_MODEL, vision_off_reason: opts.visionOff ?? null, workers: VISION_WORKERS,
      current: state === 'running' ? v.active[0] ?? null : null, active: state === 'running' ? v.active : [], recent: v.recent, errors: [],
    };
  };

  const laneFrame = (lane: Lane): LaneFrame => {
    const k = lane.k!, at = SCAN_STEPS.indexOf(lane.step), f = lane.waiting ? 0 : clamp01((t - lane.start) / dur(k, lane.step));
    const lf: LaneFrame = { image_id: 1000 + k, file: name(k), step: lane.step };
    if (at >= 2 || (at === 1 && f > 0.6)) lf.thumb_url = deps.thumb(k);
    const boxes = frameBoxes(k);
    if (at > 2) lf.boxes = boxes;
    else if (at === 2 && !lane.waiting) lf.boxes = boxes.filter((_, i) => f > 0.3 + i * 0.35);
    const af = frameAf(k);
    if (af && (at > 3 || (at === 3 && f > 0.2))) { lf.af = af; lf.af_kind = frameAfKind(k); }
    if (at > 3 || (at === 3 && f > 0.6)) { lf.stars = starsOf(k); lf.verdict = verdictOf(k); lf.panning = panOf(k); }
    return lf;
  };

  const scanSnap = (state: ScanProgress['state']): ScanProgress => {
    const inFlight = (from: number) => lanes.filter((l) => l.k != null && SCAN_STEPS.indexOf(l.step) >= from).length;
    const passed = { read: finished + inFlight(2), detect: finished + inFlight(3), sharpness: finished + inFlight(4), faces: finished + inFlight(5) };
    const afterScan = phase !== 'scan', afterGroup = afterScan && phase !== 'group';
    const group = !afterScan ? 0 : phase === 'group' ? (groupT / GROUP_S) * total : total;
    const pick = !afterGroup ? 0 : phase === 'pick' ? (pickT / PICK_S) * total : total;
    const idDone = state === 'done' ? cars : phase === 'vision' || phase === 'done' || phase === 'stopped' ? visionSnap().done : 0;
    const vision = cars > 0;
    // B1's nine first-class stages. The old single `identify` stage is gone:
    // OCR → DECISION → VISION, each with its own status word (§2, §6.8).
    const word = (done: number, tot: number, busy: number) => done >= tot && tot > 0 ? 'done' : busy > 0 || done > 0 ? 'running' : 'pending';
    const stages: PipelineStage[] = (['read', 'detect', 'sharpness', 'faces'] as const).map((id) => {
      const m = STAGE_META[id], done = passed[id];
      const busy = lanes.filter((l) => l.k != null && !l.waiting && stageOfStep(l.step) === id).length;
      return { id, label: m.label, model: m.model, device: m.device, workers_total: m.workers, workers_busy: Math.min(busy, m.workers), queue_len: 0, done, total, errors: 0, rate_per_s: Math.round(rate * 10) / 10, eta_s: rate > 0.05 && done < total ? (total - done) / rate : null, status: word(done, total, busy) };
    });
    const identifyOn = phase === 'vision' || phase === 'done' || phase === 'stopped';
    const sub = (id: string, label: string, done: number, tot: number, busy: number, model: string): PipelineStage => ({
      id, label, model, device: 'ollama', workers_total: VISION_WORKERS, workers_busy: busy, queue_len: 0,
      done, total: tot, errors: 0, rate_per_s: 0, eta_s: null, status: word(done, tot, busy),
    });
    stages.push(
      sub('identify:ocr', 'Identify · OCR', identifyOn ? cars : 0, cars, 0, 'plate + OCR readers'),
      sub('identify:decision', 'Identify · Decision', identifyOn ? cars : 0, cars, 0, 'local decision model'),
      sub('identify:vision', 'Identify · Vision', idDone, cars, phase === 'vision' ? Math.min(VISION_WORKERS, cars - idDone) : 0, vision && !opts.visionOff ? `${VISION_MODEL} via Ollama` : ''),
      { id: 'group', label: 'Group', model: 'EXIF time + similarity', device: 'CPU', workers_total: 1, workers_busy: phase === 'group' ? 1 : 0, queue_len: 0, done: Math.floor(group), total, errors: 0, rate_per_s: phase === 'group' ? total / GROUP_S : 0, eta_s: phase === 'group' ? GROUP_S - groupT : null, status: word(Math.floor(group), total, phase === 'group' ? 1 : 0) },
      { id: 'pick', label: 'Pick', model: 'keeper ranker', device: 'CPU', workers_total: 1, workers_busy: phase === 'pick' ? 1 : 0, queue_len: 0, done: Math.floor(pick), total, errors: 0, rate_per_s: phase === 'pick' ? total / PICK_S : 0, eta_s: phase === 'pick' ? PICK_S - pickT : null, status: word(Math.floor(pick), total, phase === 'pick' ? 1 : 0) },
    );
    const open = stages.findIndex((s) => s.done < s.total);
    const headAt = open < 0 ? stages.length - 1 : open, head = stages[headAt];
    const workers: PipelineWorker[] = lanes.map((lane) => {
      if (lane.k == null) return { stage: 'read', worker_index: lane.id, lane: lane.id, state: 'idle', waiting_for: null, file: null, step: null, ms_in_step: 0, current: null };
      return {
        stage: stageOfStep(lane.step), worker_index: lane.id, lane: lane.id, state: paused ? 'paused' : lane.waiting ? 'waiting' : 'busy',
        waiting_for: paused ? 'paused' : lane.waiting, file: name(lane.k), step: STEP_WORDS[lane.step], ms_in_step: Math.round((t - lane.start) * 1000), current: laneFrame(lane),
      };
    });
    const scanFrac = clamp01((passed.read + passed.detect + passed.sharpness + passed.faces + group + pick) / (total * 6));
    const found = {
      subjects: Math.floor(passed.detect * 1.1), soft: recent.filter((r) => r.verdict !== 'sharp').length, closed_eyes: Math.floor(passed.faces * 0.02),
      numbers: Math.floor(idDone * 0.8), plates: Math.floor(idDone * 0.5), bursts: afterGroup ? Math.round(total / 5.5) : 0, keepers: phase === 'pick' || !afterGroup ? 0 : Math.round(total / 8),
    };
    const eta = rate > 0.05 ? Math.max(0, total - finished) / rate + GROUP_S + PICK_S : null;
    return {
      job_id: jobId, state, stage: head.id, stage_index: headAt, stage_count: 9, done: head.done, total: head.total, rate_per_s: head.rate_per_s,
      eta_s: state === 'running' ? head.eta_s : null, recent: [...recent], found, pipeline: { stages, workers },
      overall: { done: Math.floor(scanFrac * total), total, fraction: state === 'done' ? 1 : scanFrac, rate_per_s: rate, eta_s: state === 'running' ? eta : null, eta_confident: finished >= 8 },
    };
  };

  const emit = () => {
    if (mode === 'ingest') sendScan(scanSnap('running'));
    if (phase === 'vision') deps.onIdentify(identifySnap('running'));
  };

  /** End the run: the identify snapshot first, then the scan's, as the engine does. */
  const conclude = (scan: 'done' | 'stopped' | null, identify: 'done' | 'stopped' | null) => {
    if (over) return;
    over = true; sim.finished = true; clearInterval(timer);
    if (identify) deps.onIdentify(identifySnap(identify));
    if (scan && mode === 'ingest') sendScan(scanSnap(scan));
    phase = identify === 'stopped' || scan === 'stopped' ? 'stopped' : 'done';
    deps.onEnd(phase === 'stopped' ? 'stopped' : 'done');
  };

  const advance = (dt: number) => {
    t += dt;
    if (phase === 'scan') {
      for (const lane of lanes) {
        if (lane.k == null) {
          if (next < total && t >= lane.id * 0.4) { lane.k = next; next += 1; enter(lane, 'read'); }
          continue;
        }
        if (lane.waiting) {
          if (slotFree(lane.step)) { lane.waiting = null; lane.start = t; }
          continue;
        }
        if (t - lane.start < dur(lane.k, lane.step)) continue;
        const at = SCAN_STEPS.indexOf(lane.step);
        if (at >= SCAN_STEPS.length - 1) finish(lane); else enter(lane, SCAN_STEPS[at + 1] as LaneStep);
      }
      if (finished >= total && lanes.every((l) => l.k == null)) phase = 'group';
    } else if (phase === 'group') {
      groupT += dt;
      if (groupT >= GROUP_S) { groupT = GROUP_S; phase = 'pick'; }
    } else if (phase === 'pick') {
      pickT += dt;
      if (pickT >= PICK_S) { pickT = PICK_S; if (cars > 0) phase = 'vision'; else conclude('done', null); }
    } else if (phase === 'vision') {
      visionT += dt;
      if (visionSnap().done >= cars) conclude(mode === 'ingest' ? 'done' : null, 'done');
    }
  };

  const step = (seconds: number) => {
    if (paused || over) return;
    const before = finished;
    for (let left = seconds; left > 1e-6 && !over; left -= 0.1) advance(Math.min(0.1, left));
    if (seconds > 0) rate = rate * 0.85 + ((finished - before) / seconds) * 0.15;
  };

  const sim: IngestSim = {
    jobId, mode, finished: false,
    stop: () => conclude(mode === 'ingest' ? 'stopped' : null, mode === 'identify' || phase === 'vision' ? 'stopped' : null),
    cancelVision: () => { if (phase === 'vision') conclude(mode === 'ingest' ? 'done' : null, 'stopped'); },
    setPaused: (v) => { paused = v; if (!over) emit(); },
    info: () => ({
      phase, paused, done: phase === 'vision' ? visionSnap().done : finished, total: phase === 'vision' ? cars : total, eta: null,
      detail: phase === 'vision' ? 'Reading cars' : lanes.find((l) => l.k != null) ? name(lanes.find((l) => l.k != null)!.k!) : 'Grouping',
    }),
  };

  // `at=<seconds>` starts partway through the run so any phase can be opened at once.
  for (let skipped = 0; skipped < opts.at && !over; skipped += 0.25) {
    const wasPaused = paused;
    paused = false; step(0.25); paused = wasPaused;
  }
  if (!over) {
    emit();
    timer = window.setInterval(() => {
      const now = performance.now();
      const dt = Math.min(0.5, (now - last) / 1000) * deps.speed();
      last = now;
      step(dt);
      if (!over) emit();
    }, 250);
  }
  return sim;
}
