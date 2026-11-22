// Pure model for the "Photo helpers" card (Version C — Two zones).
// No React and no engine calls live here: the component passes the settings
// draft and the model lists it enumerated from the engine, and every answer
// below is deterministic so it can be unit-tested. Setting names are the
// engine's wire format and must never change here.

export const ZONE_TITLES = { pc: 'On this PC', cloud: 'Via a cloud service' };
export const ZONE_EMPTY = {
  cloud: 'Add a cloud key or download the models to move car reading and moments here.',
};
export const NOT_SET_UP = 'Not set up — add a key or download the models';

/** The seven helpers, in reading order. `capability` marks the two whose
 * models are unlocked by a configured service; the rest always run on this PC. */
export const HELPERS = [
  { id: 'cars-people', title: 'Finds cars & people', desc: 'Spots the cars, bikes, trucks and people to check in each photo.', keys: ['include_cars', 'include_bikes', 'include_trucks'] },
  { id: 'sharpness', title: 'Checks sharpness', desc: 'Flags soft photos and helps choose the sharpest frame in a burst.', keys: ['use_quality_score'] },
  { id: 'faces', title: 'Finds faces', desc: "Checks faces and closed eyes, so a blink can lower a portrait's rating.", keys: ['include_people'] },
  { id: 'plates', title: 'Reads plates', desc: 'Finds number plates and reads their letters and numbers.', keys: ['read_plates'] },
  { id: 'race-numbers', title: 'Reads race numbers', desc: 'Reads numbers on cars, bikes and race suits; can also read signs and sponsor boards.', keys: ['read_numbers', 'read_text'] },
  { id: 'reads-cars', title: 'Reads cars', desc: "Tells you the car's make, name, colour and team, and confirms its race number.", capability: 'vision' },
  { id: 'moments', title: 'Spots moments', desc: 'Finds things like tyre smoke, a kiss or the peak of a jump, to help choose between sharp frames.', capability: 'decision' },
];

/** The on/off switches shown in a simple helper's expanded panel (real keys). */
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
};

export const SERVICE_IDS = ['local', 'ollama', 'openrouter', 'opencode'];
/** Where each provider-neutral service runs and what its price tag says. */
export const SERVICE_META = {
  local: { name: 'Built-in models (This PC)', short: 'This PC', tag: 'Own PC', zone: 'pc' },
  ollama: { name: 'Ollama hosts', short: 'Ollama', tag: 'Your network', zone: 'pc' },
  openrouter: { name: 'OpenRouter', short: 'OpenRouter', tag: '¢ per photo', zone: 'cloud' },
  opencode: { name: 'OpenCode', short: 'OpenCode', tag: '¢ per photo', zone: 'cloud' },
};

export const STRATS = [
  { id: 'least_busy', name: 'Least busy' },
  { id: 'round_robin', name: 'Round robin' },
  { id: 'random', name: 'Random' },
];
export const STRATS_HINT = 'Least busy uses the service with the shortest queue; Round robin takes turns.';

/** Exact empty-state ("fork") copy. The two paths lead into AI services. */
export const EMPTY_FORK = {
  heading: 'Two ways to let Conrod read your photos',
  lead: 'Pick the one that suits your PC. You can add the other any time.',
  download: {
    title: 'Download the models',
    sub: 'Free · needs a PC with a decent graphics card (about 5 GB) · works offline',
  },
  cloud: {
    title: 'Use a cloud service',
    sub: 'Add a key and pay as you go · works on any PC.',
    note: 'OpenCode covers both car reading and moments; OpenRouter does too, though its moments model can’t see photos yet.',
  },
  cloudFormTitle: 'Add a key to get started',
  opencodeNote: 'Buy credits in the OpenCode Console.',
  openrouterNote: 'Its moments model can’t see photos yet.',
};

export const SUMMARY_LINES = {
  both: 'Reading on this PC and through a cloud service. Your setup is ready.',
  local: 'Reading on this PC. The built-in models are ready.',
  cloud: 'Reading through a cloud service. Your key is saved.',
};

