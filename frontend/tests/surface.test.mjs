import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildRunSnapshot, stageFailureReason, failedStages,
} from '../src/lib/runSnapshot.mjs';
import {
  buildSurface, IDENTIFY_SUBSTAGES, FLOW_ORDER, STAGE_WORDS, STAGE_TOGGLES,
  CONTROL_ACTIONS, controlSpec, stageWord, stageEnabled, offReason, offShortcut,
  skipConsequence, vramView, concurrencyView, modelsLocal, typeSuggestionView,
} from '../src/lib/surface.mjs';
import { subjectWords } from '../src/lib/subjects.mjs';
import { applyOverrides, defaultSummary } from '../src/lib/presetPolicy.mjs';
import {
  SEEDS, resolvedSettings, bothLocalSettings, VRAM_FITS, VRAM_SERIAL, nineStages,
} from '../src/lib/mockSeeds.mjs';

const snap = (name) => buildRunSnapshot(SEEDS[name]());
const surface = (name) => snap(name).surface;
const ids = (s) => s.nodes.map((n) => n.id);
const node = (s, id) => s.nodes.find((x) => x.id === id);
const subject = subjectWords(applyOverrides(defaultSummary('motorsport')));

// ---- Flowchart sub-nodes (§6.8, §1) ----

test('the flowchart replaces the single identify block with three sub-nodes in wire order', () => {
  const s = surface('subNodes');
  assert.deepEqual(ids(s), [...FLOW_ORDER]);
  for (const d of IDENTIFY_SUBSTAGES) {
    const n = node(s, d.id);
    assert.ok(n, `${d.id} is drawn`);
    assert.equal(n.label, d.label);
    assert.equal(n.key, d.key);
  }
  assert.equal(s.nodes.findIndex((x) => x.id === 'identify:decision') < s.nodes.findIndex((x) => x.id === 'identify:vision'), true,
    'Decision runs BEFORE Vision');
  assert.equal(s.nodes.some((n) => n.id === 'identify'), false, 'no single identify block survives');
});

test('every sub-node carries one of the six status words and an honest, phase-gated tick', () => {
  const s = surface('subNodes');
  for (const n of s.identify) {
    assert.ok(STAGE_WORDS.includes(n.word), `${n.id} has a canonical word`);
    assert.equal(n.honestCheck, n.word === 'done', '✓ only when the stage reported done');
  }
  // OCR is running, Decision/Vision pending: neither may claim done.
  assert.equal(node(s, 'identify:ocr').word, 'running');
  assert.equal(node(s, 'identify:ocr').honestCheck, false);
  assert.equal(node(s, 'identify:vision').word, 'pending');
});

test('a done stage ticks; paused and skipped read honestly, never as done/active', () => {
  const paused = surface('pausedStage');
  assert.equal(node(paused, 'identify:decision').word, 'paused');
  assert.equal(node(paused, 'identify:decision').honestCheck, false);
  assert.equal(node(paused, 'identify:ocr').word, 'done');
  assert.equal(node(paused, 'identify:ocr').honestCheck, true);
  const skipped = surface('skippedStage');
  assert.equal(node(skipped, 'identify:vision').word, 'skipped');
  assert.equal(node(skipped, 'identify:vision').honestCheck, false);
  assert.equal(node(skipped, 'identify:vision').state, 'skipped');
});

test('stageWord reads B1 status and falls back to legacy counters', () => {
  assert.equal(stageWord({ status: 'paused' }), 'paused');
  assert.equal(stageWord({ status: 'skipped' }), 'skipped');
  assert.equal(stageWord({ status: 'failed', error: 'x' }), 'failed');
  assert.equal(stageWord({ total: 10, done: 10, workers_busy: 0, workers_total: 2 }), 'done');
  assert.equal(stageWord({ total: 10, done: 4, workers_busy: 1, workers_total: 2 }), 'running');
  assert.equal(stageWord({ total: 10, done: 4, workers_busy: 1, workers_total: 2 }, { paused: true }), 'paused');
});

// ---- Controls wire to B2's engine API (§2) ----

test('controls call EXACTLY stage_pause / stage_skip / stage_rerun with {jobId, stage}', () => {
  const s = surface('subNodes');
  const running = node(s, 'identify:ocr');
  assert.deepEqual(running.controls.pause.action, 'stage_pause');
  assert.deepEqual(running.controls.pause.args, { jobId: 7, stage: 'identify:ocr' });
  assert.deepEqual(node(s, 'identify:vision').controls.skip.action, 'stage_skip');
  assert.deepEqual(node(s, 'identify:vision').controls.skip.args, { jobId: 7, stage: 'identify:vision' });
  assert.equal(CONTROL_ACTIONS.rerun, 'stage_rerun');
  assert.deepEqual(controlSpec('rerun', 7, 'identify:vision'), { action: 'stage_rerun', args: { jobId: 7, stage: 'identify:vision' } });
});

