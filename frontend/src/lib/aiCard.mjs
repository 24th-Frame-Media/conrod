// Pure model for the AI services & Photo helpers card (`AiSettingsCard.tsx`).
// No React and no engine calls live here: the component passes the settings
// draft and the model lists it enumerated from the engine, and every answer
// below is deterministic so it can be unit-tested. Setting names are the
// engine's wire format and must never change here.
//
// Snapshot prices are $ per million input/output tokens; one photo is estimated
// at ~900 tokens in + ~250 out. An unknown live model's price is reported as
// "varies" rather than invented.

const arr = (value) => (Array.isArray(value) ? value : []);

/** The seven helpers, in reading order. `capability` marks the two whose
 *  models are served by configured AI services; the rest always run on this PC. */
export const HELPERS = [
  { id: 'cars-people', title: 'Finds cars & people', desc: 'Spots the cars, bikes, trucks and people to check in each photo.', keys: ['include_cars', 'include_bikes', 'include_trucks'] },
  { id: 'sharpness', title: 'Checks sharpness', desc: 'Flags soft photos and helps choose the sharpest frame in a burst.', keys: ['use_quality_score'] },
  { id: 'faces', title: 'Finds faces', desc: "Checks faces and closed eyes, so a blink can lower a portrait's rating.", keys: ['include_people'] },
  { id: 'plates', title: 'Reads plates', desc: 'Finds number plates and reads their letters and numbers.', keys: ['read_plates'] },
  { id: 'race-numbers', title: 'Reads race numbers', desc: 'Reads numbers on cars, bikes and race suits; can also read signs and sponsor boards.', keys: ['read_numbers', 'read_text'] },
  { id: 'reads-cars', title: 'Reads cars', desc: "Tells you the car's make, name, colour and team, and confirms its race number.", capability: 'vision' },
  { id: 'moments', title: 'Spots moments', desc: 'Finds things like tyre smoke, a kiss or the peak of a jump, to help choose between sharp frames.', capability: 'decision' },
];

/** The real settings keys each Photo helper switch reads and writes. */
export const SIMPLE_CONTROLS = {
  'cars-people': [
    { key: 'include_cars', label: 'Cars' },
    { key: 'include_bikes', label: 'Bikes' },
    { key: 'include_trucks', label: 'Trucks' },
  ],
  sharpness: [{ key: 'use_quality_score', label: 'Use an extra quality check to break ties' }],
  faces: [{ key: 'include_people', label: 'Check faces and closed eyes' }],
  plates: [{ key: 'read_plates', label: 'Read number plates' }],
  'race-numbers': [
    { key: 'read_numbers', label: 'Read race numbers' },
    { key: 'read_text', label: 'Also read signs and sponsor boards' },
  ],
  'reads-cars': [{ key: 'use_vlm', label: 'Use a vision model' }],
  moments: [{ key: 'identify_decision', label: 'Judge moments' }],
};

export const IN_TOK = 900, OUT_TOK = 250;

/** The six addable services. `kind: 'key'` = API key, `kind: 'host'` = a
 *  per-host Ollama server (one card per host). `decisions` = offers a Decisions
 *  dropdown; `thinking` = offers a per-card Thinking toggle. `pM`/`cM` are
 *  $/million input/output tokens (-1 = varies, 0 = free tier). */