export const LOCKED_REASON = {
  vision: 'Reading cars needs a service that can read a photo.',
  decision: 'Moments need a service that can judge a photo.',
};

const arr = (value) => (Array.isArray(value) ? value : []);

/** Fold the engine's provider ids onto the four provider-neutral services. */
export function providerGroup(provider) {
  const p = String(provider || '').toLowerCase();
  if (p === 'in_process' || p === 'local') return 'local';
  if (p === 'opencode-go') return 'opencode';
  return p;
}

/** Derive the four services' configuration from the real settings draft.
 * Keys and hosts live on vlm_services / moments_services (the same wire
 * shape); the built-in models are switched on by `use_decider`. */
export function serviceConfig(settings = {}) {
  const vlm = arr(settings.vlm_services);
  const moments = arr(settings.moments_services);
  const all = [...vlm, ...moments];
  const rowsFor = (id) => all.filter((s) => providerGroup(s.provider) === id);
  const keyFor = (id) => {
    const withKey = rowsFor(id).filter((s) => String(s.api_key || '').trim());
    return withKey.length ? String(withKey[withKey.length - 1].api_key).trim() : '';
  };
  const enabledFor = (id) => rowsFor(id).some((s) => s.enabled);
  const localOn = Boolean(settings.use_decider);
  return {
    local: { downloaded: localOn, enabled: localOn },
    ollama: {
      hosts: [...new Set(vlm.filter((s) => providerGroup(s.provider) === 'ollama').map((s) => String(s.host || '')).filter(Boolean))],
      enabled: enabledFor('ollama'),
    },
    openrouter: { key: keyFor('openrouter'), enabled: enabledFor('openrouter') },
    opencode: { key: keyFor('opencode'), enabled: enabledFor('opencode') },
  };
}

/** A service "serves" a capability only when it is configured and enabled. */
export function serving(id, config) {
  const s = config?.[id];
  if (!s) return false;
  if (id === 'local') return Boolean(s.downloaded && s.enabled);
  if (id === 'ollama') return s.hosts.length > 0 && Boolean(s.enabled);
  return Boolean(s.key && s.enabled);
}

export function anyServing(config) {
  return SERVICE_IDS.some((id) => serving(id, config));
}
export function countOn(config) {
  return SERVICE_IDS.filter((id) => serving(id, config)).length;
}
export function isEmpty(config) {
  return !anyServing(config);
}

/** The service groups that can serve a capability, with the models the engine
 * enumerated for each. `modelLists[cap][id] = { models:[{id,name}], freeText }`. */
export function capGroups(cap, config, modelLists) {
  const lists = (modelLists && modelLists[cap]) || {};
  return SERVICE_IDS.filter((id) => serving(id, config)).map((id) => {
    const meta = SERVICE_META[id];
    const entry = lists[id] || {};
    const models = entry.models || [];
    const disabledNote = cap === 'decision' && id === 'openrouter' ? 'Not available for photos yet' : null;
    return {
      id,
      name: meta.short,
      tag: meta.tag,
      zone: meta.zone,
      key: (id === 'openrouter' || id === 'opencode') ? config[id].key : null,
      models: disabledNote ? [] : models,
      disabledNote,
      freeText: !disabledNote && (models.length === 0 || Boolean(entry.freeText)),
      testable: id !== 'local',
    };
  });
}

/** A capability is usable once a service that can actually look at a photo is
 * configured. OpenRouter's moments model cannot see photos yet, so it does not
 * unlock the decision lane. Readiness never waits on the (async) model list. */
export function capUsable(cap, config) {
  const photo = cap === 'decision'
    ? ['local', 'ollama', 'opencode']
    : ['local', 'ollama', 'openrouter', 'opencode'];
  return photo.some((id) => serving(id, config));
}

