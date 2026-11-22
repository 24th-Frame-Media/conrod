import test from 'node:test';
import assert from 'node:assert/strict';
import {
  EMPTY_FORK, HELPERS, NOT_SET_UP, STRATS, SUMMARY_LINES, ZONE_TITLES,
  buildAiView, capStatus, capUsable, groupByZone, isEmpty, serviceConfig,
  serving, setService, statusOf, summaryLine, unlockReason, zoneOf,
} from '../src/lib/aiCard.mjs';
import {
  ADD_ORDER, BUILTIN, IN_TOK, OUT_TOK, SERVICE_TYPES, SHARING, THINK_HINT,
  addService, builtinRowStates, cardsFromDraft, checkOutcome, costLine, decisionCandidates, defaultThinking,
  fmtUsd, helperStatus, isPicked, makeCard, modelLine, momentsStatus, patchService, photoCost, poolCount,
  providerToType, removeService, servingFor, setCapModel, setSharing, sharingValue, summaryText,
  visionCandidates,
} from '../src/lib/aiCard.mjs';

const NO_MODELS = { vision: {}, decision: {} };
const view = (settings) => buildAiView(settings, NO_MODELS);

const svc = (provider, extra = {}) => ({ provider, model: 'm', host: '', api_key: '', enabled: true, ...extra });
const settingsWith = (opts = {}) => ({
  ...(opts.use_decider != null ? { use_decider: opts.use_decider } : {}),
  ...(opts.vlm_services ? { vlm_services: opts.vlm_services } : {}),
  ...(opts.moments_services ? { moments_services: opts.moments_services } : {}),
});

const EMPTY = {};
const CLOUD = settingsWith({
  vlm_services: [svc('opencode', { api_key: 'oc-key' })],
  moments_services: [svc('opencode', { api_key: 'oc-key' })],
});
const LOCAL = settingsWith({
  use_decider: true,
  moments_services: [svc('in_process', { api_key: '' })],
});
const OPENROUTER_ONLY = settingsWith({
  vlm_services: [svc('openrouter', { api_key: 'or-key' })],
  moments_services: [svc('openrouter', { api_key: 'or-key' })],
});
const BOTH = settingsWith({
  use_decider: true,
  moments_services: [svc('in_process', { api_key: '' }), svc('opencode', { api_key: 'oc-key' })],
  vlm_services: [svc('opencode', { api_key: 'oc-key' })],
});

const capHelper = (cap) => HELPERS.find((h) => h.capability === cap);

test('the card is exactly seven helpers, with plain names and no model words at rest', () => {
  assert.equal(HELPERS.length, 7);
  assert.deepEqual(HELPERS.map((h) => h.id), [
    'cars-people', 'sharpness', 'faces', 'plates', 'race-numbers', 'reads-cars', 'moments',
  ]);
  for (const h of HELPERS) {
    assert.ok(h.title && h.desc);
    // No visible row title leaks engine names.
    assert.doesNotMatch(h.title, /YOLO|OCR|VLM|model|provider|quant|endpoint/i);
  }
  assert.deepEqual(HELPERS.filter((h) => h.capability).map((h) => h.capability), ['vision', 'decision']);
});

test('zone grouping: empty slots every helper on this PC and leaves the cloud zone empty', () => {
  const v = view(EMPTY);
  const [pc, cloud] = v.groups;
  assert.equal(pc.id, 'pc');
  assert.equal(pc.title, ZONE_TITLES.pc);
  assert.equal(cloud.id, 'cloud');
  assert.equal(cloud.title, ZONE_TITLES.cloud);
  assert.equal(pc.helpers.length, 7);
  assert.equal(cloud.helpers.length, 0);
  assert.match(cloud.empty, /cloud key|download/i);
  for (const h of HELPERS) assert.equal(zoneOf(h, v.ctx), 'pc');
});