export const SERVICE_TYPES = {
  opencode: {
    name: 'OpenCode', tag: '¢ per photo', dot: 'amber', kind: 'key', decisions: true, thinking: true,
    keyPlaceholder: 'Paste your OpenCode key',
    note: 'Buy credits in the OpenCode Console. Conrod never logs your key.',
    models: [
      { id: 'glm-5.3-flash', role: 'vision', img: true, pM: 0.15, cM: 0.5, on: true },
      { id: 'jev-1.13', role: 'decisions', img: true, pM: 0.042, cM: 0, on: true },
      { id: 'jev-1.13-free', role: 'decisions', img: true, pM: 0, cM: 0, on: false },
      { id: 'gemini-decision-1', role: 'decisions', img: true, pM: 0.1, cM: 0, on: true },
      { id: 'gpt-6-luna-decision', role: 'decisions', img: true, pM: 0.1, cM: 0, on: false },
    ],
  },
  openrouter: {
    name: 'OpenRouter', tag: '¢ per photo', dot: 'amber', kind: 'key', decisions: true, thinking: true,
    keyPlaceholder: 'Paste your OpenRouter key',
    note: 'One key, many model makers. The decision routers can see photos too.',
    models: [
      { id: 'qwen/qwen2.5-vl-72b-instruct', role: 'vision', img: true, pM: 0.8, cM: 1, on: true },
      { id: 'qwen/qwen3-vl-30b-a3b-instruct', role: 'vision', img: true, pM: 0.15, cM: 0.6, on: false },
      { id: 'tencent/hy4', role: 'vision', img: true, pM: 0.417, cM: 1.2505, on: false },
      { id: 'typesafe/jev-router', role: 'decisions', img: true, pM: -1, cM: -1, on: true },
      { id: 'openrouter/auto', role: 'decisions', img: true, pM: -1, cM: -1, on: false },
      { id: 'openrouter/free', role: 'decisions', img: true, pM: 0, cM: 0, on: false },
    ],
  },
  ollama: {
    name: 'Local Ollama', tag: 'Your network', dot: 'amber', kind: 'host', decisions: false, thinking: true,
    note: 'A host on your network. Its vision models join car reading and moments.',
    models: [
      { id: 'qwen3-vl:8b', role: 'vision', local: true, img: true, on: true },
      { id: 'qwen3-vl:4b', role: 'vision', local: true, img: true, on: true },
      { id: 'llava:7b', role: 'vision', local: true, img: true, on: false },
    ],
  },
  gemini: {
    name: 'Gemini', tag: '¢ per photo', dot: 'amber', kind: 'key', decisions: true, thinking: true,
    keyPlaceholder: 'Paste your Google AI key',
    note: "Google's models, billed to your Google AI key. They read photos and decide.",
    models: [
      { id: 'gemini-4-hydrogen', role: 'both', img: true, pM: 1.5, cM: 7.5, on: true },
      { id: 'gemini-3.8-flash', role: 'both', img: true, pM: 0.825, cM: 4.125, on: false },
    ],
  },
  anthropic: {
    name: 'Anthropic', tag: '¢ per photo', dot: 'amber', kind: 'key', decisions: true, thinking: true,
    keyPlaceholder: 'Paste your Anthropic key',
    note: 'Claude models, billed to your Anthropic key. They read photos and decide.',
    models: [
      { id: 'claude-haiku-5.5', role: 'both', img: true, pM: 0.1, cM: 0.5, on: true },
      { id: 'claude-sonnet-5.5', role: 'both', img: true, pM: 2, cM: 10, on: false },
    ],
  },
  openai: {
    name: 'OpenAI', tag: '¢ per photo', dot: 'amber', kind: 'key', decisions: true, thinking: true,
    keyPlaceholder: 'Paste your OpenAI key',
    note: 'GPT models, billed to your OpenAI key. They read photos and decide.',
    models: [
      { id: 'gpt-6-luna', role: 'both', img: true, pM: 0.1, cM: 0.5, on: true },
      { id: 'gpt-6.1-terra', role: 'both', img: true, pM: 1, cM: 5, on: false },
    ],
  },
};

export const ADD_ORDER = ['opencode', 'openrouter', 'ollama', 'gemini', 'anthropic', 'openai'];
export const ADD_DESC = {
  opencode: 'Credits key · many makers', openrouter: 'One key · many makers',
  ollama: 'One card per host · vision only', gemini: 'Google AI key',
  anthropic: 'Anthropic key', openai: 'OpenAI key',
};
export const THINK_HINT = 'Models reason step by step before answering. Off keeps reads fast and cheap.';

/** The always-present built-in card: Conrod's own bundled models on this PC.
 *  `assets` names the manifest files a Download installs (exactly those, never
 *  the whole set); the decision model has its own engine installer instead. */
export const BUILTIN = {
  name: 'Built-in models (This PC)', tag: 'Own PC',
  note: 'Ready on this PC. Nothing is uploaded.',
  models: [
    {
      id: 'qwen3-vl:8b', role: 'vision', group: 'Reads photos', on: true,
      dl: '~6 GB', assets: ['Qwen3VL-8B-Instruct-Q4_K_M.gguf', 'mmproj-Qwen3VL-8B-Instruct-F16.gguf'],
    },
    { id: 'pplx-decider-local', role: 'decisions', group: 'Makes decisions', dl: '~5 GB', on: false, assets: null },
  ],
};

