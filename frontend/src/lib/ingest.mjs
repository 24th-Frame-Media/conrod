// Pure logic for the unified ingest screen: one flowchart for scan + group + pick + vision, lane state words,
// which lanes the grid shows, and the single progress bar. The contract the engine fills is in docs/plans/ingest-live-contract.md.
import { deciderNode } from './decider.mjs';
import { deciderStepNode, flowNodes } from './identifyFlow.mjs';
import { STAGE_ORDER, orderStages, stageStatus } from './scan.mjs';
import { visionNodeSub, visionOffReason } from './vision.mjs';

/** The steps a scan lane carries a frame through, in order. */
export const SCAN_STEPS = ['read', 'decode', 'detect', 'sharpness', 'faces', 'persist'];

/** The pipeline stage each step belongs to, so a lane that reports a step can be filed under a flowchart node. */
export const STEP_STAGE = { read: 'read', decode: 'read', detect: 'detect', sharpness: 'sharpness', faces: 'faces', persist: 'faces' };

/** The stage of a step; an unknown step keeps `fallback`. */
export function stageOfStep(step, fallback = '') {
  return STEP_STAGE[step] ?? fallback;
}

/** Index of a step in `SCAN_STEPS`, or -1. */
export function stepIndex(step) {
  return SCAN_STEPS.indexOf(step);
}

const SCAN_NODE_IDS = ['read', 'detect', 'sharpness', 'faces', 'group', 'pick'];
const NODE_LABELS = { read: 'READ', detect: 'DETECT', sharpness: 'SHARPNESS', faces: 'FACES', group: 'GROUP', pick: 'PICK' };
/** Stages that have no per-item progress: they run once over the whole album. */
const PHASES = new Set(['group', 'pick']);

const n = (v) => Math.max(0, Math.round(Number(v) || 0)).toLocaleString('en-US');
const two = (i) => String(i + 1).padStart(2, '0');

/** Is a vision phase part of this run: the identify run reports in, or the scan lists an identify stage that is on. */
export function visionExpected(stages, identify) {
  if (identify) return true;
  return (Array.isArray(stages) ? stages : []).some((s) => s.id === 'identify' && stageStatus(s) !== 'off');
}

/** "2 failed" for the frames that came to nothing inside a stage; '' for none (and for engines that do not count them). */
export function failedWords(errors) {
  const count = Math.max(0, Math.round(Number(errors) || 0));
  return count > 0 ? `${count.toLocaleString('en-US')} failed` : '';
}

function scanNode(id, stage, finished) {
  const status = stage ? stageStatus(stage) : 'pending';
  const off = status === 'off';
  const state = off ? 'pending' : finished ? 'done' : status === 'done' ? 'done' : status === 'active' ? 'active' : 'pending';
  const workers = stage?.workers_total ?? 0;
  const busy = stage?.workers_busy ?? 0;
  const model = stage?.model ?? '';
  let sub;
  if (off) sub = 'not used';
  else if (PHASES.has(id)) sub = state === 'done' ? 'done' : state === 'active' ? 'working' : 'queued';
  else if (!stage || !(stage.total > 0)) sub = state === 'done' ? 'done' : 'waiting';
  else sub = `${n(stage.done)} / ${n(stage.total)}`;
  const crew = workers > 0 ? (state === 'done' ? `${workers} ${workers === 1 ? 'worker' : 'workers'}` : `${busy}/${workers} busy`) : '';
  const failed = off ? '' : failedWords(stage?.errors);
  const meta = off ? '' : [failed, crew, model].filter(Boolean).join(' · ');
  return {
    id, number: '', label: stage?.label ? String(stage.label).toUpperCase() : NODE_LABELS[id] ?? String(id).toUpperCase(),
    sub, meta, title: [sub, failed, crew, model, stage?.device].filter(Boolean).join(' · '), state, off,
    filter: off || PHASES.has(id) ? null : id,
  };
}