test('zone grouping: a cloud key moves the two capable helpers to the cloud zone', () => {
  const v = view(CLOUD);
  const [pc, cloud] = v.groups;
  assert.deepEqual(pc.helpers.map((h) => h.id), ['cars-people', 'sharpness', 'faces', 'plates', 'race-numbers']);
  assert.deepEqual(cloud.helpers.map((h) => h.id), ['reads-cars', 'moments']);
  assert.equal(zoneOf(capHelper('vision'), v.ctx), 'cloud');
  assert.equal(zoneOf(capHelper('decision'), v.ctx), 'cloud');
});

test('zone grouping: local models (and local + cloud) keep the capable helpers on this PC', () => {
  assert.deepEqual(view(LOCAL).groups.map((g) => g.helpers.length), [7, 0]);
  assert.equal(zoneOf(capHelper('vision'), view(LOCAL).ctx), 'pc');
  assert.deepEqual(view(BOTH).groups.map((g) => g.helpers.length), [7, 0]);
  assert.equal(capStatus('vision', view(BOTH).ctx.config, NO_MODELS).text, 'Ready on this PC + cloud');
});

test('statuses at rest: ready on this PC, ready via the service, or not set up', () => {
  const empty = view(EMPTY).ctx;
  assert.equal(statusOf(capHelper('vision'), empty).text, NOT_SET_UP);
  assert.equal(statusOf(capHelper('vision'), empty).tone, 'locked');

  const cloud = view(CLOUD).ctx;
  assert.equal(statusOf(capHelper('vision'), cloud).text, 'Ready via OpenCode');
  assert.equal(statusOf(capHelper('decision'), cloud).tag, 'Cloud');

  const local = view(LOCAL).ctx;
  assert.equal(statusOf(capHelper('vision'), local).text, 'Ready on this PC');

  // Simple helpers reflect their real on/off keys.
  assert.equal(statusOf(HELPERS[0], { ...empty, settings: { include_cars: true } }).text, 'Ready on this PC');
  assert.equal(statusOf(HELPERS[0], { ...empty, settings: { include_cars: false } }).text, 'Off');
});

test('unlock reasons: nothing configured locks both capabilities with the fork copy', () => {
  const ctx = view(EMPTY).ctx;
  for (const cap of ['vision', 'decision']) {
    const u = unlockReason(capHelper(cap), ctx);
    assert.equal(u.locked, true);
    assert.equal(u.reason, NOT_SET_UP);
    assert.match(u.sub, /add a key|download the built-in models/i);
  }
  assert.equal(capUsable('vision', ctx.config), false);
  assert.equal(capUsable('decision', ctx.config), false);
});

test('unlock reasons: a cloud key unlocks both; OpenRouter alone cannot unlock moments', () => {
  const cloud = view(CLOUD).ctx;
  assert.equal(unlockReason(capHelper('vision'), cloud).locked, false);
  assert.equal(unlockReason(capHelper('decision'), cloud).locked, false);

  const orOnly = view(OPENROUTER_ONLY).ctx;
  assert.equal(unlockReason(capHelper('vision'), orOnly).locked, false, 'OpenRouter can read photos');
  const moments = unlockReason(capHelper('decision'), orOnly);
  assert.equal(moments.locked, true, 'its moments model cannot see photos yet');
  assert.match(moments.sub, /can’t see photos yet/i);
  assert.match(moments.sub, /OpenCode key|built-in models/i);

  assert.equal(unlockReason(capHelper('decision'), view(LOCAL).ctx).locked, false);
});

test('empty-fork copy: the two paths and the OpenCode / OpenRouter note', () => {
  assert.equal(isEmpty(serviceConfig(EMPTY)), true);
  assert.equal(isEmpty(serviceConfig(CLOUD)), false);
  assert.equal(isEmpty(serviceConfig(LOCAL)), false);

  assert.match(EMPTY_FORK.heading, /two ways/i);
  assert.match(EMPTY_FORK.download.title, /Download the models/i);
  assert.match(EMPTY_FORK.download.sub, /Free/i);
  assert.match(EMPTY_FORK.download.sub, /5 GB/i);
  assert.match(EMPTY_FORK.download.sub, /offline/i);
  assert.match(EMPTY_FORK.cloud.title, /cloud service/i);
  assert.match(EMPTY_FORK.cloud.sub, /pay as you go/i);
  assert.match(EMPTY_FORK.cloud.sub, /any PC/i);
  assert.match(EMPTY_FORK.cloud.note, /OpenCode covers both car reading and moments/i);
  assert.match(EMPTY_FORK.cloud.note, /OpenRouter/i);
  assert.match(EMPTY_FORK.cloud.note, /moments model can’t see photos yet/i);
});

