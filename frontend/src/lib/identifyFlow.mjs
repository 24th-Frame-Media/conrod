// Pure logic for the identify live screen: the flow row, the image to show, the confidence meter and the words around them.
import { msWords, percent } from './decider.mjs';
import { visionModelWords, visionNodeSub, visionOffReason } from './vision.mjs';

/** The stages the `identify-progress` event reports, in the order they happen. */
export const IDENTIFY_STAGES = ['read', 'crop', 'vision', 'merge', 'done'];

/**
 * The six nodes the flow row draws. `vision` lights the two AI nodes: READ is the plate/number
 * read, VISION is the model's own answer, so both glow while the model is working.
 */
export const FLOW_NODES = [
  { number: '01', label: 'LOAD', sub: 'RAW', stage: 'read' },
  { number: '02', label: 'CROP', sub: 'subject', stage: 'crop' },
  { number: '03', label: 'READ', sub: 'plate + OCR', stage: 'vision' },
  { number: '04', label: 'VISION', sub: 'model', stage: 'vision' },
  { number: '05', label: 'ENTRY LIST', sub: 'candidates', stage: 'merge' },
  { number: '06', label: 'RESULT', sub: 'number + fields', stage: 'done' },
];

/** The first node index of a stage; before the first car, and for an unknown stage, that is the start. */
export function activeStage(stage) {
  const i = FLOW_NODES.findIndex((node) => node.stage === stage);
  return i < 0 ? 0 : i;
}

/**
 * The row's nodes with their state. Everything before the first node of the active stage is
 * `done`, a node of the active stage is `active`, the rest is `pending`. `stage` overrides the
 * car-specific stage the screen is showing; otherwise the live car's own stage wins over the
 * run-wide one. A finished run has every node done: there is nothing left to pulse.
 */
export function flowNodes(live, model, stage) {
  const at = stage ?? live?.current?.stage ?? live?.stage ?? 'read';
  const active = activeStage(at);
  // `done` only ever comes from a finished car (the engine clears `current` when the run
  // ends), so the whole row checks off while that result is held.
  const finished = at === 'done';
  // Vision off is a reason, not a model: the VISION node says so instead of naming one.
  const reason = visionOffReason(live);
  return FLOW_NODES.map((node, i) => ({
    ...node,
    sub: node.label !== 'VISION' ? node.sub : reason ? visionNodeSub(reason) : visionModelWords(model) ? `Reading cars — ${visionModelWords(model)}` : node.sub,
    off: node.label === 'VISION' && Boolean(reason),
    state: finished || i < active ? 'done' : node.stage === at ? 'active' : 'pending',
  }));
}

/** States in which the decision model cannot run as things stand, for the node's dimmed look. */
const DECIDER_OFF = new Set(['off', 'not_installed', 'installing', 'no_gpu', 'no_vram']);

/** A question, shortened for the node's subline; the whole text stays in the node's title. */
function shortAsk(text) {
  const t = String(text ?? '').replace(/\s+/g, ' ').trim();
  return t.length > 44 ? `${t.slice(0, 43)}…` : t;
}

/**
 * The decision model's node for the identify flow: what it is asking (or last answered), how many
 * frames it has judged, and the seconds each frame takes. It runs beside the vision workers, so
 * it appears in the same row; when it is off, the node is dimmed and says why.
 * `serving` is the cloud decision service actually doing the moments job (e.g. "jev-1.13 via
 * OpenCode"). Its state comes from `decider-progress`, a feed for the LOCAL model only: when a
 * cloud service serves, nothing is installed locally, so the node must name that service and
 * never show the local model's "not installed"/off state.
 */
export function deciderStepNode(live, serving = null) {
  if (!live || typeof live !== 'object') return null;
  const frames = Math.max(0, Math.round(Number(live.done) || 0));
  const busy = Number(live.busy) > 0 || live.current != null;
  const ask = live.current?.asking?.[0]?.text ?? '';
  const said = live.recent?.[0]?.says?.[0];
  const answer = said ? `${said.label} ${percent(said.p)}` : '';
  const cloud = Boolean(serving);
  const off = !cloud && DECIDER_OFF.has(live.state) && !busy && frames === 0;
  const sub = off ? 'off' : cloud ? serving : shortAsk(ask) || (frames ? `${frames} frames` : 'waiting');
  const per = live.avg_ms ? `${msWords(live.avg_ms)}/frame` : '';
  const meta = off ? '' : [frames ? `${frames} frames` : '', per, answer].filter(Boolean).join(' · ');
  const title = cloud
    ? [serving, shortAsk(ask), answer].filter(Boolean).join(' · ')
    : [sub, live.message, answer].filter(Boolean).join(' · ');
  return {
    id: 'decider', number: '', label: 'DECIDER', sub, meta, title,
    state: busy ? 'active' : frames > 0 ? 'done' : 'pending', off, filter: null,
  };
}

/** The picture of the car: the crop when there is one, else the preview, else the thumbnail. */
export function displayPath(item) {
  if (!item) return '';
  return item.crop_path || item.preview_path || item.thumb_path || item.path || '';
}

