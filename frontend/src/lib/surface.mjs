// The SURFACE of one grouped run (plan §6, batch B5). Pure, so `node --test`
// pins it. This module never reads the DOM or the engine: it is fed the ONE run
// snapshot (`buildRunSnapshot` in runSnapshot.mjs) plus the album's RESOLVED
// settings. Every screen draws from what `buildSurface` returns — nothing
// invents a node, a status word, a worker or a reason (§6.1).
//
// B1 split the single identify wire stage into three first-class stages; B2
// added per-stage controls; B3 adds the VRAM fit. This file binds to those
// contracts and mocks them so the whole surface is testable without Rust.
import { stageStatus } from './scan.mjs';
import { SUBJECT_WORDS } from './subjects.mjs';
import policies from './presetPolicies.mjs';

/** B1's wire ids, in flow order. `key` is the short name; `settingsKey` is the
 *  resolved setting that says whether the step is on for this album (§6.3). */
export const IDENTIFY_SUBSTAGES = Object.freeze([
  {
    id: 'identify:ocr', key: 'ocr', label: 'OCR', name: 'OCR', settingsKey: 'identify_ocr',
    purpose: 'Reads plates, numbers and visible text',
  },
  {
    id: 'identify:decision', key: 'decision', label: 'DECISION', name: 'Decision', settingsKey: 'use_decider',
    purpose: 'Judges moments and suggests the shoot type',
  },
  {
    id: 'identify:vision', key: 'vision', label: 'VISION', name: 'Vision', settingsKey: 'use_vlm',
    purpose: 'Reads each subject with the vision model',
  },
]);

/** The nine grouped-run stages, then the reveal/finish node (§2, §6.8). */
export const FLOW_ORDER = Object.freeze([
  'read', 'detect', 'sharpness', 'faces',
  'identify:ocr', 'identify:decision', 'identify:vision',
  'group', 'pick', 'done',
]);

const PLAIN_LABELS = Object.freeze({
  read: 'READ', detect: 'DETECT', sharpness: 'SHARPNESS', faces: 'FACES', group: 'GROUP', pick: 'PICK',
});
const PLAIN_PURPOSE = Object.freeze({
  read: 'Opens each RAW', faces: 'Finds faces and closed eyes',
  group: 'Groups frames of one moment', pick: 'Suggests one keeper per burst',
});

/** The ONE status vocabulary for a stage node (§4, §6.8). */
export const STAGE_WORDS = Object.freeze(['pending', 'running', 'paused', 'skipped', 'done', 'failed']);
export const STAGE_WORD_LABEL = Object.freeze({
  pending: 'Pending', running: 'Running', paused: 'Paused', skipped: 'Skipped', done: 'Done', failed: 'Failed',
});

/** B2 engine commands, called EXACTLY as `{ action, args: { jobId, stage } }`. */
export const CONTROL_ACTIONS = Object.freeze({ pause: 'stage_pause', skip: 'stage_skip', rerun: 'stage_rerun' });

/** One control's exact wire call. `stage` is the wire id (e.g. `identify:vision`). */
export function controlSpec(action, jobId, stage) {
  return { action: CONTROL_ACTIONS[action], args: { jobId, stage } };
}

const clamp01 = (v) => Math.max(0, Math.min(1, Number(v) || 0));
const n = (v) => Math.max(0, Math.round(Number(v) || 0)).toLocaleString('en-US');
const two = (i) => String(i + 1).padStart(2, '0');

/** A plain reason for a stage that failed, or '' when it is fine. Reads B1's
 *  `status`/`error` as well as the older `state`/`failed`/`reason` shapes. */
export function stageFailReason(stage) {
  if (!stage) return '';
  const raw = String(stage.status ?? stage.state ?? '').toLowerCase();
  if (raw === 'failed' || stage.failed === true) {
    return String(stage.reason ?? stage.error ?? 'This stage stopped with an error.').trim() || 'This stage stopped with an error.';
  }
  return '';
}