test('collapsed summary line follows whichever path is done', () => {
  assert.equal(summaryLine(serviceConfig(CLOUD)), SUMMARY_LINES.cloud);
  assert.equal(summaryLine(serviceConfig(LOCAL)), SUMMARY_LINES.local);
  assert.equal(summaryLine(serviceConfig(BOTH)), SUMMARY_LINES.both);
  assert.equal(view(CLOUD).summary, SUMMARY_LINES.cloud);
  assert.equal(view(EMPTY).isEmpty, true);
});

test('work-sharing strategies are the three engine values', () => {
  assert.deepEqual(STRATS.map((s) => s.id), ['least_busy', 'round_robin', 'random']);
});

test('wiring: adding a key or downloading writes vlm_services / moments_services / use_decider', () => {
  const keyed = setService(EMPTY, 'opencode', { key: 'oc-key' });
  assert.equal(keyed.vlm_services[0].provider, 'opencode');
  assert.equal(keyed.vlm_services[0].api_key, 'oc-key');
  assert.equal(keyed.moments_services[0].provider, 'opencode');
  assert.equal(serving('opencode', serviceConfig({ ...keyed, use_decider: keyed.use_decider })), true);

  const downloaded = setService(EMPTY, 'local', { download: true });
  assert.equal(downloaded.use_decider, true);
  assert.equal(downloaded.moments_services[0].provider, 'in_process');
  assert.equal(serving('local', serviceConfig({ ...downloaded })), true);

  const hosted = setService(EMPTY, 'ollama', { addHost: 'http://localhost:11434' });
  assert.equal(hosted.vlm_services[0].host, 'http://localhost:11434');
  assert.deepEqual(serviceConfig({ ...hosted }).ollama.hosts, ['http://localhost:11434']);
});

test('wiring: groupByZone never lists a helper in two zones', () => {
  for (const settings of [EMPTY, CLOUD, LOCAL, OPENROUTER_ONLY, BOTH]) {
    const groups = groupByZone(view(settings).ctx);
    const ids = groups.flatMap((g) => g.helpers.map((h) => h.id));
    assert.equal(new Set(ids).size, ids.length);
    assert.equal(ids.length, HELPERS.length);
  }
});

/* ── services-plan rebuild: the four new behaviours ─────────────────────── */

test('service definitions: six addable services, built-in rows, vision-only Ollama', () => {
  assert.equal(ADD_ORDER.length, 6);
  assert.equal(BUILTIN.models.length, 2);
  assert.equal(SERVICE_TYPES.ollama.decisions, false);
  assert.ok(THINK_HINT.length > 0);
  assert.equal(providerToType('opencode-go'), 'opencode');
  assert.equal(providerToType('in_process'), null); // built-in, not a service card
});

test('sharing control: one global value writes both strategy keys together', () => {
  assert.deepEqual(SHARING.map((s) => s.id), ['round_robin', 'least_busy', 'random']);
  assert.deepEqual(SHARING.map((s) => s.name), ['Round robin', 'Load balance', 'Random']);
  assert.deepEqual(setSharing('least_busy'), { vlm_strategy: 'least_busy', moments_strategy: 'least_busy' });
  assert.deepEqual(setSharing('random'), { vlm_strategy: 'random', moments_strategy: 'random' });
  assert.deepEqual(setSharing('bogus'), { vlm_strategy: 'round_robin', moments_strategy: 'round_robin' });
  assert.equal(sharingValue({ vlm_strategy: 'least_busy' }), 'least_busy');
  assert.equal(sharingValue({}), 'round_robin');
  assert.equal(sharingValue({ vlm_strategy: 'bogus' }), 'round_robin');
});