/** One-line status for a capability: ready, or not set up. */
export function capStatus(cap, config, modelLists) {
  if (!capUsable(cap, config)) return { text: NOT_SET_UP, tone: 'locked', tag: 'Needs setup' };
  const hasLocal = serving('local', config) || serving('ollama', config);
  const hasCloud = serving('openrouter', config) || serving('opencode', config);
  if (hasLocal && hasCloud) return { text: 'Ready on this PC + cloud', tone: 'ok', tag: 'This PC + cloud' };
  if (hasLocal) return { text: 'Ready on this PC', tone: 'ok', tag: 'This PC' };
  const names = capGroups(cap, config, modelLists)
    .filter((g) => g.zone === 'cloud' && !g.disabledNote)
    .map((g) => g.name);
  return { text: 'Ready via ' + (names.length ? names.join(' and ') : 'a cloud service'), tone: 'ok', tag: 'Cloud' };
}

function simpleHelperOn(helper, settings) {
  if (!helper.keys) return true;
  return helper.keys.some((key) => Boolean(settings?.[key]));
}

/** `ctx` = { settings, config, modelLists }. The row's slim status. */
export function statusOf(helper, ctx) {
  const { config, modelLists, settings } = ctx;
  if (helper.capability) return capStatus(helper.capability, config, modelLists);
  if (!simpleHelperOn(helper, settings)) return { text: 'Off', tone: 'off', tag: 'This PC' };
  return { text: 'Ready on this PC', tone: 'ok', tag: 'This PC' };
}

/** Helpers are slotted by where they actually run: simple ones on this PC,
 * capability ones in the cloud zone only when they run purely via a cloud
 * service. A not-set-up helper stays on this PC so the fork can lead it. */
export function zoneOf(helper, ctx) {
  if (!helper.capability) return 'pc';
  return capStatus(helper.capability, ctx.config, ctx.modelLists).tag === 'Cloud' ? 'cloud' : 'pc';
}

export function groupByZone(ctx) {
  const pc = HELPERS.filter((h) => zoneOf(h, ctx) === 'pc');
  const cloud = HELPERS.filter((h) => zoneOf(h, ctx) === 'cloud');
  return [
    { id: 'pc', title: ZONE_TITLES.pc, helpers: pc, empty: '' },
    { id: 'cloud', title: ZONE_TITLES.cloud, helpers: cloud, empty: ZONE_EMPTY.cloud },
  ];
}

/** Why a capability is locked, and what unlocks it. */
export function unlockReason(helper, ctx) {
  const cap = helper?.capability;
  if (!cap) return { locked: false };
  const { config } = ctx;
  if (capUsable(cap, config)) return { locked: false };
  let sub = LOCKED_REASON[cap] + ' Add a key or download the built-in models, then pick a model here.';
  if (cap === 'decision' && serving('openrouter', config) && !serving('opencode', config) && !serving('local', config)) {
    sub = 'OpenRouter’s moments model can’t see photos yet. Add an OpenCode key or download the built-in models.';
  }
  return { locked: true, reason: NOT_SET_UP, sub };
}

export function summaryLine(config) {
  const hasLocal = serving('local', config) || serving('ollama', config);
  const hasCloud = serving('openrouter', config) || serving('opencode', config);
  if (hasLocal && hasCloud) return SUMMARY_LINES.both;
  if (hasLocal) return SUMMARY_LINES.local;
  return SUMMARY_LINES.cloud;
}

/** Convenience: build the whole derived view the component renders. */
export function buildAiView(settings, modelLists) {
  const config = serviceConfig(settings);
  const ctx = { settings, config, modelLists };
  return {
    config,
    ctx,
    groups: groupByZone(ctx),
    isEmpty: isEmpty(config),
    summary: summaryLine(config),
    countOn: countOn(config),
  };
}

/* ── wiring helpers: keep the provider-neutral services in sync with the two
   real service lists the engine reads (vlm_services / moments_services). ── */