/**
 * The one canonical status word for a stage. The engine's explicit `status`
 * (B1) wins; legacy payloads fall back to their counters and worker counts.
 * A switched-off stage keeps its counters at zero, so it never reads as active.
 */
export function stageWord(stage, { paused = false } = {}) {
  if (!stage) return 'pending';
  const raw = String(stage.status ?? stage.state ?? '').toLowerCase();
  if (STAGE_WORDS.includes(raw)) return raw;
  if (stageFailReason(stage)) return 'failed';
  if (stage.skipped === true || raw === 'off') return 'skipped';
  const total = Number(stage.total ?? 0), done = Number(stage.done ?? 0);
  const busy = Number(stage.workers_busy ?? 0), workers = Number(stage.workers_total ?? 0);
  if (workers === 0 && total === 0 && done === 0) return 'pending';
  if (total > 0 && done >= total && busy === 0) return 'done';
  if (busy > 0 || done > 0) return paused ? 'paused' : 'running';
  return 'pending';
}

/** The tone class shared by the word chip and the node border. */
export function toneForWord(word) {
  switch (word) {
    case 'running': return 'running';
    case 'paused': return 'paused';
    case 'failed': return 'error';
    case 'done': return 'done';
    case 'skipped': return 'stopped';
    default: return 'idle';
  }
}

/** CSS state for the node box (the flowchart's `id-node` classes). */
function stateForWord(word, off) {
  if (off) return 'off';
  return word === 'running' ? 'active' : word;
}

/** Whether a stage is ON for this album, from the RESOLVED settings (§6.3).
 *  Defaults reproduce today's behaviour for older albums (§7). OCR needs a
 *  reader; Decision needs a decision service; Vision is the `use_vlm` switch. */
export function stageEnabled(key, settings = {}) {
  const s = settings ?? {};
  const on = (v, dflt = false) => (v === undefined ? dflt : Boolean(v));
  const enabled = (arr) => (Array.isArray(arr) ? arr.filter((m) => m && m.enabled !== false) : []);
  switch (key) {
    case 'ocr':
      return on(s.identify_ocr, true) && (on(s.read_plates) || on(s.read_numbers) || on(s.read_text));
    case 'decision':
      return on(s.use_decider) || s.moments_backend === 'decider' || enabled(s.moments_services).length > 0;
    case 'vision':
      return on(s.use_vlm);
    default:
      return true;
  }
}

/** The plain reason a greyed step is off (§6.3). */
export function offReason(key) {
  const name = IDENTIFY_SUBSTAGES.find((d) => d.key === key)?.name ?? 'This step';
  return `${name} is off for this album`;
}

/** Where to enable a greyed step. A shortcut, never a hidden control (§6.3). */
export function offShortcut(key) {
  const d = IDENTIFY_SUBSTAGES.find((x) => x.key === key);
  return {
    label: `Enable in Manage shoot`,
    target: 'manage-shoot',
    settingsKey: d?.settingsKey ?? key,
    settingsTab: 'identification',
    section: 'identify-steps',
  };
}

/** The plain one-line consequence of skipping a step, shown BEFORE confirming
 *  (§4). Derived from the snapshot's counters, never hardcoded. */
export function skipConsequence(key, { subject = SUBJECT_WORDS.subjects, remaining = 0 } = {}) {
  const left = n(remaining);
  switch (key) {
    case 'ocr': return `Skipping OCR: the vision model reads the numbers on its own. ${left} ${subject.many} still to read.`;
    case 'decision': return 'Skipping Decision: no shoot-type suggestion and no burst peaks. Ranking stays as it is, and a mixed album keeps its stored type.';
    case 'vision': return `Skipping Vision: no make/model reads for ${left} ${subject.many}. OCR and Decision results stay as they are.`;
    default: return `Skipping this step: its work will not run.`;
  }
}