test('cost line: est. $X per photo · $Y per 1,000, free / varies honestly', () => {
  assert.equal(IN_TOK, 900);
  assert.equal(OUT_TOK, 250);
  assert.equal(photoCost('glm-5.3-flash').usd, (900 * 0.15 + 250 * 0.5) / 1e6);
  assert.equal(photoCost('not-a-known-model').varies, true);
  assert.equal(fmtUsd(0.00026, true), '0.0003');
  assert.equal(fmtUsd(0.26, false), '0.26');
  const line = costLine([{ id: 'glm-5.3-flash', local: false }, { id: 'jev-1.13', local: false }]);
  assert.match(line, /^est\. \$/);
  assert.match(line, /per photo · \$/);
  assert.match(line, /per 1,000 photos$/);
  assert.equal(costLine([]), 'No models picked yet — no cost.');
  assert.match(costLine([{ id: 'qwen3-vl:8b', local: true }]), /Free — runs on your own hardware/);
  assert.match(costLine([{ id: 'openrouter/auto', local: false }]), /varies per photo/);
  assert.match(costLine([{ id: 'jev-1.13-free', local: false }]), /free tier/);
});

test('popup data: the model line lists every serving model behind +N more', () => {
  const list = [
    { id: 'qwen3-vl:8b', ready: true },
    { id: 'glm-5.3-flash', ready: true },
    { id: 'gemini-4-hydrogen', ready: false },
  ];
  const line = modelLine(list);
  assert.equal(line.noModel, false);
  assert.equal(line.head, 'qwen3-vl:8b');
  assert.deepEqual(line.rest, ['glm-5.3-flash', 'gemini-4-hydrogen']);
  assert.deepEqual(line.all, ['qwen3-vl:8b', 'glm-5.3-flash', 'gemini-4-hydrogen']);
  assert.equal(modelLine([]).noModel, true);
});

test('per-card Thinking defaults off, is persisted as reasoning on both service lists', () => {
  assert.equal(defaultThinking(), false);
  for (const type of ADD_ORDER) assert.equal(makeCard(type).reasoning, false, `${type} default`);
  const a = makeCard('opencode'), b = makeCard('gemini');
  a.reasoning = true;
  assert.equal(b.reasoning, false, 'toggling one card does not affect another');
  const added = addService({ vlm_services: [], moments_services: [] }, 'opencode');
  assert.equal(added.vlm_services[0].reasoning, false);
  assert.equal(added.moments_services[0].reasoning, false);
  assert.equal(cardsFromDraft(added)[0].reasoning, false);
  const on = patchService(added, 'opencode', { reasoning: true });
  assert.equal(on.vlm_services[0].reasoning, true);
  assert.equal(on.moments_services[0].reasoning, true);
  assert.equal(cardsFromDraft(on)[0].reasoning, true);
});

test('helper serving: non-AI helpers use bundled models, AI helpers use the pool', () => {
  const ctx = {
    builtinVision: ['qwen3-vl:8b'], builtinDecisions: [],
    bundled: { 'cars-people': [{ id: 'Vehicle detector (YOLO)', ready: true }, { id: 'Person detector', ready: true }] },
    cards: [{ type: 'opencode', enabled: true, visionSel: 'glm-5.3-flash', decisionSel: 'jev-1.13', apiKey: 'k' }],
  };
  assert.deepEqual(servingFor('cars-people', ctx).map((x) => x.id), ['Vehicle detector (YOLO)', 'Person detector']);
  assert.deepEqual(servingFor('reads-cars', ctx).map((x) => x.id), ['qwen3-vl:8b', 'glm-5.3-flash']);
  assert.deepEqual(servingFor('moments', ctx).map((x) => x.id), ['jev-1.13']);
});

test('helper status: ready / needs a key / no model / off, no cloud wording', () => {
  assert.equal(helperStatus(false, [{ id: 'x', ready: true }]).text, 'Off');
  assert.equal(helperStatus(true, [{ id: 'x', ready: true }]).text, 'Ready');
  assert.equal(helperStatus(true, [{ id: 'x', ready: false }]).text, 'Needs a key');
  assert.equal(helperStatus(true, []).text, 'No model');
});