const PROVIDER_OF = { local: 'in_process', ollama: 'ollama', openrouter: 'openrouter', opencode: 'opencode' };
const DEFAULT_MODELS = {
  vision: { local: 'Built-in car reader', ollama: 'qwen2.5vl:7b', openrouter: 'qwen/qwen2.5-vl-72b-instruct', opencode: 'glm-5.3-flash' },
  decision: { local: 'in_process', ollama: 'qwen2.5vl:7b', openrouter: 'perplexity/pplx-decider-flash-v1.3:free', opencode: 'jev-1.13' },
};
let idCounter = 0;
const newId = (cap, id) => `${cap}-${id}-${Date.now()}-${idCounter++}`;

function baseRow(id, cap, patch) {
  return {
    id: newId(cap, id),
    provider: PROVIDER_OF[id],
    model: DEFAULT_MODELS[cap][id],
    host: id === 'ollama' ? String(patch.host ?? patch.addHost ?? '') : '',
    api_key: 'key' in patch ? String(patch.key) : '',
    enabled: 'enabled' in patch ? Boolean(patch.enabled) : true,
  };
}

const groupRows = (list, id) => list.filter((s) => providerGroup(s.provider) === id);
const ensureGroup = (list, id, make) =>
  groupRows(list, id).length ? list : [...list, make()];

/** Apply one config-panel edit. Returns the settings keys to write back:
 * { vlm_services, moments_services, use_decider }. Pure. */
export function setService(settings, id, patch = {}) {
  const copy = (value) => arr(value).map((s) => ({ ...s }));
  let vlm = copy(settings.vlm_services);
  let moments = copy(settings.moments_services);
  let use_decider = Boolean(settings.use_decider);

  if (id === 'local') {
    if (patch.download) use_decider = true;
    if ('enabled' in patch) use_decider = Boolean(patch.enabled);
    moments = ensureGroup(moments, 'local', () => ({ id: 'moments-local', provider: 'in_process', model: 'in_process', host: '', api_key: '', enabled: true }));
    moments = moments.map((s) => providerGroup(s.provider) === 'local' ? { ...s, enabled: use_decider } : s);
    return { vlm_services: vlm, moments_services: moments, use_decider };
  }

  if (patch.removeHost != null) {
    const host = String(patch.removeHost);
    const drop = (list) => list.filter((s) => !(providerGroup(s.provider) === 'ollama' && String(s.host || '') === host));
    return { vlm_services: drop(vlm), moments_services: drop(moments), use_decider };
  }
  if (patch.addHost) {
    const host = String(patch.addHost).trim();
    if (host) {
      const next = { ...patch, host };
      vlm = [...vlm, baseRow(id, 'vision', next)];
      moments = [...moments, baseRow(id, 'decision', next)];
    }
    return { vlm_services: vlm, moments_services: moments, use_decider };
  }

  vlm = ensureGroup(vlm, id, () => baseRow(id, 'vision', patch));
  moments = ensureGroup(moments, id, () => baseRow(id, 'decision', patch));
  const apply = (s) => {
    const next = { ...s };
    if ('key' in patch) next.api_key = String(patch.key);
    if ('enabled' in patch) next.enabled = Boolean(patch.enabled);
    if ('model' in patch) next.model = String(patch.model);
    return next;
  };
  vlm = vlm.map((s) => providerGroup(s.provider) === id ? apply(s) : s);
  moments = moments.map((s) => providerGroup(s.provider) === id ? apply(s) : s);
  return { vlm_services: vlm, moments_services: moments, use_decider };
}

/** Write a Version (model id) back to the matching service in one lane. */
export function setCapModel(settings, cap, id, model) {
  const key = cap === 'vision' ? 'vlm_services' : 'moments_services';
  const list = arr(settings[key]).map((s) => ({ ...s }));
  let done = false;
  const next = list.map((s) => {
    if (providerGroup(s.provider) === id && !done) { done = true; return { ...s, model: String(model) }; }
    return s;
  });
  if (!done) next.push(baseRow(id, cap, { model }));
  return { [key]: next };
}