/** Real `decider_quant` precision choices for the built-in local decision model. */
export const DECIDER_QUANTS = [
  { id: 'auto', name: 'Auto' },
  { id: 'int8', name: '8-bit' },
  { id: 'int4', name: '4-bit' },
];

/** The engine row id each built-in card row reads (never a fuzzy name match:
 *  a cloud "Vision model" health row is NOT the built-in vision model). */
export const BUILTIN_ROW_ID = {
  'qwen3-vl:8b': 'builtin-vision',
  'pplx-decider-local': 'decider',
};

/** One global "How work is shared" control. Its value is the engine's
 *  round_robin / least_busy / random and is written to BOTH strategy keys. */
export const SHARING = [
  { id: 'round_robin', name: 'Round robin' },
  { id: 'least_busy', name: 'Load balance' },
  { id: 'random', name: 'Random' },
];
export const SHARING_HINT = 'Across every enabled service and picked model. Switched-off cards sit out.';
export function sharingValue(settings) {
  const v = String((settings && settings.vlm_strategy) || 'round_robin');
  return SHARING.some((s) => s.id === v) ? v : 'round_robin';
}
/** Pure: the settings patch for choosing a work-sharing mode. Both keys move. */
export function setSharing(value) {
  const v = SHARING.some((s) => s.id === value) ? value : 'round_robin';
  return { vlm_strategy: v, moments_strategy: v };
}

/* ---- price snapshot, indexed by the model ids the dropdowns offer ---- */
const PRICE_BY_ID = (() => {
  const out = {};
  for (const def of Object.values(SERVICE_TYPES))
    for (const m of def.models)
      if (typeof m.pM === 'number') out[m.id] = { pM: m.pM, cM: m.cM };
  return out;
})();

export function fmtUsd(v, deep) {
  if (deep) return v < 0.0001 ? v.toFixed(5) : v.toFixed(4);
  return v < 0.01 ? v.toFixed(4) : v.toFixed(2);
}
/** One picked model's cost object. */
export function photoCost(id) {
  if (PRICE_BY_ID[id] === undefined) return { varies: true }; // honest: unknown price
  const m = PRICE_BY_ID[id];
  if (m.pM === 0 && m.cM === 0) return { free: true };
  if (m.pM < 0 || m.cM < 0) return { varies: true };
  return { usd: (IN_TOK * m.pM + OUT_TOK * m.cM) / 1e6 };
}
/** A "None — skip" (or a cleared free-text row) is not a picked model: it must
 *  never be billed or counted in the pool. */
export function isPicked(sel) {
  return Boolean(sel) && String(sel).trim() !== '' && sel !== 'none';
}

/** Card cost line: per photo AND per 1,000, always "est." unless free.
 *  `picked` = [{ id, local }]. A "None — skip" is filtered out first. */
export function costLine(picked = []) {
  const chosen = picked.filter((p) => p && isPicked(p.id));
  if (!chosen.length) return 'No models picked yet — no cost.';
  if (chosen.every((p) => p.local)) return 'Free — runs on your own hardware.';
  const costs = chosen.filter((p) => !p.local).map((p) => photoCost(p.id));
  if (costs.some((c) => c && c.varies)) return 'est. varies per photo · varies per 1,000 photos — the router’s pick sets the price.';
  const usd = costs.reduce((s, c) => s + (c && c.usd ? c.usd : 0), 0);
  if (usd === 0) return 'est. $0 per photo · $0 per 1,000 photos (free tier).';
  return 'est. $' + fmtUsd(usd, true) + ' per photo · $' + fmtUsd(usd * 1000, false) + ' per 1,000 photos';
}