test('wiring: add / patch / remove service rows, one card per service', () => {
  const added = addService({ vlm_services: [], moments_services: [] }, 'opencode');
  const cards = cardsFromDraft(added);
  assert.equal(cards.length, 1);
  assert.equal(cards[0].type, 'opencode');
  assert.equal(cards[0].visionSel, 'glm-5.3-flash');
  assert.equal(cards[0].decisionSel, 'jev-1.13');
  const patched = patchService(added, 'opencode', { apiKey: 'secret', enabled: false, visionSel: 'glm-4.6v' });
  const pc = cardsFromDraft(patched)[0];
  assert.equal(pc.apiKey, 'secret');
  assert.equal(pc.enabled, false);
  assert.equal(pc.visionSel, 'glm-4.6v');
  assert.equal(cardsFromDraft(removeService(patched, 'opencode')).length, 0);
});

test('Ollama = one card per host; OpenRouter decisions render through decision_models', () => {
  const o1 = addService({ vlm_services: [], moments_services: [] }, 'ollama', { id: 'ollama-1', url: 'http://h1:11434' });
  const o2 = addService(o1, 'ollama', { id: 'ollama-2', url: 'http://h2:11434' });
  assert.equal(cardsFromDraft(o2).length, 2);
  // decision_models enumeration (same shape) feeds the same path as OpenCode.
  const dc = decisionCandidates('openrouter', [{ id: 'qwen/qwen2.5-vl-72b-instruct' }],
    [{ id: 'typesafe/jev-router', sees_photos: true }, { id: 'openrouter/auto', sees_photos: true }]);
  assert.equal(dc.freeText, false);
  assert.deepEqual(dc.models.map((m) => m.id), ['typesafe/jev-router', 'openrouter/auto']);
  assert.equal(decisionCandidates('openrouter', [], []).freeText, true, 'honest free-text when nothing matches');
  assert.equal(decisionCandidates('opencode', [], [{ id: 'jev-1.13' }]).models[0].id, 'jev-1.13');
  assert.equal(visionCandidates('openrouter', [{ id: 'typesafe/jev-router' }, { id: 'qwen/qwen2.5-vl-72b-instruct' }]).length, 1);
});

/* ── moments status: one source of truth for "who is doing the work" ────── */

const jevSettings = () => ({
  vlm_services: [{ provider: 'opencode', model: 'glm-5.3-flash', host: '', api_key: 'oc-key', enabled: true }],
  moments_services: [{ provider: 'opencode', model: 'jev-1.13', host: '', api_key: 'oc-key', enabled: true }],
  use_decider: false,
});

test('moments status: a cloud service active shows that service and model, not "not installed"', () => {
  // The local model is NOT installed, yet a cloud service is judging moments.
  const ms = momentsStatus(jevSettings(), false);
  assert.equal(ms.path, 'cloud');
  assert.equal(ms.service, 'OpenCode');
  assert.equal(ms.model, 'jev-1.13');
  assert.equal(ms.label, 'jev-1.13 via OpenCode');
  assert.equal(ms.status, 'Ready');
  assert.equal(ms.ready, true);
  assert.doesNotMatch(`${ms.label} ${ms.status}`, /not installed|off/i);
  // The picked decision model is read from the OpenCode service, not the local in-process seat.
  assert.doesNotMatch(ms.label, /in_process/i);
});

test('moments status: local-only shows the local install state', () => {
  const local = {
    use_decider: true,
    moments_services: [{ provider: 'in_process', model: 'in_process', host: '', api_key: '', enabled: true }],
    vlm_services: [],
  };
  const missing = momentsStatus(local, false);
  assert.equal(missing.path, 'local');
  assert.equal(missing.label, 'On this PC');
  assert.equal(missing.status, 'Not installed');
  assert.equal(missing.ready, false);
  const installed = momentsStatus(local, true);
  assert.equal(installed.path, 'local');
  assert.equal(installed.status, 'Ready');
  assert.equal(installed.ready, true);
  assert.equal(installed.model, 'pplx-decider-local');
});