function visionNode(identify, identifyStage, model, finished) {
  let state = 'pending';
  if (identify) state = identify.state === 'running' ? 'active' : identify.state === 'done' ? 'done' : 'pending';
  else if (identifyStage) state = stageStatus(identifyStage) === 'done' ? 'done' : stageStatus(identifyStage) === 'active' ? 'active' : 'pending';
  if (finished && (!identify || identify.state === 'done')) state = 'done';
  // Vision off is a reason, never a model name: the engine sends `model: null` and says why.
  const reason = visionOffReason(identify);
  const shownModel = reason ? '' : identify?.model || model || identifyStage?.model || '';
  const total = identify ? identify.total : identifyStage?.total ?? 0;
  const done = identify ? identify.done : identifyStage?.done ?? 0;
  const workers = identify?.workers ?? identifyStage?.workers_total ?? 0;
  const sub = reason ? visionNodeSub(reason) : total > 0 ? `${n(done)} / ${n(total)} cars` : state === 'pending' ? 'waiting' : 'reading';
  const meta = [workers > 0 ? `${workers} ${workers === 1 ? 'worker' : 'workers'}` : '', shownModel].filter(Boolean).join(' · ');
  return { id: 'vision', number: '', label: 'VISION', sub, meta, title: [sub, meta].filter(Boolean).join(' · '), state, off: Boolean(reason), filter: 'vision' };
}

/**
 * The merged flowchart of one ingest run: every scan stage the engine reports (READ to PICK, in flow order), then VISION when the
 * run identifies, then DONE. A scan `identify` stage is VISION's fallback while no identify-progress has arrived, so it never
 * draws a node of its own. A stage the engine has not reported yet is drawn pending, so the row is whole from the first frame.
 * decider (the decision model's live snapshot) adds the DECIDER node before DONE, dimmed while it is off or unavailable.
 */
export function ingestNodes({ stages, identify = null, model = '', finished = false, decider = null } = {}) {
  const ordered = orderStages(stages);
  const byId = new Map(ordered.map((s) => [s.id, s]));
  const ids = [...SCAN_NODE_IDS, ...ordered.map((s) => s.id).filter((id) => id !== 'identify' && !SCAN_NODE_IDS.includes(id))];
  const rank = (id) => { const i = STAGE_ORDER.indexOf(id); return i < 0 ? STAGE_ORDER.length : i; };
  ids.sort((a, b) => rank(a) - rank(b));
  const nodes = ids.map((id) => scanNode(id, byId.get(id), finished));
  if (visionExpected(ordered, identify)) nodes.push(visionNode(identify, byId.get('identify'), model, finished));
  // The decision model sits after VISION: it answers about the same frames, on the same GPU, when the shoot type asks it.
  if (decider) nodes.push(deciderNode(decider));
  nodes.push({
    id: 'done', number: '', label: 'DONE', sub: finished ? 'ready to review' : 'review opens', meta: '', title: '',
    state: finished ? 'done' : 'pending', off: false, filter: null,
  });
  return nodes.map((node, i) => ({ ...node, number: two(i) }));
}

/**
 * The identify-only flowchart: today's six nodes in the same shape, none of them a filter. When
 * the decision model reports in (`decider`, else `live.decider`), its node sits after VISION,
 * with the question it is on and the frames it has judged.
 */
export function identifyNodes(live, model, decider = null) {
  const nodes = flowNodes(live, model).map((node) => ({
    id: node.label.toLowerCase(), number: node.number, label: node.label, sub: node.sub, meta: '', title: node.sub, state: node.state, off: node.off === true, filter: null,
  }));
  const step = deciderStepNode(decider ?? live?.decider ?? null);
  if (step) nodes.splice(4, 0, step);
  return nodes.map((node, i) => ({ ...node, number: String(i + 1).padStart(2, '0') }));
}

/**
 * Which phase the run is in. `scan.state` is 'idle' | 'starting' | 'running' | 'done' | 'stopped' | 'error'; `identify` is the run's
 * identify snapshot (already filtered to this run); `identifying` is whether the engine lists an identify operation for the album.
 * In `identify` mode only the vision phase exists.
 */
export function ingestPhase({ mode = 'ingest', scan, identify = null, identifying = false } = {}) {
  if (mode === 'identify') {
    if (!identify) return identifying ? 'starting' : 'idle';
    return identify.state === 'running' ? 'vision' : identify.state === 'done' ? 'done' : identify.state === 'stopped' ? 'stopped' : 'error';
  }
  const state = scan?.state ?? 'idle';
  if (state === 'idle' || state === 'starting' || state === 'error' || state === 'stopped') return state;
  if (state === 'running') {
    if (scan.stage === 'group' || scan.stage === 'pick') return scan.stage;
    return scan.stage === 'identify' || identify?.state === 'running' ? 'vision' : 'scan';
  }
  if (identify?.state === 'running' || identifying) return 'vision';
  if (identify?.state === 'stopped') return 'stopped';
  if (identify?.state === 'error') return 'error';
  return 'done';
}

