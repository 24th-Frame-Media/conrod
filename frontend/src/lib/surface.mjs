// Pure view model for a grouped pipeline run. This module never reads the DOM
// or the engine: it takes the normalized run snapshot (`buildRunSnapshot` in
// runSnapshot.mjs) plus the album's resolved settings, and produces the nodes,
// status words, per-stage controls, VRAM fit view, and type suggestion that
// screens render directly.
import { stageStatus } from './scan.mjs';
import { SUBJECT_WORDS } from './subjects.mjs';
import policies from './presetPolicies.mjs';
import { normalizeIdentifyOrder } from './settings.mjs';

/** Identify substage wire ids in flow order. `key` is the short name; `settingsKey`
 *  is the resolved setting that determines whether the step is on for this album. */
export const IDENTIFY_SUBSTAGES = Object.freeze([
  {
    id: 'identify:ocr', key: 'ocr', label: 'OCR', name: 'OCR', settingsKey: 'identify_ocr',
    purpose: 'Reads plates, numbers and visible text',
  },
  {
    id: 'identify:decision', key: 'decision', label: 'DECISION', name: 'Decision', settingsKey: 'identify_decision',
    purpose: 'Judges moments and suggests the shoot type',
  },
  {
    id: 'identify:vision', key: 'vision', label: 'VISION', name: 'Vision', settingsKey: 'use_vlm',
    purpose: 'Reads each subject with the vision model',
  },
]);

/** The grouped-run stages in flow order, followed by the finish node. The three
 *  identify stages sit in the album's `identify_order` (default OCR, Decision,
 *  Vision, one after another); everything around them is fixed. */
export function flowOrder(identifyOrder = null) {
  return Object.freeze([
    'read', 'detect', 'sharpness', 'faces',
    ...normalizeIdentifyOrder(identifyOrder).map((key) => `identify:${key}`),
    'group', 'pick', 'done',
  ]);
}

/** The default flow: sequential OCR, Decision, Vision. */
export const FLOW_ORDER = flowOrder(null);

const PLAIN_LABELS = Object.freeze({
  read: 'READ', detect: 'DETECT', sharpness: 'SHARPNESS', faces: 'FACES', group: 'GROUP', pick: 'PICK',
});
const PLAIN_PURPOSE = Object.freeze({
  read: 'Opens each RAW', faces: 'Finds faces and closed eyes',
  group: 'Groups frames of one moment', pick: 'Suggests one keeper per burst',
});

/** Canonical status vocabulary for a stage node. */
export const STAGE_WORDS = Object.freeze(['pending', 'running', 'paused', 'skipped', 'done', 'failed']);
const STAGE_WORD_LABEL = Object.freeze({
  pending: 'Pending', running: 'Running', paused: 'Paused', skipped: 'Skipped', done: 'Done', failed: 'Failed',
});

/** Per-stage engine commands, invoked as `{ action, args: { jobId, stage } }`. */
export const CONTROL_ACTIONS = Object.freeze({ pause: 'stage_pause', skip: 'stage_skip', rerun: 'stage_rerun' });

/** One control's exact wire call. `stage` is the wire id (e.g. `identify:vision`). */
export function controlSpec(action, jobId, stage) {
  return { action: CONTROL_ACTIONS[action], args: { jobId, stage } };
}

const clamp01 = (v) => Math.max(0, Math.min(1, Number(v) || 0));
const n = (v) => Math.max(0, Math.round(Number(v) || 0)).toLocaleString('en-US');
const two = (i) => String(i + 1).padStart(2, '0');

/** A plain reason for a stage that failed, or '' when the stage is fine.
 *  Reads both the current `status`/`error` wire shape and legacy `state`/`failed`/`reason` fields. */
export function stageFailureReason(stage) {
  if (!stage) return '';
  const raw = String(stage.status ?? stage.state ?? '').toLowerCase();
  if (raw === 'failed' || stage.failed === true) {
    return String(stage.reason ?? stage.error ?? 'This stage stopped with an error.').trim() || 'This stage stopped with an error.';
  }
  return '';
}

/**
 * Canonical status word for a stage. The engine's explicit `status` wins;
 * legacy payloads fall back to their counters and worker counts.
 * A switched-off stage keeps its counters at zero, so it never reads as active.
 */
