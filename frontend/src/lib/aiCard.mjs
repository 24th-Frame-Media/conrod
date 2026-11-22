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