function controlSpecs(stageId, jobId, word, off, consequence) {
  const mk = (action, available, extra = {}) => ({
    ...controlSpec(action, jobId, stageId), available: Boolean(available) && !off, ...extra,
  });
  return {
    pause: mk('pause', word === 'running', { label: 'Pause' }),
    skip: mk('skip', word !== 'skipped' && word !== 'done' && word !== 'failed', { label: 'Skip', consequence }),
    rerun: mk('rerun', word === 'done' || word === 'skipped' || word === 'failed' || word === 'paused', { label: 'Run again' }),
  };
}

// ---- VRAM fit + concurrency (§5, §6.8). Contract B3 emits `identifyVram`. ----

const GIG = 2 ** 30;
const gb = (bytes) => (bytes > 0 ? `${(bytes / GIG).toFixed(1)} GB` : null);
const finite = (v) => (Number.isFinite(Number(v)) ? Number(v) : 0);

/**
 * Normalise B3's `identifyVram: { cardBytes, decisionBytes, visionBytes,
 * fitsBoth, mode }`. `fitsBoth === false` forces serial mode and a warning with
 * the real numbers; serial mode is explained in plain words.
 */
export function vramView(identifyVram = null) {
  const v = identifyVram ?? {};
  const cardBytes = finite(v.cardBytes), decisionBytes = finite(v.decisionBytes), visionBytes = finite(v.visionBytes);
  const reported = typeof v.fitsBoth === 'boolean' ? v.fitsBoth
    : (cardBytes > 0 ? decisionBytes + visionBytes <= cardBytes : true);
  const fitsBoth = Boolean(reported);
  // Only-one-fits always engages the serial fallback, whatever the preference.
  const mode = !fitsBoth ? 'serial' : (v.mode === 'serial' ? 'serial' : 'concurrent');
  const numbers = {
    card: gb(cardBytes), decision: gb(decisionBytes), vision: gb(visionBytes), sum: gb(decisionBytes + visionBytes),
  };
  const showWarning = !fitsBoth && identifyVram != null;
  const warning = !fitsBoth
    ? (numbers.decision && numbers.vision && numbers.card
      ? `This PC can hold one model at a time: Decision ${numbers.decision} + Vision ${numbers.vision} > ${numbers.card}.`
      : 'This PC can only hold one of the two models at a time.')
    : '';
  const serialExplanation = 'Conrod finishes every frame with the first model before loading the second.';
  return { present: identifyVram != null, cardBytes, decisionBytes, visionBytes, fitsBoth, mode, numbers, showWarning, warning, serialExplanation };
}

const isCloud = (provider) => /^(opencode|openrouter|gemini|anthropic|openai|google)/i.test(String(provider ?? ''));
const isLocal = (provider) => /^(ollama|in_process|local)/i.test(String(provider ?? ''));

/** Whether the decision and vision models both run locally (§5). Only an
 *  explicit local signal counts: an unset/auto transport, or no configured
 *  service, is not proof that both models run on this PC, so the preference
 *  stays hidden (shown ONLY when both are genuinely local). */
export function modelsLocal(settings = {}) {
  const s = settings ?? {};
  const enabled = (x) => (Array.isArray(x) ? x.filter((m) => m && m.enabled !== false) : []);
  const momentServices = enabled(s.moments_services);
  const visionServices = enabled(s.vlm_services);
  const decisionCloud = ['openrouter', 'opencode', 'auto'].includes(s.moments_transport) || momentServices.some((m) => isCloud(m.provider));
  const decisionLocal = stageEnabled('decision', s) && !decisionCloud
    && (s.moments_transport === 'in_process' || momentServices.some((m) => isLocal(m.provider)));
  const visionCloud = visionServices.some((m) => isCloud(m.provider)) || isCloud(s.vlm_provider);
  const visionLocal = stageEnabled('vision', s) && !visionCloud
    && (visionServices.some((m) => isLocal(m.provider)) || isLocal(s.vlm_provider));
  return { decisionLocal, visionLocal, bothLocal: decisionLocal && visionLocal };
}