export function stageWord(stage, { paused = false } = {}) {
  if (!stage) return 'pending';
  const raw = String(stage.status ?? stage.state ?? '').toLowerCase();
  // A stage the engine says is parked reads paused until it is resumed.
  if (stage.parked === true && raw !== 'done' && raw !== 'skipped' && raw !== 'failed') return 'paused';
  if (STAGE_WORDS.includes(raw)) return raw;
  if (stageFailureReason(stage)) return 'failed';
  if (stage.skipped === true || raw === 'off') return 'skipped';
  const total = Number(stage.total ?? 0), done = Number(stage.done ?? 0);
  const busy = Number(stage.workers_busy ?? 0), workers = Number(stage.workers_total ?? 0);
  if (workers === 0 && total === 0 && done === 0) return 'pending';
  if (total > 0 && done >= total && busy === 0) return 'done';
  if (busy > 0 || done > 0) return paused ? 'paused' : 'running';
  return 'pending';
}

/** The colour/tone class shared by status chips and flowchart nodes. */
export function toneFor(state) {
  switch (String(state ?? '').toLowerCase()) {
    case 'running': return 'running';
    case 'paused': case 'stage_paused': return 'paused';
    case 'queued': return 'queued';
    case 'failed': case 'error': return 'error';
    case 'done': case 'done_errors': return 'done';
    case 'skipped': case 'stopped': case 'scanning': case 'indexed': case 'incomplete': return 'stopped';
    default: return 'idle';
  }
}

/** CSS state for the node box (the flowchart's `id-node` classes). */
function stateForWord(word, off) {
  if (off) return 'off';
  return word === 'running' ? 'active' : word;
}

/** Whether a stage is enabled for this album from its resolved settings.
 *  Defaults preserve compatibility with older albums: OCR needs a reader,
 *  Decision needs a decision service, and Vision follows `use_vlm`. */
export function stageEnabled(key, settings = {}) {
  const s = settings ?? {};
  const on = (v, dflt = false) => (v === undefined ? dflt : Boolean(v));
  const enabled = (arr) => (Array.isArray(arr) ? arr.filter((m) => m && m.enabled !== false) : []);
  switch (key) {
    case 'ocr':
      return on(s.identify_ocr, true) && (on(s.read_plates) || on(s.read_numbers) || on(s.read_text));
    case 'decision':
      return on(s.identify_decision, true)
        && (s.moments_detect !== false)
        && (on(s.use_decider) || s.moments_backend === 'decider' || enabled(s.moments_services).length > 0);
    case 'vision':
      return on(s.use_vlm);
    default:
      return true;
  }
}

/** Plain reason shown on a disabled step. */
export function offReason(key) {
  const name = IDENTIFY_SUBSTAGES.find((d) => d.key === key)?.name ?? 'This step';
  return `${name} is off for this album`;
}

/** Shortcut metadata pointing to where a disabled step can be enabled. */
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

/** Plain one-line consequence of skipping a step, shown before confirming.
 *  Derived from the snapshot's counters rather than hardcoded. */
export function skipConsequence(key, { subject = SUBJECT_WORDS.subjects, remaining = 0 } = {}) {
  const left = n(remaining);
  switch (key) {
    case 'ocr': return `Skipping OCR: the vision model reads the numbers on its own. ${left} ${subject.many} still to read.`;
    case 'decision': return 'Skipping Decision: no shoot-type suggestion and no burst peaks. Ranking stays as it is, and a mixed album keeps its stored type.';
    case 'vision': return `Skipping Vision: no make/model reads for ${left} ${subject.many}. OCR and Decision results stay as they are.`;
    default: return `Skipping this step: its work will not run.`;
  }
}

function controlSpecs(stageId, jobId, word, off, consequence, { skippable = true, rerunnable = true } = {}) {
  const mk = (action, available, extra = {}, argExtra = null) => {
    const spec = controlSpec(action, jobId, stageId);
    return {
      ...spec,
      args: argExtra ? { ...spec.args, ...argExtra } : spec.args,
      available: Boolean(available) && !off,
      ...extra,
    };
  };
  const isPaused = word === 'paused';
  return {
    pause: isPaused
      ? mk('pause', true, { label: 'Resume' }, { paused: false })
      : mk('pause', word === 'running', { label: 'Pause' }, word === 'running' ? { paused: true } : null),
    skip: mk('skip', skippable && word !== 'skipped' && word !== 'done' && word !== 'failed', { label: 'Skip', consequence }),
    rerun: mk('rerun', rerunnable && (word === 'done' || word === 'skipped' || word === 'failed'), { label: 'Run again' }),
  };
}