test('moments status: a cloud service with no key is named but not ready', () => {
  const settings = {
    moments_services: [{ provider: 'opencode', model: 'jev-1.13', host: '', api_key: '', enabled: true }],
    vlm_services: [],
  };
  const ms = momentsStatus(settings, false);
  assert.equal(ms.path, 'cloud');
  assert.equal(ms.label, 'jev-1.13 via OpenCode');
  assert.equal(ms.status, 'Needs a key');
  assert.equal(ms.ready, false);
});

test('moments status: every serving cloud service is named, first is the head', () => {
  const settings = {
    moments_services: [
      { provider: 'opencode', model: 'jev-1.13', host: '', api_key: 'k', enabled: true },
      { provider: 'gemini', model: 'gemini-4-hydrogen', host: '', api_key: 'k', enabled: true },
    ],
    vlm_services: [],
  };
  const ms = momentsStatus(settings);
  assert.equal(ms.label, 'jev-1.13 via OpenCode');
  assert.deepEqual(ms.others, ['gemini-4-hydrogen via Gemini']);
});

test('pool + summary strip follow enabled services and routing', () => {
  const ctx = {
    builtinVision: ['qwen3-vl:8b'], builtinDecisions: [], routing: 'round_robin',
    cards: [{ enabled: true, visionSel: 'glm-5.3-flash', decisionSel: 'none' }],
  };
  assert.deepEqual(poolCount(ctx), { n: 2, svc: 2 });
  assert.match(summaryText(ctx).text, /2 models ready across 2 services, shared round robin/);
  assert.equal(poolCount({ ...ctx, cards: [], builtinVision: [] }).n, 0);
  assert.match(summaryText({ ...ctx, cards: [], builtinVision: [], builtinDecisions: [] }).text, /No models in the pool/);
});

test('the built-in reader never becomes a vision service row', () => {
  // An early version of this card wrote the built-in reader into vlm_services
  // with provider in_process, and every car read then failed with "unknown
  // vision provider 'in_process'".
  const before = settingsWith({ vlm_services: [svc('ollama', { host: 'http://h:1' })] });
  const out = setCapModel(before, 'vision', 'local', 'Built-in car reader');
  assert.deepEqual(out.vlm_services.map((s) => s.provider), ['ollama']);
  assert.equal(out.vlm_services.some((s) => s.provider === 'in_process'), false);
  // A real service still records its model.
  const cloud = setCapModel(before, 'vision', 'opencode', 'glm-5.3-flash');
  assert.equal(cloud.vlm_services.at(-1).provider, 'opencode');
});

/* ── Test button: it must surface the engine's real failure reason ───────── */
test('a real check failure is surfaced; only a clean reply is ready', () => {
  // openrouter_check / openCodeCheck: a bad key resolves with { ok:false, error }.
  const badKey = checkOutcome({ ok: false, error: 'OpenRouter rejected the key (401).' });
  assert.equal(badKey.ready, false);
  assert.match(badKey.reason, /401/);
  // ollama_models: an offline host resolves with { online:false, error }.
  const offline = checkOutcome({ ok: false, online: false, error: 'Could not connect to Ollama: connection refused' });
  assert.equal(offline.ready, false);
  assert.match(offline.reason, /connection refused/);
  // A rejected promise carries the same plain reason (provider_models).
  const thrown = checkOutcome(undefined, new Error('network unreachable'));
  assert.equal(thrown.ready, false);
  assert.match(thrown.reason, /network unreachable/);
  const listed = checkOutcome({ models: [], error: 'the key was rejected' });
  assert.equal(listed.ready, false);
  // Clean replies from every check command are ready, with no reason.
  for (const good of [
    { ok: true, label: 'Conrod dev key' },
    { ok: true, online: true, models: [{ name: 'qwen3-vl:8b' }] },
    { models: [{ id: 'glm-5.3-flash' }] },
    null, // the built-in (on-this-PC) card has no engine check to fail
  ]) {
    assert.equal(checkOutcome(good).ready, true);
    assert.equal(checkOutcome(good).reason, '');
  }
});