/* ════════════════════════════════════════════════════════════════════════
   Services-plan view — approved mock conrod-notes/mocks/ai-settings/
   services-plan/index.html. Pure data + derivations: no React, no engine.
   Snapshot prices are $ per million input/output tokens; one photo is estimated
   at ~900 tokens in + ~250 out. An unknown live model's price is reported as
   "varies" rather than invented. The legacy two-zone helpers above are kept so
   the existing test suite (and any other caller) stays valid; the rebuilt card
   uses only the exports below.
   ════════════════════════════════════════════════════════════════════════ */

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
      { id: 'glm-5.3-flash', role: 'vision', img: true, pM: 0.15, cM: 0.5, efforts: ['low', 'high', 'max'], def: 'high', on: true },
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
      { id: 'tencent/hy4', role: 'vision', img: true, pM: 0.417, cM: 1.2505, efforts: ['high', 'low', 'none'], def: 'high', on: false },
      { id: 'typesafe/jev-router', role: 'decisions', img: true, pM: -1, cM: -1, efforts: ['auto', 'low', 'medium', 'high'], def: 'auto', on: true },
      { id: 'openrouter/auto', role: 'decisions', img: true, pM: -1, cM: -1, efforts: ['auto', 'low', 'medium', 'high'], def: 'auto', on: false },
      { id: 'openrouter/free', role: 'decisions', img: true, pM: 0, cM: 0, on: false },
    ],
  },
  ollama: {
    name: 'Local Ollama', tag: 'Your network', dot: 'amber', kind: 'host', decisions: false, thinking: true,
    note: 'A host on your network. Its vision models join car reading and moments.',
    models: [
      { id: 'qwen3-vl:8b', role: 'vision', local: true, img: true, variants: ['q4_K_M', 'q8_0'], on: true },
      { id: 'qwen3-vl:4b', role: 'vision', local: true, img: true, variants: ['q8_0'], on: true },
      { id: 'llava:7b', role: 'vision', local: true, img: true, variants: ['v1.6'], on: false },
    ],
  },
  gemini: {
    name: 'Gemini', tag: '¢ per photo', dot: 'amber', kind: 'key', decisions: true, thinking: true,
    keyPlaceholder: 'Paste your Google AI key',
    note: "Google's models, billed to your Google AI key. They read photos and decide.",
    models: [
      { id: 'gemini-4-hydrogen', role: 'both', img: true, pM: 1.5, cM: 7.5, efforts: ['xhigh', 'high', 'medium', 'low'], def: 'medium', on: true },
      { id: 'gemini-3.8-flash', role: 'both', img: true, pM: 0.825, cM: 4.125, efforts: ['high', 'medium', 'low'], def: 'medium', on: false },
    ],
  },
  anthropic: {
    name: 'Anthropic', tag: '¢ per photo', dot: 'amber', kind: 'key', decisions: true, thinking: true,
    keyPlaceholder: 'Paste your Anthropic key',
    note: 'Claude models, billed to your Anthropic key. They read photos and decide.',
    models: [
      { id: 'claude-haiku-5.5', role: 'both', img: true, pM: 0.1, cM: 0.5, efforts: ['max', 'xhigh', 'high', 'medium', 'low', 'none'], def: 'medium', on: true },
      { id: 'claude-sonnet-5.5', role: 'both', img: true, pM: 2, cM: 10, efforts: ['max', 'xhigh', 'high', 'medium', 'low'], def: 'high', on: false },
    ],
  },
  openai: {
    name: 'OpenAI', tag: '¢ per photo', dot: 'amber', kind: 'key', decisions: true, thinking: true,
    keyPlaceholder: 'Paste your OpenAI key',
    note: 'GPT models, billed to your OpenAI key. They read photos and decide.',
    models: [
      { id: 'gpt-6-luna', role: 'both', img: true, pM: 0.1, cM: 0.5, efforts: ['max', 'xhigh', 'high', 'medium', 'low', 'none'], def: 'medium', on: true },
      { id: 'gpt-6.1-terra', role: 'both', img: true, pM: 1, cM: 5, efforts: ['max', 'xhigh', 'high', 'medium', 'low', 'none'], def: 'medium', on: false },
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

/** The always-present built-in card: Conrod's own bundled models on this PC. */
export const BUILTIN = {
  name: 'Built-in models (This PC)', tag: 'Own PC',
  note: 'Ready on this PC. Nothing is uploaded.',
  models: [
    { id: 'qwen3-vl:8b', role: 'vision', group: 'Reads photos', variants: ['q4_K_M', 'q8_0'], on: true },
    { id: 'pplx-decider-local', role: 'decisions', group: 'Makes decisions', dl: '~5 GB', on: false },
  ],
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
export const PRICE_BY_ID = (() => {
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
/** One picked model's cost object, or null when it is a local model. */
export function photoCost(id) {
  if (PRICE_BY_ID[id] === undefined) return { varies: true }; // honest: unknown price
  const m = PRICE_BY_ID[id];
  if (m.pM === 0 && m.cM === 0) return { free: true };
  if (m.pM < 0 || m.cM < 0) return { varies: true };
  return { usd: (IN_TOK * m.pM + OUT_TOK * m.cM) / 1e6 };
}
/** Card cost line: per photo AND per 1,000, always "est." unless free.
 *  `picked` = [{ id, local }]. */
export function costLine(picked = []) {
  if (!picked.length) return 'No models picked yet — no cost.';
  if (picked.every((p) => p.local)) return 'Free — runs on your own hardware.';
  const costs = picked.filter((p) => !p.local).map((p) => photoCost(p.id));
  if (costs.some((c) => c && c.varies)) return 'est. varies per photo · varies per 1,000 photos — the router’s pick sets the price.';
  const usd = costs.reduce((s, c) => s + (c && c.usd ? c.usd : 0), 0);
  if (usd === 0) return 'est. $0 per photo · $0 per 1,000 photos (free tier).';
  return 'est. $' + fmtUsd(usd, true) + ' per photo · $' + fmtUsd(usd * 1000, false) + ' per 1,000 photos';
}

/* ---- a service card, with a per-card Thinking flag that defaults OFF ---- */
export function visionModels(type) {
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
  const def = SERVICE_TYPES[type];
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
export function cardReady(card) {
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
export const SERVICE_PROVIDER_OF = {
  opencode: 'opencode', openrouter: 'openrouter', ollama: 'ollama',
  gemini: 'gemini', anthropic: 'anthropic', openai: 'openai',
};
export function providerToType(provider) {
  const p = String(provider || '').toLowerCase();
  if (p === 'opencode-go') return 'opencode';
  return SERVICE_PROVIDER_OF[p] || null; // in_process / local = built-in (separate)
}
export function cardKeyFor(entry) {
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
export const VISION_HELPER = 'reads-cars';
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
  if (list.length) return { text: 'Needs a key', cls: 'locked', dot: 'amber' };
  return { text: 'No model', cls: 'off', dot: 'off' };
}

/* ---- pool count + one-line summary strip ---- */
export function poolCount(ctx = {}) {
  let n = (ctx.builtinVision || []).length + (ctx.builtinDecisions || []).length;
  let svc = n ? 1 : 0;
  for (const c of (ctx.cards || [])) {
    if (!c.enabled) continue;
    let joined = 0;
    if (c.visionSel && c.visionSel !== 'none') joined++;
    if (c.decisionSel && c.decisionSel !== 'none') joined++;
    if (joined) { svc++; n += joined; }
  }
  return { n, svc };
}
export function poolLabel(p) {
  return p.n
    ? `<b>${p.n}</b> model${p.n === 1 ? '' : 's'} in the pool · ${p.svc} service${p.svc === 1 ? '' : 's'}`
    : 'Nothing in the pool yet';
}
export function summaryText(ctx = {}) {
  const p = poolCount(ctx);
  const routing = (SHARING.find((s) => s.id === ctx.routing) || SHARING[0]).name.toLowerCase();
  if (!p.n) return { text: 'No models in the pool. Enable a service and pick its models.', dot: 'amber' };
  return { text: `${p.n} model${p.n === 1 ? '' : 's'} ready across ${p.svc} service${p.svc === 1 ? '' : 's'}, shared ${routing}.`, dot: 'ok' };
}