/* ---- a service card, with a per-card Thinking flag that defaults OFF ---- */
function visionModels(type) {
  const def = SERVICE_TYPES[type];
  if (!def) return [];
  return def.models.filter((m) => m.img && (m.role === 'vision' || m.role === 'both'));
}
export function decisionModels(type) {
  const def = SERVICE_TYPES[type];
  if (!def || !def.decisions) return [];
  return def.models.filter((m) => m.role === 'decisions' || m.role === 'both');
}
export function makeCard(type, opts = {}) {
  const c = {
    key: type === 'ollama' ? `ollama-${opts.hostIndex ?? 1}` : type,
    type, enabled: true, thinking: false, reasoning: false, sel: { vision: 'none', decisions: 'none' }, opts: {},
    apiKey: '', url: opts.url ?? (type === 'ollama' ? 'http://localhost:11434' : ''),
    tested: false,
  };
  const vPick = visionModels(type).find((m) => m.on) || visionModels(type)[0];
  const dPick = decisionModels(type).find((m) => m.on && m.role === 'decisions');
  if (vPick) c.sel.vision = vPick.id;
  if (dPick) c.sel.decisions = dPick.id;
  return c;
}
/** The Thinking default for every card. */
export function defaultThinking() { return false; }
function cardReady(card) {
  const def = SERVICE_TYPES[card && card.type];
  if (!def) return false;
  if (def.kind === 'key') return Boolean(card.apiKey);
  return Boolean(card.tested) || (Array.isArray(card.liveModels) && card.liveModels.length > 0);
}

/* ---- which live models fill each dropdown (all honest about gaps) ---- */
export function visionCandidates(type, live = []) {
  if (type === 'ollama') return live.filter((m) => m.vision).map((m) => ({ id: m.name, name: m.name }));
  let list = live;
  if (type === 'openrouter') {
    // Decision routers are capable models too, but they belong in the
    // Decisions dropdown, not Vision (matching the service's own model roles).
    const decisionIds = new Set(decisionModels('openrouter').map((m) => m.id));
    list = live.filter((m) => !decisionIds.has(m.id || m.name));
  }
  const out = list.map((m) => ({ id: m.id || m.name, name: m.name || m.id }));
  // A provider that cannot list models still gets its known capable ones.
  if (!out.length && SERVICE_TYPES[type]) return visionModels(type).map((m) => ({ id: m.id, name: m.id }));
  return out;
}
export function decisionCandidates(type, live = [], decisionLive = []) {
  const def = SERVICE_TYPES[type];
  if (!def || !def.decisions) return { models: [], freeText: false };
  const named = (m) => ({ id: m.id || m.model || m.name, name: m.name || m.id || m.model });
  // TODO(engine, backend-owned): OpenRouter's decision enumeration is a
  // decision_models command returning the same shape as OpenCode
  // ({ id/model, sees_photos, note? }); both render through this same path.
  // Until that command lands, we fall back to matching decision-capable ids
  // from the live model list, then to an honest free-text row when none match.
  if (type === 'opencode' || type === 'openrouter') {
    const ms = decisionLive.map(named);
    if (ms.length) return { models: ms, freeText: false };
    if (type === 'openrouter') {
      const matches = live
        .filter((m) => /decision|decider|jev/i.test((m.id || '') + ' ' + (m.name || '')))
        .map(named);
      return { models: matches, freeText: matches.length === 0 };
    }
    return { models: decisionModels('opencode').map((m) => ({ id: m.id, name: m.id })), freeText: false };
  }
  // gemini / anthropic / openai: every offered model both sees and decides.
  const out = live.map(named);
  return { models: out.length ? out : decisionModels(type).map((m) => ({ id: m.id, name: m.id })), freeText: false };
}

/* ---- settings wire helpers (vlm_services / moments_services) ----
   TODO(engine, backend-owned): every service entry now carries a `reasoning`
   boolean (per-service thinking, default false). The backend applies it when it
   builds the VLM request; this UI only reads and writes the field on the two
   service lists. ---- */
const SERVICE_PROVIDER_OF = {
  opencode: 'opencode', openrouter: 'openrouter', ollama: 'ollama',
  gemini: 'gemini', anthropic: 'anthropic', openai: 'openai',
};
export function providerToType(provider) {
  const p = String(provider || '').toLowerCase();
  if (p === 'opencode-go') return 'opencode';
  return SERVICE_PROVIDER_OF[p] || null; // in_process / local = built-in (separate)
}
function cardKeyFor(entry) {
  const type = providerToType(entry && entry.provider);
  if (!type) return null;
  if (type === 'ollama') return String(entry.id || entry.host || 'ollama');
  return type;
}
/** Derive the added service cards from the real settings draft. One card per
 *  service type; one card per host for Ollama. */