/** The concurrency preference (seats per model), shown ONLY when both models
 *  run locally. Seats are the two settings B3 clamps to the worker caps. */
export function concurrencyView(identifyVram = null, settings = {}) {
  const v = vramView(identifyVram);
  const local = modelsLocal(settings);
  const seats = {
    vision: Math.max(1, Math.round(finite(settings?.identify_vision_workers) || 2)),
    decision: Math.max(1, Math.round(finite(settings?.identify_decision_workers) || 1)),
    visionKey: 'identify_vision_workers', decisionKey: 'identify_decision_workers',
  };
  const mode = v.fitsBoth ? v.mode : 'serial';
  const explanation = !v.fitsBoth ? v.serialExplanation
    : mode === 'serial' ? 'Stages stay in order: Decision finishes before Vision loads.'
      : 'Both models fit. The seat preference sets how many workers each stage uses; the stages still run in order.';
  return { ...v, ...local, seats, visible: local.bothLocal, mode, explanation };
}

// ---- Type suggestion (§3b, §6.8, §7) ----

/** B1 stores `shoot_type_suggested` + confidence on the album; the suggestion
 *  is SHOWN, never silently applied. Accepting affects only not-yet-run Vision
 *  prompts. `auto` (the meta-type) is the only profile that gets one. */
export function typeSuggestionView(album = null, settings = null) {
  const profile = String(album?.scan_profile ?? settings?.scan_profile ?? '');
  const isAuto = profile === 'auto' || profile === 'mixed';
  const type = album?.shoot_type_suggested ?? null;
  const confidence = finite(album?.shoot_type_confidence);
  const accepted = album?.shoot_type_accepted ?? null;
  const shown = isAuto && Boolean(type) && !accepted;
  const typeLabel = (type && policies[type]?.label) || (type ? String(type).replace(/[-_]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase()) : '');
  return {
    isAuto, shown: Boolean(shown), type: type ?? null, typeLabel,
    confidence, confidencePct: Math.round(clamp01(confidence) * 100), accepted: Boolean(accepted),
    canAccept: Boolean(shown) && Boolean(type),
    note: 'Your chosen shoot type always wins. Accepting only changes the vision reads that have not run yet; nothing already finished is re-run.',
  };
}

// ---- Manage shoot per-stage toggles (§6.8) ----

export const STAGE_TOGGLES = Object.freeze([
  { key: 'ocr', settingsKey: 'identify_ocr', label: 'Read plates, numbers and text (OCR)', hint: 'A separate first pass. When it is off, the vision model reads numbers on its own.' },
  { key: 'decision', settingsKey: 'use_decider', label: 'Judge moments with the decision model', hint: 'Asks what matters in this shoot, suggests a shoot type for mixed albums, and compares each burst for its peak.' },
  { key: 'vision', settingsKey: 'use_vlm', label: 'Read each subject with the vision model', hint: 'Reads make, model, colour and team. Off keeps the OCR and Decision results.' },
]);

// ---- The whole surface ----

function plainPurpose(id, subject) {
  const w = subject ?? SUBJECT_WORDS.subjects;
  if (id === 'detect') {
    if (w.many === 'people') return 'Person detector';
    if (w.many === 'subjects') return "Finds the shoot's subjects";
    return `${w.title} and people detector`;
  }
  if (id === 'sharpness') return `Checks the main ${w.one} is in focus`;
  return PLAIN_PURPOSE[id] ?? null;
}

