import test from 'node:test';
import assert from 'node:assert/strict';
import {
  EMPTY_FORK, HELPERS, NOT_SET_UP, STRATS, SUMMARY_LINES, ZONE_TITLES,
  buildAiView, capStatus, capUsable, groupByZone, isEmpty, serviceConfig,
  serving, setService, statusOf, summaryLine, unlockReason, zoneOf,
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
