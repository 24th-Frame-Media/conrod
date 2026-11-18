// Pure logic for the identify live screen: the flow row, the image to show, the confidence meter and the words around them.

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
  return FLOW_NODES.map((node, i) => ({
    ...node,
    sub: node.label === 'VISION' && model ? String(model) : node.sub,
    state: finished || i < active ? 'done' : node.stage === at ? 'active' : 'pending',
  }));
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
 * The newest finished car while its result is still fresh: the screen holds it for a beat after
 * the next car takes over, so the number and fields can actually be read. `now` is milliseconds.
 */
export function justFinished(live, now, withinMs = 1800) {
  const item = live?.recent?.[0];
  if (!item || item.at == null) return null;
  const at = Number(item.at) * 1000;
  if (!Number.isFinite(at)) return null;
  return Math.abs(Number(now) - at) <= withinMs ? item : null;
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