test('Pause only on a running node; Run again on a finished/skipped/paused node; Skip needs a consequence', () => {
  const s = surface('subNodes');
  assert.equal(node(s, 'identify:ocr').controls.pause.available, true, 'running node can pause');
  assert.equal(node(s, 'identify:vision').controls.pause.available, false, 'pending node cannot pause');
  assert.equal(node(s, 'identify:vision').controls.rerun.available, false, 'pending node cannot re-run');
  const paused = surface('pausedStage');
  assert.equal(node(paused, 'identify:decision').controls.pause.available, false, 'a paused node is not re-paused');
  assert.equal(node(paused, 'identify:decision').controls.rerun.available, true, 'a paused node can run again');
  const skipped = surface('skippedStage');
  assert.equal(node(skipped, 'identify:vision').controls.skip.available, false, 'cannot skip an already-skipped node');
  assert.equal(node(skipped, 'identify:vision').controls.rerun.available, true);
  // The plain consequence comes from the snapshot's counters, shown before confirming.
  const vision = node(s, 'identify:vision');
  assert.match(vision.controls.skip.consequence, /Skipping Vision/);
  assert.match(vision.controls.skip.consequence, /no make\/model/i);
  assert.match(vision.controls.skip.consequence, /100/, 'remaining count comes from the snapshot');
});

test('skip consequences are plain and derived, one line each', () => {
  assert.match(skipConsequence('ocr', { subject, remaining: 12 }), /Skipping OCR/);
  assert.match(skipConsequence('decision', { subject, remaining: 12 }), /no burst peaks/i);
  assert.match(skipConsequence('vision', { subject, remaining: 12 }), /12 vehicles/);
  assert.equal(skipConsequence('vision', { subject, remaining: 0 }).includes('0 vehicles'), true);
});

// ---- Greyed disabled steps (§6.3) ----

test('a step off for this album is GREYED with a plain reason and an enable shortcut', () => {
  const s = surface('greyedOff');
  for (const key of ['ocr', 'decision', 'vision']) {
    const n = s.identify.find((x) => x.key === key);
    assert.equal(n.off, true, `${key} is greyed`);
    assert.equal(n.word, 'skipped');
    assert.equal(n.state, 'off');
    assert.equal(n.offReason, offReason(key));
    assert.equal(n.offReason, `${key === 'ocr' ? 'OCR' : key === 'decision' ? 'Decision' : 'Vision'} is off for this album`);
    assert.equal(n.offShortcut.target, 'manage-shoot');
    assert.equal(n.offShortcut.settingsKey, IDENTIFY_SUBSTAGES.find((d) => d.key === key).settingsKey);
    assert.equal(n.workersVisible, false, 'a greyed step never shows workers');
    assert.equal(n.meta, '', 'a greyed step never invents counters');
    assert.equal(n.honestCheck, false, 'a greyed step is never ticked');
    assert.equal(n.controls.pause.available, false);
    assert.equal(n.controls.skip.available, false);
    assert.equal(n.controls.rerun.available, false);
  }
  assert.equal(s.stageTotal, 6, 'greyed steps never count toward progress (read/detect/sharpness/faces/group/pick)');
});

test('offForAlbum is derived from RESOLVED settings, not the engine status', () => {
  assert.equal(stageEnabled('ocr', resolvedSettings({ identify_ocr: false })), false);
  assert.equal(stageEnabled('ocr', resolvedSettings({ read_plates: false, read_numbers: false, read_text: false })), false);
  assert.equal(stageEnabled('ocr', resolvedSettings()), true);
  assert.equal(stageEnabled('vision', resolvedSettings({ use_vlm: false })), false);
  assert.equal(stageEnabled('decision', resolvedSettings({ use_decider: false, moments_backend: 'vlm' })), false);
  assert.equal(stageEnabled('decision', resolvedSettings({ use_decider: false, moments_backend: 'vlm', moments_services: [{ enabled: true }] })), true);
  // Settings say ON but the engine reports a runtime skip: NOT greyed, honest skipped.
  const runtimeSkipped = buildSurface(buildRunSnapshot(SEEDS.skippedStage()), {
    settings: resolvedSettings(), album: null, identifyVram: VRAM_FITS(),
  });
  assert.equal(node(runtimeSkipped, 'identify:vision').off, false);
  assert.equal(node(runtimeSkipped, 'identify:vision').word, 'skipped');
});