export function cardsFromDraft(settings = {}) {
  const groups = new Map();
  const add = (s, cap) => {
    const key = cardKeyFor(s);
    if (!key) return;
    if (!groups.has(key)) groups.set(key, { key, type: providerToType(s.provider), vlm: null, moments: null });
    groups.get(key)[cap] = s;
  };
  arr(settings.vlm_services).forEach((s) => add(s, 'vlm'));
  arr(settings.moments_services).forEach((s) => add(s, 'moments'));
  return [...groups.values()].map((g) => ({
    key: g.key, type: g.type,
    apiKey: String((g.vlm || g.moments || {}).api_key || ''),
    host: String((g.vlm || g.moments || {}).host || ''),
    enabled: (g.vlm || g.moments).enabled !== false,
    // Per-service thinking: the persisted `reasoning` flag (default false).
    reasoning: Boolean((g.vlm || g.moments || {}).reasoning),
    visionSel: String((g.vlm || {}).model || 'none'),
    decisionSel: String((g.moments || {}).model || 'none'),
  }));
}
/** Add a service: seed both service lists with its default picked models. */
export function addService(settings, type, opts = {}) {
  const def = SERVICE_TYPES[type];
  const vlm = arr(settings.vlm_services), moments = arr(settings.moments_services);
  if (!def) return { vlm_services: vlm.slice(), moments_services: moments.slice() };
  const card = makeCard(type, opts);
  const id = type === 'ollama' ? (opts.id || `ollama-${Date.now()}-${Math.floor(Math.random() * 1e4)}`) : type;
  const host = type === 'ollama' ? card.url : '';
  const vRow = { id: type === 'ollama' ? id : `vlm-${type}`, provider: SERVICE_PROVIDER_OF[type], model: card.sel.vision, host, api_key: '', enabled: true, reasoning: false };
  const mRow = { id: type === 'ollama' ? id : `moments-${type}`, provider: SERVICE_PROVIDER_OF[type], model: card.sel.decisions, host, api_key: '', enabled: true, reasoning: false };
  return {
    vlm_services: [...vlm, vRow],
    moments_services: def.decisions ? [...moments, mRow] : moments.slice(),
  };
}
/** Remove a card (all its vlm + moments rows). */
export function removeService(settings, key) {
  const drop = (list) => arr(list).filter((s) => cardKeyFor(s) !== key);
  return { vlm_services: drop(settings.vlm_services), moments_services: drop(settings.moments_services) };
}
/** Patch one card's key/host/enabled or a picked model. Pure. */
export function patchService(settings, key, patch = {}) {
  const map = (list, cap) => arr(list).map((s) => {
    if (cardKeyFor(s) !== key) return s;
    const next = { ...s };
    if ('apiKey' in patch) next.api_key = String(patch.apiKey);
    if ('host' in patch) next.host = String(patch.host);
    if ('enabled' in patch) next.enabled = Boolean(patch.enabled);
    if ('reasoning' in patch) next.reasoning = Boolean(patch.reasoning);
    if (cap === 'vlm' && 'visionSel' in patch) next.model = String(patch.visionSel);
    if (cap === 'moments' && 'decisionSel' in patch) next.model = String(patch.decisionSel);
    return next;
  });
  return { vlm_services: map(settings.vlm_services, 'vlm'), moments_services: map(settings.moments_services, 'moments') };
}

/* ---- helpers: who serves each one, and the anchored popup data ---- */
const VISION_HELPER = 'reads-cars';
export const DECISION_HELPER = 'moments';
/** ctx = { builtinVision:[ids], builtinDecisions:[ids],
 *  cards:[{type,enabled,visionSel,decisionSel,apiKey,tested,liveModels}],
 *  bundled:{ helperId:[{id,ready}] } }. */