function plainNode(id, stage, ctx) {
  const { subject, paused } = ctx;
  const status = stage ? stageStatus(stage) : 'pending';
  const off = status === 'off';
  const word = stageWord(stage, { paused });
  const reason = stageFailReason(stage);
  const isFailed = Boolean(reason) && !off;
  const finalWord = isFailed ? 'failed' : off ? 'skipped' : word;
  const workers = Number(stage?.workers_total ?? 0), busy = Number(stage?.workers_busy ?? 0);
  const total = Number(stage?.total ?? 0), done = Number(stage?.done ?? 0);
  const purpose = plainPurpose(id, subject) ?? stage?.label ?? id;
  const sub = isFailed ? reason : off ? offReasonForPlain(id) : purpose;
  const count = !off && stage && total > 0 ? `${n(done)} / ${n(total)}` : '';
  const fails = !off && Number(stage?.errors ?? 0) > 0 ? `${n(stage.errors)} failed` : '';
  const crew = workers > 0 ? (finalWord === 'done' ? `${workers} ${workers === 1 ? 'worker' : 'workers'}` : `${busy}/${workers} busy`) : '';
  const meta = off ? '' : [count, fails, crew, stage?.model].filter(Boolean).join(' · ');
  return {
    id, key: null, number: '', label: stage?.label ? String(stage.label).toUpperCase() : PLAIN_LABELS[id] ?? String(id).toUpperCase(),
    sub, meta, title: [sub, count, fails, crew, stage?.model, stage?.device].filter(Boolean).join(' · '),
    word: finalWord, wordLabel: STAGE_WORD_LABEL[finalWord] ?? finalWord, tone: toneForWord(finalWord), state: stateForWord(finalWord, off),
    off, offReason: off ? offReasonForPlain(id) : '', offShortcut: off ? { label: 'Enable in Manage shoot', target: 'manage-shoot', settingsTab: 'culling' } : null,
    honestCheck: finalWord === 'done' && !off,
    controls: controlSpecs(id, ctx.jobId, finalWord, off, skipConsequence('', { subject })),
    suggestion: null, workersVisible: !off && (finalWord === 'running' || finalWord === 'paused'),
    filter: off || id === 'group' || id === 'pick' ? null : id,
    counters: { done, total, remaining: Math.max(0, total - done) },
    phase: id === 'group' || id === 'pick',
  };
}

function offReasonForPlain(id) {
  if (id === 'faces') return 'Faces are off for this album';
  return `${PLAIN_LABELS[id] ?? id} is off for this album`;
}

function identifyNode(def, stage, ctx) {
  const { settings, subject, decisionService, suggestion, jobId, paused } = ctx;
  const on = stageEnabled(def.key, settings);
  const off = !on;
  const word = off ? 'skipped' : stageWord(stage, { paused });
  const total = Number(stage?.total ?? 0), done = Number(stage?.done ?? 0);
  const remaining = Math.max(0, total - done);
  const consequence = skipConsequence(def.key, { subject, remaining });
  const reason = stageFailReason(stage);
  const finalWord = reason && !off ? 'failed' : word;
  const workers = Number(stage?.workers_total ?? 0), busy = Number(stage?.workers_busy ?? 0);
  const sub = off ? offReason(def.key)
    : reason ? reason
      : def.key === 'decision' && decisionService ? `Served by ${decisionService}`
        : def.purpose;
  const count = !off && finalWord !== 'pending' && finalWord !== 'skipped' && total > 0 ? `${n(done)} / ${n(total)}` : '';
  const crew = !off && workers > 0 && (finalWord === 'running' || finalWord === 'paused') ? `${busy}/${workers} busy` : '';
  const meta = [count, crew].filter(Boolean).join(' · ');
  return {
    id: def.id, key: def.key, number: '', label: def.label,
    sub, meta, title: [sub, count, stage?.model].filter(Boolean).join(' · '),
    word: finalWord, wordLabel: STAGE_WORD_LABEL[finalWord] ?? finalWord, tone: toneForWord(finalWord),
    state: stateForWord(finalWord, off),
    off, offReason: off ? offReason(def.key) : '', offShortcut: off ? offShortcut(def.key) : null,
    honestCheck: finalWord === 'done' && !off,
    controls: controlSpecs(def.id, jobId, finalWord, off, consequence),
    suggestion: def.key === 'decision' && suggestion.shown ? suggestion : null,
    workersVisible: !off && (finalWord === 'running' || finalWord === 'paused'),
    filter: null,
    counters: { done, total, remaining },
    phase: false,
  };
}