/** A phase that still has work in flight. */
export function phaseRunning(phase) {
  return phase === 'starting' || phase === 'scan' || phase === 'group' || phase === 'pick' || phase === 'vision';
}

const PHASE_WORDS = { starting: 'Starting', scan: 'Scanning', group: 'Grouping', pick: 'Picking', vision: 'Identifying', done: 'Complete', stopped: 'Stopped', error: 'Failed', idle: 'Idle' };

/** The word next to the status dot: "Scanning", "Paused" while a running phase is frozen. */
export function phaseWord(phase, paused = false) {
  if (paused && phaseRunning(phase) && phase !== 'starting') return 'Paused';
  return PHASE_WORDS[phase] ?? 'Idle';
}

/** The dot's colour class: `running`, `done`, `stopped` (also paused) or `error`. */
export function phaseTone(phase, paused = false) {
  if (paused && phaseRunning(phase) && phase !== 'starting') return 'stopped';
  return phase === 'done' ? 'done' : phase === 'stopped' ? 'stopped' : phase === 'error' ? 'error' : phaseRunning(phase) ? 'running' : 'stopped';
}

const SCAN_WEIGHT = 0.7;

/**
 * 0..1 for the one bar. Without a vision phase it is the scan's own fraction. With one, the scan counts for 70% and the
 * identify run for the other 30%, so the bar does not sit at 100% while the model is still reading cars.
 */
export function ingestFraction({ scanFraction = 0, identify = null, vision = false } = {}) {
  const scan = Math.max(0, Math.min(1, Number(scanFraction) || 0));
  if (!vision) return scan;
  let id = 0;
  if (identify) id = identify.state === 'done' ? 1 : identify.total > 0 ? Math.max(0, Math.min(1, identify.done / identify.total)) : 0;
  return SCAN_WEIGHT * scan + (1 - SCAN_WEIGHT) * id;
}

/** Seconds left for the identify run from its own pace, or null before a car is done. */
export function identifyEta(identify) {
  if (!identify || identify.state !== 'running') return null;
  const done = Number(identify.done), total = Number(identify.total), ms = Number(identify.elapsed_ms);
  if (!(done > 0) || !(total > done) || !(ms > 0)) return null;
  return ((total - done) * ms) / done / 1000;
}

/** "about 6 min left", "estimating…", "almost done": the readout under Time left. */
export function leftLabel(label) {
  return label.startsWith('about ') ? `${label} left` : label;
}

/** "1.2 fr/s": the scan's rate as frames, not generic items. */
export function frameRate(text) {
  return text ? text.replace('/s', ' fr/s') : '';
}

/** The lane's identity in the grid: a stable lane id when the engine sends one, else its stage and index. */
export function laneKey(worker) {
  return worker.lane != null ? `lane:${worker.lane}` : `${worker.stage}:${worker.worker_index}`;
}

/** The number on the card: 1-based. */
export function laneNumber(worker) {
  return (worker.lane ?? worker.worker_index ?? 0) + 1;
}

/**
 * What the lane is doing as the card shows it. A lane the engine calls `paused`, waiting for `paused`, or that holds a
 * frame while the whole run is paused, is `paused`; paused beats busy, so a frozen lane never looks busy.
 */
export function laneState(worker, paused = false) {
  if (!worker || worker.state === 'idle') return 'idle';
  if (worker.state === 'paused' || worker.waiting_for === 'paused' || paused) return 'paused';
  return worker.state === 'waiting' ? 'waiting' : 'busy';
}

const WAITING_WORDS = {
  gpu: 'waiting for detector', detector: 'waiting for detector', faces: 'waiting for faces', lock: 'waiting for a lock',
  writer: 'waiting for the database', disk: 'waiting for the disk', ollama: 'waiting for the model',
};

/** The words next to the lane's name: "detect", "waiting for detector", "paused", "idle". */
export function laneText(worker, paused = false) {
  const state = laneState(worker, paused);
  if (state === 'idle') return 'idle';
  if (state === 'paused') return 'paused';
  if (state === 'waiting') return WAITING_WORDS[worker.waiting_for] ?? 'waiting';
  return worker.current?.step ?? worker.step ?? 'working';
}

/** The badge on the frame. */
export function laneBadge(state) {
  return state === 'busy' ? 'LIVE' : state === 'waiting' ? 'WAITING' : state === 'paused' ? 'PAUSED' : 'IDLE';
}