export function servingFor(helperId, ctx = {}) {
  const list = [];
  const push = (id, ready) => { if (id && !list.some((x) => x.id === id)) list.push({ id, ready: Boolean(ready) }); };
  const cards = (ctx.cards || []).filter((c) => c.enabled);
  if (helperId === DECISION_HELPER) {
    (ctx.builtinDecisions || []).forEach((id) => push(id, true));
    for (const c of cards) {
      const ready = cardReady(c);
      if (c.decisionSel && c.decisionSel !== 'none') push(c.decisionSel, ready);
      else if (c.type === 'ollama' && c.visionSel && c.visionSel !== 'none') push(c.visionSel, ready);
    }
  } else if (helperId === VISION_HELPER) {
    (ctx.builtinVision || []).forEach((id) => push(id, true));
    for (const c of cards) if (c.visionSel && c.visionSel !== 'none') push(c.visionSel, cardReady(c));
  } else {
    // Non-AI helpers run on Conrod's own bundled models, never a cloud provider.
    (ctx.bundled && ctx.bundled[helperId] ? ctx.bundled[helperId] : []).forEach((m) => push(m.id, m.ready));
  }
  return list;
}
/** Data for the compact "model in use" line + its anchored popup. */
export function modelLine(list) {
  if (!list.length) return { head: null, rest: [], all: [], noModel: true };
  const all = list.map((x) => x.id);
  return { head: all[0], rest: all.slice(1), all, noModel: false };
}
/** Status for a helper row (no location tags, no AI-cloud wording). */
export function helperStatus(on, list) {
  if (!on) return { text: 'Off', cls: 'off', dot: 'off' };
  if (list.some((x) => x.ready)) return { text: 'Ready', cls: '', dot: 'ok' };
  if (list.length) return { text: 'Not installed', cls: 'locked', dot: 'amber' };
  return { text: 'No model', cls: 'off', dot: 'off' };
}

/* ── who is actually doing the moments (decisions) job ─────────────────────
   One source of truth for every "who serves moments" status. A cloud decision
   service (e.g. OpenCode's JEV) is ready with a key — nothing is installed on
   this PC — while the built-in local model reports its own install state. The
   "Spots moments" helper row, the This PC card's note and the identify screen's
   DECIDER node all read this, so none of them claims "not installed" while a
   cloud service is judging. Setting names are the engine's wire format. */

/** Cloud decision services, keyed by the provider ids the settings use. */
const DECISION_SERVICES = {
  opencode: 'OpenCode', openrouter: 'OpenRouter', gemini: 'Gemini',
  anthropic: 'Anthropic', openai: 'OpenAI',
};

/**
 * Who serves the moments job, in plain words. `localInstalled` is the built-in
 * local decider's install state (the This PC card's "Makes decisions" row); it
 * only matters when no cloud decision service is enabled with a picked model.
 * Returns { path, service, model, label, status, ready, others }.
 */
export function momentsStatus(settings = {}, localInstalled = false) {
  const cloud = cardsFromDraft(settings)
    .filter((c) => c.enabled && c.decisionSel && c.decisionSel !== 'none'
      && c.decisionSel !== 'in_process' && DECISION_SERVICES[c.type])
    .map((c) => ({ service: DECISION_SERVICES[c.type], model: c.decisionSel, ready: Boolean(c.apiKey) }));
  if (cloud.length) {
    const first = cloud[0];
    return {
      path: 'cloud', service: first.service, model: first.model,
      label: `${first.model} via ${first.service}`,
      status: first.ready ? 'Ready' : 'Needs a key', ready: first.ready,
      others: cloud.slice(1).map((c) => `${c.model} via ${c.service}`),
    };
  }
  return {
    path: 'local', service: null, model: localInstalled ? 'pplx-decider-local' : null,
    label: 'On this PC', status: localInstalled ? 'Ready' : 'Not installed',
    ready: Boolean(localInstalled), others: [],
  };
}

/* ---- pool count + one-line summary strip ---- */
export function poolCount(ctx = {}) {
  const builtinVision = (ctx.builtinVision || []).filter(isPicked);
  const builtinDecisions = (ctx.builtinDecisions || []).filter(isPicked);
  let n = builtinVision.length + builtinDecisions.length;
  let svc = n ? 1 : 0;
  for (const c of (ctx.cards || [])) {
    if (!c.enabled) continue;
    let joined = 0;
    if (isPicked(c.visionSel)) joined++;
    if (isPicked(c.decisionSel)) joined++;
    if (joined) { svc++; n += joined; }
  }
  return { n, svc };
}
export function summaryText(ctx = {}) {
  const p = poolCount(ctx);
  const routing = (SHARING.find((s) => s.id === ctx.routing) || SHARING[0]).name.toLowerCase();
  if (!p.n) return { text: 'No models in the pool. Enable a service and pick its models.', dot: 'amber' };
  return { text: `${p.n} model${p.n === 1 ? '' : 's'} ready across ${p.svc} service${p.svc === 1 ? '' : 's'}, shared ${routing}.`, dot: 'ok' };
}