/* ── built-in rows read the real manifest + use_decider ─────────────────── */
test('built-in rows: downloaded/on come from the manifest and use_decider, never invented flags', () => {
  const missing = builtinRowStates([
    { name: 'Detector', file: 'det.onnx', ready: true },
    { name: 'Vision model', file: 'qwen', ready: false },
    { name: 'Decision model', file: 'decider-int8.onnx', ready: false, optional: true },
  ], false);
  assert.deepEqual(missing['qwen3-vl:8b'], { downloaded: false, on: false }, 'qwen3-vl:8b reflects the manifest');
  assert.deepEqual(missing['pplx-decider-local'], { downloaded: false, on: false });

  // Installed AND active: the switch is on, not a "not downloaded" Download button.
  const installed = builtinRowStates([
    { name: 'Vision model', file: 'qwen', ready: true },
    { name: 'Decision model', file: 'decider-int8.onnx', ready: true, optional: true },
  ], true);
  assert.deepEqual(installed['qwen3-vl:8b'], { downloaded: true, on: true });
  assert.deepEqual(installed['pplx-decider-local'], { downloaded: true, on: true });

  // Installed but use_decider off: still a switch (downloaded), just off.
  const off = builtinRowStates([
    { name: 'Vision model', ready: true }, { name: 'Decision model', ready: true },
  ], false);
  assert.deepEqual(off['pplx-decider-local'], { downloaded: true, on: false });
  assert.equal(builtinRowStates([], false)['qwen3-vl:8b'].downloaded, false);
});

/* ── "None — skip" is excluded from cost and pool counts ────────────────── */
test('"None — skip" is never billed or counted in the pool or cost', () => {
  assert.equal(isPicked('none'), false);
  assert.equal(isPicked(''), false);
  assert.equal(isPicked('glm-5.3-flash'), true);
  // A stray "none" in picked is not a model (it used to be billed as "varies").
  assert.equal(costLine([{ id: 'none', local: false }]), 'No models picked yet — no cost.');
  assert.equal(costLine([{ id: '', local: false }]), 'No models picked yet — no cost.');
  assert.equal(
    costLine([{ id: 'none', local: false }, { id: 'glm-5.3-flash', local: false }]),
    costLine([{ id: 'glm-5.3-flash', local: false }]),
  );
  const base = { builtinVision: [], builtinDecisions: [], routing: 'round_robin',
    cards: [{ enabled: true, visionSel: 'none', decisionSel: 'none' }] };
  assert.deepEqual(poolCount(base), { n: 0, svc: 0 }, 'a card with both None joins nothing');
  assert.deepEqual(poolCount({ ...base, cards: [{ enabled: true, visionSel: 'glm-5.3-flash', decisionSel: 'none' }] }),
    { n: 1, svc: 1 }, 'None is excluded while the picked model is counted');
  assert.deepEqual(poolCount({ ...base, builtinVision: ['none'] }), { n: 0, svc: 0 });
});

/* ── a download completes only when the model appears in the manifest ────── */
test('a download completes only when the model appears in the real manifest', () => {
  // The card polls health/onModelsChanged. While no ready row exists, the
  // built-in row must stay in its running/Download state, not flip to a switch.
  const before = [
    { name: 'Vision model', file: 'qwen', ready: false },
    { name: 'Decision model', file: 'decider-int8.onnx', ready: false },
  ];
  assert.equal(builtinRowStates(before, false)['qwen3-vl:8b'].downloaded, false);
  assert.equal(builtinRowStates(before, false)['pplx-decider-local'].downloaded, false);
  // Once the installer lands and health reports the row ready, only then does
  // the row flip to its switch (the real completion condition).
  const after = [
    { name: 'Vision model', file: 'qwen', ready: true },
    { name: 'Decision model', file: 'decider-int8.onnx', ready: true },
  ];
  assert.equal(builtinRowStates(after, true)['qwen3-vl:8b'].downloaded, true);
  assert.equal(builtinRowStates(after, true)['pplx-decider-local'].downloaded, true);
});