const PRIORITY = { busy: 0, paused: 0, waiting: 1, idle: 2 };

/**
 * The scan lanes the grid draws. Group and pick are barriers, not lanes. `filter` is a flowchart node id (a scan stage keeps that stage's
 * lanes). Past `cap` lanes the busiest are kept (busy and paused, then waiting, then idle) and the rest are counted in `hidden`;
 * whatever is kept stays in stable lane order so cards do not shuffle.
 */
export function selectLanes(workers, { filter = null, cap = 8, all = false, paused = false, identify = false } = {}) {
  const rank = (id) => { const i = STAGE_ORDER.indexOf(id); return i < 0 ? STAGE_ORDER.length : i; };
  let list = (Array.isArray(workers) ? workers : []).filter((w) => !PHASES.has(w.stage) && (identify || w.stage !== 'identify'));
  if (filter) list = list.filter((w) => w.stage === filter);
  const stable = list.map((w, i) => [w, i]).sort((a, b) => {
    if (a[0].lane != null && b[0].lane != null) return a[0].lane - b[0].lane || a[1] - b[1];
    return rank(a[0].stage) - rank(b[0].stage) || a[0].worker_index - b[0].worker_index || a[1] - b[1];
  }).map((p) => p[0]);
  if (all || stable.length <= cap) return { lanes: stable, hidden: 0 };
  const keep = new Set(stable.map((w, i) => [w, i]).sort((a, b) => PRIORITY[laneState(a[0], paused)] - PRIORITY[laneState(b[0], paused)] || a[1] - b[1]).slice(0, cap).map((p) => p[0]));
  return { lanes: stable.filter((w) => keep.has(w)), hidden: stable.length - cap };
}

/** The frame a scan lane draws: its own `current`, else its file's entry in the finished list, else just the file name and step. */
export function laneFrame(worker, recent) {
  const file = worker.current?.file ?? worker.file ?? '';
  const done = file && Array.isArray(recent) ? recent.find((r) => r.file === file) : undefined;
  const cur = worker.current;
  return {
    file,
    step: cur?.step ?? worker.step ?? null,
    thumb_url: cur?.thumb_url ?? done?.thumb_url ?? '',
    boxes: cur?.boxes ?? done?.boxes ?? [],
    af: cur?.af ?? done?.af ?? null,
    af_kind: cur?.af ? cur.af_kind ?? 'point' : done?.af ? done.af_kind ?? 'point' : null,
    stars: cur?.stars ?? done?.stars ?? null,
    verdict: cur?.verdict ?? done?.verdict ?? null,
    panning: cur?.panning ?? done?.panning ?? false,
  };
}

/** A fraction box [x1, y1, x2, y2] as CSS percentages, clamped into the frame; null for anything that is not a box. */
export function boxStyle(box) {
  if (!Array.isArray(box) || box.length < 4 || box.some((v) => !Number.isFinite(Number(v)))) return null;
  const c = (v) => Math.max(0, Math.min(1, Number(v)));
  const x1 = c(Math.min(box[0], box[2])), x2 = c(Math.max(box[0], box[2]));
  const y1 = c(Math.min(box[1], box[3])), y2 = c(Math.max(box[1], box[3]));
  if (x2 - x1 <= 0 || y2 - y1 <= 0) return null;
  const p = (v) => `${Math.round(v * 10000) / 100}%`;
  return { left: p(x1), top: p(y1), width: p(x2 - x1), height: p(y2 - y1) };
}

/** "★★★★☆" for 0..5 (rounded); '' when there is no rating yet. */
export function starsText(stars) {
  if (stars == null || !Number.isFinite(Number(stars))) return '';
  const s = Math.max(0, Math.min(5, Math.round(Number(stars))));
  return '★'.repeat(s) + '☆'.repeat(5 - s);
}

const VERDICT_WORDS = { sharp: 'Sharp', soft: 'Soft', blurred: 'Blurred' };

/** "Sharp · pan"; '' while the frame has no verdict. */
export function verdictText(verdict, panning = false) {
  const word = VERDICT_WORDS[verdict] ?? '';
  return word ? (panning ? `${word} · pan` : word) : panning ? 'pan' : '';
}

/** Pixels a 9.5 px mono tag needs for `text`: its glyphs plus the padding. */
export function tagWidth(text) {
  return String(text ?? '').length * 5.8 + 12;
}