/** Just the file name of `displayPath`, for the caption over the stage. */
export function fileName(item) {
  const path = displayPath(item);
  return path.split(/[\\/]/).pop() ?? path;
}

/**
 * The car the stage and the side panels show: the strip's selection when it still exists in
 * this run, otherwise the live car, otherwise the newest finished one.
 */
export function displayItem(live, selectedId) {
  if (!live) return null;
  if (selectedId != null) {
    const hit = [...(live.active ?? []), ...(live.recent ?? [])].find((item) => item.detection_id === selectedId);
    if (hit) return hit;
  }
  return live.current ?? live.recent?.[0] ?? null;
}

/**
 * One lane per identify worker: its car being read, else the last car it finished, else empty.
 * `live.workers` is the run's worker count; older snapshots or tests fall back to the highest
 * worker index seen. `live` says whether that lane is reading right now.
 */
export function workerLanes(live) {
  if (!live) return [];
  const active = live.active ?? [];
  const recent = live.recent ?? [];
  const seen = [...active, ...recent].map((item) => Number(item.worker) || 0);
  const count = Math.max(Number(live.workers) || 0, ...seen.map((worker) => worker + 1), 0);
  const reading = new Map(active.map((item) => [Number(item.worker) || 0, item]));
  const finished = new Map();
  for (const item of recent) {
    const worker = Number(item.worker) || 0;
    if (!finished.has(worker)) finished.set(worker, item);
  }
  return Array.from({ length: count }, (_, worker) => ({
    worker,
    item: reading.get(worker) ?? finished.get(worker) ?? null,
    live: reading.has(worker),
  }));
}

/** Human labels for the vision model's action tags. */
export const ACTION_LABELS = {
  airborne: 'Airborne', dust: 'Dust', tyre_smoke: 'Tyre smoke', spray: 'Spray',
  flames: 'Flames', sparks: 'Sparks', contact: 'Contact', wheel_lift: 'Wheel lift',
};
export function actionLabel(id) {
  return ACTION_LABELS[id] ?? String(id ?? '');
}

/** 0..100, rounded, clamped; null when there is no confidence at all. */export function confidencePercent(conf) {
  const n = Number(conf);
  if (conf == null || !Number.isFinite(n)) return null;
  return Math.max(0, Math.min(100, Math.round(n * 100)));
}

/** Green from .8, amber from .5, red below; `none` when the value is missing. */
export function confidenceTone(conf) {
  const n = Number(conf);
  if (conf == null || !Number.isFinite(n)) return 'none';
  const c = Math.max(0, Math.min(1, n));
  return c >= 0.8 ? 'good' : c >= 0.5 ? 'warn' : 'bad';
}

/** The header's bar: 0..100, never past the ends and never NaN. */
export function progressPercent(done, total) {
  const d = Number(done), t = Number(total);
  if (!Number.isFinite(d) || !Number.isFinite(t) || t <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((d * 100) / t)));
}

/** "1.2 s", "42 s", "2 min 03 s", "1 h 02 min": a stopwatch, never finer than it deserves. */
export function elapsed(ms) {
  const s = Number(ms) / 1000;
  if (ms == null || !Number.isFinite(s) || s < 0) return '';
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)} s`;
  const whole = Math.round(s);
  if (whole < 3600) return `${Math.floor(whole / 60)} min ${String(whole % 60).padStart(2, '0')} s`;
  const m = Math.round(whole / 60);
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')} min`;
}

/** Cars a minute and seconds a car, or null before anything finished. */
export function rate(done, elapsedMs) {
  const d = Number(done), ms = Number(elapsedMs);
  if (!(d > 0) || !(ms > 0)) return null;
  const perMin = d / (ms / 60000), secondsPerCar = ms / 1000 / d;
  if (!Number.isFinite(perMin) || !Number.isFinite(secondsPerCar)) return null;
  return { perMin, secondsPerCar };
}

/** The header's rate: cars a minute once it is quick, seconds a car while it is slow. */
export function rateLabel(done, elapsedMs) {
  const r = rate(done, elapsedMs);
  if (!r) return '';
  if (r.perMin >= 10) return `${Math.round(r.perMin)} cars/min`;
  return `${r.secondsPerCar.toFixed(1)} s/car`;
}

/** The model's reply as pretty JSON; a JSON string is parsed, anything else is left as it came. */
export function prettyReply(reply) {
  if (reply == null) return '';
  if (typeof reply === 'string') {
    try { return JSON.stringify(JSON.parse(reply), null, 2); } catch { return reply; }
  }
  try { return JSON.stringify(reply, null, 2) ?? ''; } catch { return ''; }
}

const SOURCE_WORDS = { vlm: 'VLM', ocr: 'OCR', entry: 'entry', entrylist: 'entry', manual: 'manual', plate: 'plate', text: 'text' };

/** The number's provenance as a chip: "vlm" -> "VLM", "ocr+entry" -> "OCR+entry". */
export function sourceLabel(source) {
  const s = String(source ?? '').trim().toLowerCase();
  if (!s) return '';
  return s.split('+').map((word) => SOURCE_WORDS[word] ?? word).join('+');
}