test('settings matrix: every combination of the three toggles renders the right greyed/active nodes', () => {
  for (let mask = 0; mask < 8; mask += 1) {
    const settings = resolvedSettings({
      identify_ocr: Boolean(mask & 1),
      use_decider: Boolean(mask & 2), moments_backend: mask & 2 ? 'decider' : 'vlm',
      use_vlm: Boolean(mask & 4),
      moments_services: [], vlm_services: [],
    });
    const stages = nineStages({
      'identify:ocr': { status: mask & 1 ? 'running' : 'skipped', workers_total: mask & 1 ? 2 : 0, total: mask & 1 ? 100 : 0 },
      'identify:decision': { status: mask & 2 ? 'running' : 'skipped', workers_total: mask & 2 ? 2 : 0, total: mask & 2 ? 100 : 0 },
      'identify:vision': { status: mask & 4 ? 'running' : 'skipped', workers_total: mask & 4 ? 2 : 0, total: mask & 4 ? 100 : 0 },
    });
    const input = SEEDS.subNodes();
    input.liveByJob[7].progress.pipeline.stages = stages;
    input.settings = settings;
    const s = buildRunSnapshot(input).surface;
    const bits = [mask & 1, mask & 2, mask & 4];
    s.identify.forEach((n, i) => {
      assert.equal(n.off, !bits[i], `mask ${mask}: ${n.key} greyed matches its setting`);
      if (bits[i]) {
        assert.equal(n.word, 'running');
        assert.equal(n.workersVisible, true);
      } else {
        assert.equal(n.offReason, offReason(n.key));
      }
    });
    assert.equal(s.stageTotal, 6 + bits.filter(Boolean).length, `mask ${mask}: off steps excluded from progress`);
  }
});

test('a greyed node never appears as a filter (no phantom workers)', () => {
  const s = surface('greyedOff');
  assert.equal(s.identify.every((n) => n.filter === null), true);
  assert.equal(s.identify.every((n) => n.workersVisible === false), true);
});

// ---- Settings concurrency + VRAM warning (§5) ----

test('VRAM warning has the real numbers when only one model fits, and explains serial mode', () => {
  const v = vramView(VRAM_SERIAL());
  assert.equal(v.fitsBoth, false);
  assert.equal(v.mode, 'serial', 'one-model VRAM forces serial');
  assert.equal(v.showWarning, true);
  assert.match(v.warning, /4\.8 GB/);
  assert.match(v.warning, /6\.0 GB/);
  assert.match(v.warning, /8\.0 GB/);
  assert.match(v.serialExplanation, /finishes every frame with the first model before loading the second/i);
  const s = surface('serialVram');
  assert.equal(s.vram.showWarning, true);
  assert.equal(s.concurrency.mode, 'serial');
});

test('no warning when both fit; a fit reported by bytes alone is honoured', () => {
  const fits = vramView(VRAM_FITS());
  assert.equal(fits.fitsBoth, true);
  assert.equal(fits.showWarning, false);
  assert.equal(fits.warning, '');
  assert.equal(vramView({ cardBytes: 20 * 2 ** 30, decisionBytes: 4 * 2 ** 30, visionBytes: 5 * 2 ** 30 }).fitsBoth, true);
  assert.equal(vramView(null).fitsBoth, true, 'no contract yet = no false alarm');
});

test('concurrency seats are shown ONLY when both decision and vision run locally', () => {
  const local = concurrencyView(VRAM_FITS(), bothLocalSettings());
  assert.equal(modelsLocal(bothLocalSettings()).bothLocal, true);
  assert.equal(local.visible, true);
  assert.equal(local.seats.vision, 2);
  assert.equal(local.seats.decision, 1);
  assert.equal(local.seats.visionKey, 'identify_vision_workers');
  assert.equal(local.seats.decisionKey, 'identify_decision_workers');
  // A cloud decision service: the preference must not appear.
  const cloud = concurrencyView(VRAM_FITS(), resolvedSettings({ moments_transport: 'opencode' }));
  assert.equal(modelsLocal(resolvedSettings({ moments_transport: 'opencode' })).decisionLocal, false);
  assert.equal(cloud.visible, false);
  const s = surface('subNodes');
  // The default mock uses a cloud opencode service → not both-local.
  assert.equal(s.concurrency.visible, false);
});

