import test from 'node:test';
import assert from 'node:assert/strict';
import {
  ADD_ORDER, BUILTIN, DECIDER_QUANTS, HELPERS, IN_TOK, OUT_TOK, SERVICE_TYPES, SHARING,
  SIMPLE_CONTROLS, THINK_HINT,
  addService, builtinRowStates, builtinVisionEnabled, cardsFromDraft, checkOutcome, costLine, decisionCandidates,
  decisionModels, defaultThinking, downloadProgress, fmtUsd, helperStatus, isPicked, makeCard,
  modelLine, momentsStatus, patchService, photoCost, poolCount, providerToType, removeService,
  servingFor, setBuiltinVision, setSharing, sharingValue, summaryText, visionCandidates,
} from '../src/lib/aiCard.mjs';

test('the card is exactly seven helpers, with plain names, no model words at rest, and real setting keys', () => {
  assert.equal(HELPERS.length, 7);
  assert.deepEqual(HELPERS.map((h) => h.id), [
    'cars-people', 'sharpness', 'faces', 'plates', 'race-numbers', 'reads-cars', 'moments',
  ]);
  for (const h of HELPERS) {
    assert.ok(h.title && h.desc);
    // No visible row title leaks engine names.
    assert.doesNotMatch(h.title, /YOLO|OCR|VLM|model|provider|quant|endpoint/i);
    assert.ok(Array.isArray(SIMPLE_CONTROLS[h.id]) && SIMPLE_CONTROLS[h.id].length > 0, `${h.id} has real setting keys`);
  }
  assert.deepEqual(HELPERS.filter((h) => h.capability).map((h) => h.capability), ['vision', 'decision']);
});

test('service definitions: six addable services, built-in rows, vision-only Ollama, real decider quants', () => {
  assert.equal(ADD_ORDER.length, 6);
  assert.equal(BUILTIN.models.length, 2);
  assert.equal(SERVICE_TYPES.ollama.decisions, false);
  assert.ok(THINK_HINT.length > 0);
  assert.equal(providerToType('opencode-go'), 'opencode');
  assert.equal(providerToType('in_process'), null); // built-in, not a service card
  assert.deepEqual(DECIDER_QUANTS.map((q) => q.id), ['auto', 'int8', 'int4']);
  assert.ok(decisionModels('opencode').length > 0);
  assert.ok(decisionModels('openrouter').length > 0);
  assert.deepEqual(decisionModels('ollama'), []);
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
    builtinVision: [], builtinDecisions: ['pplx-decider-local'],
    bundled: { 'cars-people': [{ id: 'Vehicle detector (YOLO)', ready: true }, { id: 'Person detector', ready: true }] },
    cards: [{ type: 'opencode', enabled: true, visionSel: 'glm-5.3-flash', decisionSel: 'jev-1.13', apiKey: 'k' }],
  };
  assert.deepEqual(servingFor('cars-people', ctx).map((x) => x.id), ['Vehicle detector (YOLO)', 'Person detector']);
  assert.deepEqual(servingFor('reads-cars', ctx).map((x) => x.id), ['glm-5.3-flash']);
  assert.deepEqual(servingFor('moments', ctx).map((x) => x.id), ['pplx-decider-local', 'jev-1.13']);
});

test('helper status: ready / not installed / no model / off, no cloud wording', () => {
  assert.equal(helperStatus(false, [{ id: 'x', ready: true }]).text, 'Off');
  assert.equal(helperStatus(true, [{ id: 'x', ready: true }]).text, 'Ready');
  assert.equal(helperStatus(true, [{ id: 'x', ready: false }]).text, 'Not installed');
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
    builtinVision: [], builtinDecisions: ['pplx-decider-local'], routing: 'round_robin',
    cards: [{ enabled: true, visionSel: 'glm-5.3-flash', decisionSel: 'none' }],
  };
  assert.deepEqual(poolCount(ctx), { n: 2, svc: 2 });
  assert.match(summaryText(ctx).text, /2 models ready across 2 services, shared round robin/);
  assert.equal(poolCount({ ...ctx, cards: [], builtinDecisions: [] }).n, 0);
  assert.match(summaryText({ ...ctx, cards: [], builtinVision: [], builtinDecisions: [] }).text, /No models in the pool/);
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
  ]) {
    assert.equal(checkOutcome(good).ready, true);
    assert.equal(checkOutcome(good).reason, '');
  }
});

/* ── built-in rows read the real manifest + use_decider ─────────────────── */
test('built-in rows: downloaded is disk truth, on follows the setting, matched by row id', () => {
  const missing = builtinRowStates([
    { id: 'detector', name: 'Detector', file: 'yolo26m-960.onnx', ready: true, downloaded: true },
    { id: 'builtin-vision', name: 'Vision model (built-in)', file: 'Qwen3VL-8B-Instruct-Q4_K_M.gguf', ready: false, downloaded: false },
    { id: 'decider', name: 'Decision model', file: 'decider-flash-v1.2-q8_0.gguf', ready: false, downloaded: false, optional: true },
  ], false);
  assert.deepEqual(missing['qwen3-vl:8b'], { downloaded: false, on: false, available: true });
  assert.deepEqual(missing['pplx-decider-local'], { downloaded: false, on: false, available: true });

  // The decider IS on disk but switched off: ready=false, downloaded=true.
  // The row must show its switch (off), never a "not downloaded" Download.
  const installedOff = builtinRowStates([
    { id: 'builtin-vision', name: 'Vision model (built-in)', ready: true, downloaded: true },
    { id: 'decider', name: 'Decision model', ready: false, downloaded: true, optional: true },
  ], false);
  assert.deepEqual(installedOff['pplx-decider-local'], { downloaded: true, on: false, available: true });
  assert.deepEqual(installedOff['qwen3-vl:8b'], { downloaded: true, on: true, available: true });

  // Installed AND active: the switch is on.
  const installedOn = builtinRowStates([
    { id: 'builtin-vision', ready: true, downloaded: true },
    { id: 'decider', ready: true, downloaded: true, optional: true },
  ], true);
  assert.deepEqual(installedOn['pplx-decider-local'], { downloaded: true, on: true, available: true });
  assert.equal(builtinRowStates([], false)['qwen3-vl:8b'].downloaded, false);
});