/**
 * Whether a label fits inside its box. `box` is a fraction box [x1, y1, x2, y2] and `frameWidth` the drawn frame's width in pixels.
 * A box too narrow for its label keeps its outline and drops the text, so a small box never shows a clipped "p…".
 */
export function tagFits(box, frameWidth, text) {
  if (!Array.isArray(box) || box.length < 4) return false;
  const w = Math.abs(Number(box[2]) - Number(box[0])) * Number(frameWidth);
  return Number.isFinite(w) && w >= tagWidth(text);
}

/** Height in pixels of a 9.5 px mono tag (line-height 1.5). */
export const TAG_HEIGHT = 15;

/** The autofocus tag's words: a grid's union is not a point, so it says so. */
export function afLabel(kind) {
  return kind === 'grid' ? 'AF grid' : 'AF';
}

const rectsMeet = (a, b) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/**
 * Where the autofocus tag goes so it does not sit on a detection label. Detection labels hang at their box's top-left, and the AF
 * box usually sits on the subject, so its tag is tried inside the box at bottom-left, bottom-right, top-right, top-left, then just
 * below and just above the box; the first spot that fits the frame and meets no drawn detection label wins. `null` when the box
 * is too narrow for the tag or every spot collides (the dashed box alone still says where the camera focused).
 * `box` and detection boxes are fraction boxes; `frame` is the drawn picture's `{ width, height }` in pixels.
 */
export function afTagSpot(af, boxes, frame, text = 'AF') {
  if (!Array.isArray(af) || af.length < 4 || af.some((v) => !Number.isFinite(Number(v)))) return null;
  const W = Number(frame?.width), H = Number(frame?.height);
  if (!(W > 0) || !(H > 0)) return null;
  const x1 = Math.min(af[0], af[2]) * W, x2 = Math.max(af[0], af[2]) * W;
  const y1 = Math.min(af[1], af[3]) * H, y2 = Math.max(af[1], af[3]) * H;
  const w = tagWidth(text), h = TAG_HEIGHT;
  if (x2 - x1 < w) return null;
  const taken = (Array.isArray(boxes) ? boxes : []).flatMap((b) => {
    const label = boxLabel(b);
    if (!label || !tagFits(b.box, W, label)) return [];
    return [{ x: Math.min(b.box[0], b.box[2]) * W, y: Math.min(b.box[1], b.box[3]) * H, w: tagWidth(label), h }];
  });
  const spots = [
    { spot: 'bl', x: x1, y: y2 - h, inside: true },
    { spot: 'br', x: x2 - w, y: y2 - h, inside: true },
    { spot: 'tr', x: x2 - w, y: y1, inside: true },
    { spot: 'tl', x: x1, y: y1, inside: true },
    { spot: 'below', x: x1, y: y2, inside: false },
    { spot: 'above', x: x1, y: y1 - h, inside: false },
  ];
  for (const s of spots) {
    if (s.inside && y2 - y1 < h) continue;
    if (s.x < 0 || s.y < 0 || s.x + w > W + 0.5 || s.y + h > H + 0.5) continue;
    if (!taken.some((r) => rectsMeet(r, { x: s.x, y: s.y, w, h }))) return s.spot;
  }
  return null;
}

/** "car 92%" for a box label. */
export function boxLabel(box) {
  const pct = Number.isFinite(Number(box?.conf)) ? ` ${Math.round(Math.max(0, Math.min(1, Number(box.conf))) * 100)}%` : '';
  return `${box?.cls ?? ''}${pct}`.trim();
}

/**
 * The identify snapshot this run shows. In an ingest run it counts when it is running, or when the run planned a vision phase
 * (the scan lists an identify stage) or the engine lists an identify operation; otherwise it is a stale snapshot of an earlier run.
 */
export function ingestIdentify(identify, { planned = false, identifying = false } = {}) {
  if (!identify) return undefined;
  return identify.state === 'running' || planned || identifying ? identify : undefined;
}

/**
 * One strip of everything that just finished, newest first: identified cars lead (the vision phase comes last, so they are the
 * newest), then culled frames.
 */
export function stripItems({ cars = [], frames = [], limit = 30 } = {}) {
  const items = [
    ...cars.map((car) => ({ kind: 'car', key: `car:${car.detection_id}`, id: car.detection_id, car })),
    ...frames.map((frame) => ({ kind: 'frame', key: `frame:${frame.image_id}`, id: frame.image_id, frame })),
  ];
  return items.slice(0, limit);
}