/* ── interpreting the REAL engine check commands ──────────────────────────
   openrouter_check / openCodeCheck / ollama_models resolve with { ok:false,
   error } on a bad key or { ok:false, online:false, error } on an offline
   host; provider_models resolves with { models:[], error }. A rejected promise
   carries the same plain reason. `ready` is true only on a clean reply, so the
   Test button can never claim "Ready" for a service that refused us. Pure, so
   the failure-surfacing behaviour is unit-testable without the engine. */
export function checkOutcome(res, err) {
  const plain = (v) => {
    if (v == null) return 'The service did not answer.';
    if (typeof v === 'string') return v.trim() || 'The service did not answer.';
    if (typeof v === 'object') {
      const field = v.error ?? v.message;
      if (field != null && String(field).trim()) return String(field).trim();
    }
    return 'The service did not answer.';
  };
  let reason = err != null ? plain(err) : null;
  if (!reason && res && typeof res === 'object') {
    const failed = res.ok === false || res.online === false
      || (typeof res.error === 'string' && res.error.trim() !== '');
    if (failed) reason = res.error ? plain(res.error) : plain(res);
  }
  return { ready: !reason, reason: reason || '' };
}

/* ── built-in rows: the engine's model manifest, not invented flags ─────────
   Each row reads its OWN engine row by stable id (`builtin-vision`, `decider`).
   `downloaded` is disk truth (the verified files exist), separate from `ready`
   (serving: the decider is downloaded but switched off, so ready=false while
   downloaded=true). A cloud service's "Vision model" health row (file:'vlm')
   can never stand in for the built-in model. `available:false` means no pinned
   source exists yet: the row says "not available yet", with no Download. */
const BUILTIN_MATCH = {
  'qwen3-vl:8b': ['vision', 'qwen'],
  'pplx-decider-local': ['decision', 'decider'],
};
export function builtinRowStates(models = [], useDecider = false) {
  const rows = Array.isArray(models) ? models : [];
  const rowFor = (id) => {
    const want = BUILTIN_ROW_ID[id];
    // Preferred: the engine's stable row id.
    const byId = rows.find((m) => m && m.id === want);
    if (byId) return byId;
    // Legacy rows (no id): match the local model row by name/file, but NEVER a
    // cloud service row (file:'vlm') or a service-labelled health row.
    const needles = BUILTIN_MATCH[id];
    return rows.find((m) => {
      if (!m || m.id || String(m.file || '') === 'vlm') return false;
      const name = String(m.name || '').toLowerCase();
      const file = String(m.file || '').toLowerCase();
      return needles.some((n) => name.includes(n) || file.includes(n));
    });
  };
  const stateOf = (id) => {
    const row = rowFor(id);
    const downloaded = Boolean(row && (row.downloaded ?? row.ready));
    const available = !row || row.available !== false;
    return { downloaded, available };
  };
  const vision = stateOf('qwen3-vl:8b');
  const decision = stateOf('pplx-decider-local');
  return {
    'qwen3-vl:8b': { downloaded: vision.downloaded, on: vision.downloaded, available: vision.available },
    'pplx-decider-local': {
      downloaded: decision.downloaded,
      on: decision.downloaded && Boolean(useDecider),
      available: decision.available,
    },
  };
}

/** Honest download progress from the engine's real byte counts. `total` of 0
 *  (or missing) is truly unknown: an indeterminate bar, never a fake percent. */
export function downloadProgress(done, total) {
  const d = Number(done) || 0;
  const t = Number(total) || 0;
  const gb = (b) => (b / 2 ** 30).toFixed(b < 10 * 2 ** 30 ? 2 : 1);
  if (t <= 0) return { known: false, pct: null, label: 'Downloading…' };
  const pct = Math.max(0, Math.min(100, Math.round((d / t) * 100)));
  return { known: true, pct, label: `${gb(d)} GB of ${gb(t)} GB · ${pct}%` };
}