function doneNode(finished) {
  return {
    id: 'done', key: null, number: '', label: 'DONE',
    sub: finished ? 'ready to review' : 'review opens', meta: '', title: '',
    word: finished ? 'done' : 'pending', wordLabel: finished ? 'Done' : 'Pending', tone: finished ? 'done' : 'idle',
    state: finished ? 'done' : 'pending', off: false, offReason: '', offShortcut: null,
    honestCheck: finished, controls: controlSpecs('done', null, finished ? 'done' : 'pending', true, ''),
    suggestion: null, workersVisible: false, filter: null, counters: { done: 0, total: 0, remaining: 0 }, phase: false,
  };
}

/**
 * Build the whole surface from the ONE run snapshot + resolved settings.
 * `snapshot` is the object `buildRunSnapshot` returns (stages, workers,
 * counters, failedStages, runActive, paused, view, activeJob). `context` adds
 * the resolved settings, the album record (type suggestion), B3's VRAM, the
 * serving decision service and the album's subject words.
 *
 * The flowchart ALWAYS carries the three identify sub-nodes, even when the
 * engine omitted a switched-off stage (§6.3: greyed, never hidden). Unknown
 * stage ids the engine sent are appended as-is (§6.1 stage-agnostic).
 */
export function buildSurface(snapshot = {}, context = {}) {
  const {
    settings = null, album = null, identifyVram = null, decisionService = null,
    subject = SUBJECT_WORDS.subjects, jobId = null,
  } = context;
  const stages = Array.isArray(snapshot.stages) ? snapshot.stages : [];
  const byId = new Map(stages.map((s) => [s.id, s]));
  const paused = Boolean(snapshot.paused);
  const finished = snapshot.view?.state === 'done' || snapshot.view?.state === 'done_errors';
  const ctx = { settings, album, subject, decisionService, jobId, paused };

  const vram = vramView(identifyVram);
  const concurrency = concurrencyView(identifyVram, settings);
  const suggestion = typeSuggestionView(album, settings);

  const nodes = [];
  for (const id of FLOW_ORDER) {
    if (id === 'done') { nodes.push(doneNode(finished)); continue; }
    const def = IDENTIFY_SUBSTAGES.find((d) => d.id === id);
    nodes.push(def ? identifyNode(def, byId.get(id), { ...ctx, suggestion }) : plainNode(id, byId.get(id), ctx));
  }
  // Unknown stages from a newer engine still render (stage-agnostic), after the
  // known flow, in the order the snapshot carries them.
  const known = new Set(FLOW_ORDER);
  for (const stage of stages) {
    if (known.has(stage.id) || !stage || typeof stage.id !== 'string') continue;
    nodes.push(plainNode(stage.id, stage, ctx));
  }

  // Honest ✓ is phase-gated: only a stage that reported done is ticked.
  // Off/greyed nodes never count toward progress (§6.3) or the stage tally.
  const numbered = nodes.map((node, i) => ({ ...node, number: two(i) }));
  const activeNodes = numbered.filter((node) => !node.off && node.id !== 'done');
  const identify = numbered.filter((node) => IDENTIFY_SUBSTAGES.some((d) => d.id === node.id));
  const toggles = STAGE_TOGGLES.map((t) => ({ ...t, on: stageEnabled(t.key, settings) }));

  return {
    nodes: numbered, identify, vram, concurrency, suggestion, toggles,
    jobId, stageTotal: activeNodes.length, stageWords: STAGE_WORDS,
    hasFailedStage: numbered.some((node) => node.word === 'failed'),
  };
}