test('a cloud "Vision model" health row is never the built-in vision model', () => {
  // The exact lie from the field: health's service row (file:'vlm', ready:true
  // because vision is disabled) made qwen3-vl:8b read as installed.
  const rows = [
    { id: 'vlm-opencode', name: 'Vision model', file: 'vlm', ready: true, detail: 'Disabled in settings' },
    { id: 'vlm-local', name: 'Ollama (localhost)', file: 'vlm', ready: true },
    { name: 'Vision model', file: 'vlm', ready: true },
  ];
  assert.equal(builtinRowStates(rows, false)['qwen3-vl:8b'].downloaded, false);
  assert.equal(builtinRowStates(rows, false)['pplx-decider-local'].downloaded, false);
  // With the real local row present, only IT decides the state.
  const withLocal = [...rows, { id: 'builtin-vision', ready: false, downloaded: false }];
  assert.equal(builtinRowStates(withLocal, false)['qwen3-vl:8b'].downloaded, false);
});

test('legacy rows without ids still match locally, and unavailable is honest', () => {
  const legacy = builtinRowStates([
    { name: 'Decision model', file: 'decider-flash-v1.2-q8_0.gguf', ready: true },
  ], true);
  assert.equal(legacy['pplx-decider-local'].downloaded, true);
  // No pinned source: the row says not available, with no Download to offer.
  const na = builtinRowStates([
    { id: 'builtin-vision', ready: false, downloaded: false, available: false },
  ], false);
  assert.deepEqual(na['qwen3-vl:8b'], { downloaded: false, on: false, available: false });
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
test('a download completes only when the row flips to downloaded (verified on disk)', () => {
  // The card polls models_status. While the files are not verified on disk,
  // the built-in row must stay in its running/Download state.
  const before = [
    { id: 'builtin-vision', ready: false, downloaded: false },
    { id: 'decider', ready: false, downloaded: false },
  ];
  assert.equal(builtinRowStates(before, false)['qwen3-vl:8b'].downloaded, false);
  assert.equal(builtinRowStates(before, false)['pplx-decider-local'].downloaded, false);
  // A serving "ready" without downloaded (a cloud row) still never completes it.
  assert.equal(builtinRowStates([{ id: 'vlm-x', name: 'Vision model', file: 'vlm', ready: true }], false)['qwen3-vl:8b'].downloaded, false);
  // Once the installer verifies the hashes and the row flips, only then does
  // the row show its switch (the real completion condition).
  const after = [
    { id: 'builtin-vision', ready: true, downloaded: true },
    { id: 'decider', ready: false, downloaded: true },
  ];
  assert.equal(builtinRowStates(after, true)['qwen3-vl:8b'].downloaded, true);
  assert.equal(builtinRowStates(after, true)['pplx-decider-local'].downloaded, true);
});

/* ── download progress is real bytes, indeterminate only when unknown ────── */
test('downloadProgress surfaces real totals and never invents a percent', () => {
  const p = downloadProgress(1_610_612_736, 6_186_814_624);
  assert.equal(p.known, true);
  assert.equal(p.pct, 26);
  assert.match(p.label, /1\.50 GB of 5\.76 GB · 26%/);
  assert.deepEqual(downloadProgress(0, 0), { known: false, pct: null, label: 'Downloading…' });
  assert.equal(downloadProgress(50, 0).known, false);
  assert.equal(downloadProgress(6_186_814_624, 6_186_814_624).pct, 100);
  // Over-read (resume accounting) clamps, never exceeds 100.
  assert.equal(downloadProgress(7e9, 6_186_814_624).pct, 100);
});

test('built-in vision toggle writes provider:builtin to vlm_services without creating an external card', () => {
  const base = { vlm_services: [{ id: 'vlm-1', provider: 'ollama', model: 'qwen3-vl:8b', host: 'http://127.0.0.1:11434', enabled: false }] };
  assert.equal(builtinVisionEnabled(base), false);

  const onList = setBuiltinVision(base, true);
  assert.equal(onList.length, 2);
  assert.equal(onList[1].provider, 'builtin');
  assert.equal(onList[1].enabled, true);
  assert.equal(builtinVisionEnabled({ vlm_services: onList }), true);
  // External cards list excludes provider:'builtin' (it lives on the Built-in card).
  assert.equal(cardsFromDraft({ vlm_services: onList }).length, 1);

  const offList = setBuiltinVision({ vlm_services: onList }, false);
  assert.equal(builtinVisionEnabled({ vlm_services: offList }), false);

  const rows = [{ id: 'builtin-vision', ready: true, downloaded: true }];
  assert.equal(builtinRowStates(rows, false, false)['qwen3-vl:8b'].on, false);
  assert.equal(builtinRowStates(rows, false, true)['qwen3-vl:8b'].on, true);
});