test('serial mode is explained plainly even when both models fit but serial is chosen', () => {
  const c = concurrencyView({ ...VRAM_FITS(), mode: 'serial' }, bothLocalSettings());
  assert.equal(c.mode, 'serial');
  assert.match(c.explanation, /Stages stay in order/i);
});

// ---- Type-suggestion accept (§3b, §6.8, item 6) ----

test('an auto album SHOWS the suggestion + confidence, never applies it silently', () => {
  const s = surface('typeSuggestion');
  assert.equal(s.suggestion.isAuto, true);
  assert.equal(s.suggestion.shown, true);
  assert.equal(s.suggestion.type, 'sport-rodeo');
  assert.equal(s.suggestion.confidencePct, 82);
  assert.equal(s.suggestion.accepted, false);
  assert.equal(s.suggestion.canAccept, true);
  assert.match(s.suggestion.note, /only changes the vision reads that have not run yet/i);
  // It rides on the DECISION node, where the type round happened.
  assert.equal(node(s, 'identify:decision').suggestion.shown, true);
  assert.equal(node(s, 'identify:vision').suggestion, null);
});

test('a typed album and an accepted suggestion show nothing; declining keeps the stored type', () => {
  assert.equal(typeSuggestionView({ scan_profile: 'motorsport', shoot_type_suggested: 'sport-rodeo' }).shown, false);
  assert.equal(typeSuggestionView({ scan_profile: 'auto', shoot_type_suggested: 'x', shoot_type_confidence: 0.9, shoot_type_accepted: 'x' }).shown, false);
  assert.equal(typeSuggestionView({ scan_profile: 'auto', shoot_type_suggested: null }).shown, false);
  const declined = typeSuggestionView({ scan_profile: 'auto', shoot_type_suggested: 'sport-rodeo', shoot_type_confidence: 0.82 });
  assert.equal(declined.accepted, false, 'showing is not accepting');
  assert.equal(declined.type, 'sport-rodeo');
});

test('the suggestion label comes from the preset table, not a hardcoded shoot word', () => {
  const s = typeSuggestionView({ scan_profile: 'auto', shoot_type_suggested: 'sport-rodeo', shoot_type_confidence: 0.82 });
  assert.equal(s.typeLabel, 'Rodeo');
});

// ---- SSOT: failures, ordering, stage-agnostic unknown ids ----

test('B1 wire-shape failures surface with their plain reason (status/error, not state/reason)', () => {
  const seed = SEEDS.failedStageWire();
  const reason = stageFailureReason(seed.liveByJob[7].progress.pipeline.stages[1]);
  assert.equal(reason, 'The vision service stopped answering.');
  const s = buildRunSnapshot(seed);
  assert.equal(s.failedStages.length, 1);
  assert.equal(s.failedStages[0].id, 'identify:vision');
  assert.equal(s.failedStages[0].reason, 'The vision service stopped answering.');
  assert.equal(failedStages([{ status: 'running' }]).length, 0);
});

test('unknown stage ids from a newer engine still render (stage-agnostic, §6.1)', () => {
  const input = SEEDS.subNodes();
  input.liveByJob[7].progress.pipeline.stages.push({ id: 'reveal', label: 'Reveal', status: 'pending', workers_total: 1, done: 0, total: 10 });
  const s = buildRunSnapshot(input);
  assert.equal(s.stages.some((x) => x.id === 'reveal'), true);
  assert.equal(s.surface.nodes.some((x) => x.id === 'reveal'), true);
});

test('buildRunSnapshot still carries B4 plumbing fields alongside the B5 surface', () => {
  const s = snap('subNodes');
  assert.equal(s.activeJob, 7);
  assert.equal(s.runActive, true);
  assert.equal(s.badge.word, 'Running');
  assert.ok(s.surface, 'the surface is built ON the snapshot');
  assert.equal(s.surface.jobId, s.activeJob);
});

test('Manage shoot exposes OCR / Decision / Vision toggles wired to B1 settings', () => {
  const s = surface('greyedOff');
  assert.deepEqual(s.toggles.map((t) => t.key), ['ocr', 'decision', 'vision']);
  assert.deepEqual(s.toggles.map((t) => t.settingsKey), ['identify_ocr', 'use_decider', 'use_vlm']);
  assert.equal(s.toggles.every((t) => t.on === false), true, 'all three off for the greyed album');
  assert.deepEqual(STAGE_TOGGLES.map((t) => t.key), ['ocr', 'decision', 'vision']);
});