// ---- VRAM fit and local-model concurrency ----

const GIG = 2 ** 30;
const gb = (bytes) => (bytes > 0 ? `${(bytes / GIG).toFixed(1)} GB` : null);
const finite = (v) => (Number.isFinite(Number(v)) ? Number(v) : 0);

/**
 * Normalise `identifyVram: { cardBytes, decisionBytes, visionBytes, fitsBoth, mode }`.
 * When `fitsBoth` is false, forces serial mode and surfaces a warning with the
 * measured sizes alongside a plain explanation of serial execution.
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
const isLocal = (provider) => /^(ollama|in_process|local|builtin)/i.test(String(provider ?? ''));
const isLoopback = (host) => {
  const h = String(host ?? '').trim();
  return !h || /^(?:https?:\/\/)?(localhost|127(?:\.\d+){3}|\[::1\]|::1)(:\d+)?(\/|$)/i.test(h);
};

/** Whether the decision and vision models both run locally. Only an
 *  explicit local signal counts: an unset provider or cloud/remote host is not
 *  proof that both models share this PC's VRAM, so the preference stays hidden
 *  (shown ONLY when both are genuinely local). */
export function modelsLocal(settings = {}, identifyVram = null) {
  const s = settings ?? {};
  const enabled = (x) => (Array.isArray(x) ? x.filter((m) => m && m.enabled !== false) : []);
  const momentServices = enabled(s.moments_services);
  const visionServices = enabled(s.vlm_services);
  const hasVlmServices = Array.isArray(s.vlm_services) && s.vlm_services.length > 0;

  // 'auto' is the Rust default transport and MUST NOT mark decision as cloud
  // when use_decider is true or a local moment service is enabled.
  const decisionCloudOnly = ['openrouter', 'opencode'].includes(s.moments_transport)
    || (momentServices.length > 0 && momentServices.every((m) => isCloud(m.provider)) && !s.use_decider);
  const decisionLocalSignal = s.use_decider === true
    || s.moments_transport === 'in_process'
    || momentServices.some((m) => isLocal(m.provider));
  let decisionLocal = stageEnabled('decision', s) && !decisionCloudOnly && decisionLocalSignal;

  let visionLocal = false;
  let visionCloudOnly = false;
  let visionConfigured = false;
  if (hasVlmServices) {
    visionConfigured = visionServices.length > 0;
    visionCloudOnly = visionServices.length > 0
      && visionServices.every((m) => isCloud(m.provider) || (isLocal(m.provider) && !isLoopback(m.host)));
    visionLocal = stageEnabled('vision', s)
      && visionServices.some((m) => isLocal(m.provider) && isLoopback(m.host));
  } else {
    visionConfigured = Boolean(s.vlm_provider);
    visionCloudOnly = isCloud(s.vlm_provider) || (isLocal(s.vlm_provider) && !isLoopback(s.vlm_host));
    visionLocal = stageEnabled('vision', s)
      && isLocal(s.vlm_provider) && isLoopback(s.vlm_host);
  }

  if (identifyVram && finite(identifyVram.decisionBytes) > 0 && finite(identifyVram.visionBytes) > 0) {
    if (stageEnabled('decision', s) && !decisionCloudOnly && (decisionLocalSignal || !s.moments_transport)) {
      decisionLocal = true;
    }
    if (stageEnabled('vision', s) && !visionCloudOnly && visionConfigured) {
      visionLocal = true;
    }
  }

  return { decisionLocal, visionLocal, bothLocal: decisionLocal && visionLocal };
}

/** Concurrency preference (worker seats per model), shown only when both models
 *  run locally. Values are clamped to engine worker caps on the backend. */
export function concurrencyView(identifyVram = null, settings = {}) {
  const v = vramView(identifyVram);
  const local = modelsLocal(settings, identifyVram);
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

// ---- Shoot type suggestion ----

/** View model for the engine's `shoot_type_suggested` and confidence on an album.
 *  Suggestions are shown for `auto`/`mixed` albums and never silently applied;
 *  accepting only affects Vision prompts that have not run yet. */
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

// ---- Manage shoot per-stage toggles ----

export const STAGE_TOGGLES = Object.freeze([
  { key: 'ocr', settingsKey: 'identify_ocr', label: 'Read plates, numbers and text (OCR)', hint: 'A separate first pass. When it is off, the vision model reads numbers on its own.' },
  { key: 'decision', settingsKey: 'identify_decision', label: 'Judge moments with the decision model', hint: 'Asks what matters in this shoot, suggests a shoot type for mixed albums, and compares each burst for its peak.' },
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
  const reason = stageFailureReason(stage);
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
  const unskippable = id === 'read' || id === 'detect' || id === 'sharpness';
  const unrerunnable = id === 'read' || id === 'faces';
  const consequence = stage?.skip_consequence || skipConsequence('', { subject });
  return {
    id, key: null, number: '', label: stage?.label ? String(stage.label).toUpperCase() : PLAIN_LABELS[id] ?? String(id).toUpperCase(),
    sub, meta, title: [sub, count, fails, crew, stage?.model, stage?.device].filter(Boolean).join(' · '),
    word: finalWord, wordLabel: STAGE_WORD_LABEL[finalWord] ?? finalWord, tone: toneFor(finalWord), state: stateForWord(finalWord, off),
    off, offReason: off ? offReasonForPlain(id) : '', offShortcut: off ? { label: 'Enable in Manage shoot', target: 'manage-shoot', settingsTab: 'culling' } : null,
    honestCheck: finalWord === 'done' && !off,
    controls: controlSpecs(id, ctx.jobId, finalWord, off, consequence, { skippable: !unskippable, rerunnable: !unrerunnable }),
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
  const consequence = stage?.skip_consequence || skipConsequence(def.key, { subject, remaining });
  const reason = stageFailureReason(stage);
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
    word: finalWord, wordLabel: STAGE_WORD_LABEL[finalWord] ?? finalWord, tone: toneFor(finalWord),
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
 * Build the full surface model from the normalized run snapshot and resolved
 * settings. `snapshot` comes from `buildRunSnapshot` (stages, workers, counters,
 * failedStages, runActive, paused, view, activeJob); `context` adds resolved
 * settings, the album record (for type suggestions), VRAM fit info, the serving
 * decision service, and the album's subject words.
 *
 * The flowchart always includes the three identify sub-nodes even when the
 * engine omits a switched-off stage (rendered greyed out, never hidden).
 * Unknown stage ids from the engine are appended in snapshot order.
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

  // The chart follows the order the engine will run: the album's own `identify_order`.
  const order = flowOrder(settings?.identify_order);
  const parallel = Boolean(settings?.identify_parallel);

  const nodes = [];
  for (const id of order) {
    if (id === 'done') { nodes.push(doneNode(finished)); continue; }
    const def = IDENTIFY_SUBSTAGES.find((d) => d.id === id);
    nodes.push(def ? identifyNode(def, byId.get(id), { ...ctx, suggestion }) : plainNode(id, byId.get(id), ctx));
  }
  // Unknown stages from a newer engine still render (stage-agnostic), after the
  // known flow, in the order the snapshot carries them.
  const known = new Set(order);
  for (const stage of stages) {
    if (known.has(stage.id) || !stage || typeof stage.id !== 'string') continue;
    nodes.push(plainNode(stage.id, stage, ctx));
  }

  // Honest ✓ is phase-gated: only a stage that reported done is ticked.
  // Off/greyed nodes never count toward progress or the stage tally.
  const numbered = nodes.map((node, i) => ({ ...node, number: two(i) }));
  const activeNodes = numbered.filter((node) => !node.off && node.id !== 'done');
  const identify = numbered.filter((node) => IDENTIFY_SUBSTAGES.some((d) => d.id === node.id));
  const toggles = STAGE_TOGGLES.map((t) => ({ ...t, on: stageEnabled(t.key, settings) }));

  return {
    nodes: numbered, identify, vram, concurrency, suggestion, toggles,
    order: order.filter((id) => id !== 'done'), parallel,
    jobId, stageTotal: activeNodes.length, stageWords: STAGE_WORDS,
    hasFailedStage: numbered.some((node) => node.word === 'failed'),
  };
}
