import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { buildRunSnapshot, jobChip, runStateWord } from '../src/lib/runSnapshot.mjs';
import { buildSurface, flowOrder, FLOW_ORDER, stageWord, toneFor } from '../src/lib/surface.mjs';
import {
  parkedStageIds, scanPausedFrom, parkedSentence, skippedStageIds, skippedChip, skipHeaderNote, stageName,
} from '../src/lib/stageState.mjs';
import { deciderForJob, settleDecider } from '../src/lib/decider.mjs';
import { ownsActivationKeys, zoomKeyIgnored } from '../src/lib/hotkeyScope.mjs';
import { trapTarget, createSheetStack } from '../src/lib/focusTrap.mjs';
import { runSaveSteps, saveFailureMessage, featureLabel } from '../src/lib/shootSave.mjs';
import { rememberProgress, PROGRESS_KEEP } from '../src/lib/progressCache.mjs';
import { decisionCandidates, decisionFlag, decisionMeta } from '../src/lib/aiCard.mjs';
import { SUBJECT_WORDS } from '../src/lib/subjects.mjs';
import { PARALLEL_NOTE, FIELDS } from '../src/lib/settings.mjs';
import { activeStage, nineStages, resolvedSettings } from '../src/lib/mockSeeds.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const read = (rel) => readFileSync(join(HERE, '..', 'src', rel), 'utf8');

const job = (extra = {}) => ({ id: 7, label: 'Album 7', root: '', status: 'scanning', total: 100, done: 40, errors: 0, created_at: 1, ...extra });
const withSettings = (settings_json) => job({ settings_json: JSON.stringify(settings_json) });
const task = (label, state = 'running') => ({ id: 1, label, detail: '', state, done: 1, total: 10, eta: null, error: null });
const live = (stages) => ({
  7: {
    progress: {
      job_id: 7, state: 'running', stage: 'read', stage_index: 0, stage_count: 6, done: 40, total: 100, rate_per_s: 1, eta_s: 5, recent: [],
      found: { subjects: 0, soft: 0, closed_eyes: 0, numbers: 0, plates: 0, bursts: 0, keepers: 0 },
      pipeline: { stages, workers: [] }, overall: { done: 40, total: 100, fraction: 0.4, rate_per_s: 1, eta_s: 5, eta_confident: false },
    },
    stable: false, history: [], at: 200,
  },
});
const snapshot = ({ tasks, album = job(), stages = nineStages(), settings = resolvedSettings() } = {}) => buildRunSnapshot({
  status: { activeJob: 7, queue: [], log: [], operations: [], tasks },
  jobs: [album], liveByJob: live(stages), hydrate: null, seed: null, jobId: 7, album, settings, now: 60000,
});

// ---- F-P2-3: a parked stage is not a paused scan ----

test('a parked Decision stage pauses "Judging moments" but the scan is not paused', () => {
  const tasks = [task('Scanning'), task('Judging moments', 'paused')];
  const snap = snapshot({ tasks, stages: nineStages({ 'identify:decision': { status: 'paused' } }) });
  assert.equal(snap.paused, false, 'the scan keeps running');
  assert.deepEqual(snap.parkedStages, ['identify:decision']);
  assert.equal(snap.badge.state, 'stage_paused');
  assert.equal(snap.badge.word, 'Stage paused');
  assert.equal(snap.jobs[0].chip.state, 'stage_paused', 'the activity popover row says so too');
  assert.equal(snap.jobs[0].chip.tone, 'paused');
});

test('the album stage controls park a stage when the snapshot field is absent', () => {
  const album = withSettings({ native_stage_pause: ['identify:decision'] });
  const tasks = [task('Scanning'), task('Judging moments', 'paused')];
  const snap = snapshot({ tasks, album, stages: nineStages() });
  assert.deepEqual(snap.parkedStages, ['identify:decision']);
  assert.equal(snap.paused, false);
});

test('an explicit per-stage parked flag from the engine is honoured', () => {
  const stages = nineStages({ 'identify:vision': { status: 'running', parked: true } });
  assert.deepEqual(parkedStageIds({ stages }), ['identify:vision']);
  assert.equal(stageWord({ id: 'identify:vision', status: 'running', parked: true }), 'paused');
  assert.equal(stageWord({ id: 'identify:vision', status: 'done', parked: true }), 'done', 'a finished stage is never parked');
});

test('a real scan pause still reads as a paused scan, with or without a parked stage', () => {
  const plain = snapshot({ tasks: [task('Scanning', 'paused')] });
  assert.equal(plain.paused, true);
  assert.equal(plain.badge.state, 'paused');
  assert.equal(plain.jobs[0].chip.word, 'Paused');
  // Both at once: the scan's own task is paused as well as the parked stage's.
  const both = snapshot({
    tasks: [task('Scanning', 'paused'), task('Judging moments', 'paused')],
    stages: nineStages({ 'identify:decision': { status: 'paused' } }),
  });
  assert.equal(both.paused, true);
  assert.equal(both.badge.state, 'paused');
  assert.deepEqual(both.parkedStages, ['identify:decision'], 'the stage still has its own Resume');
});

test('a parked cull stage holds the scan through the stage, not the scan pause', () => {
  assert.equal(scanPausedFrom({ tasks: [task('Scanning', 'paused')], parked: ['sharpness'] }), false);
});

test('Identifying tasks and engine-flagged stage tasks never count as the scan pause', () => {
  assert.equal(scanPausedFrom({ tasks: [task('Identifying 3 of 9', 'paused')] }), false);
  assert.equal(scanPausedFrom({ tasks: [{ ...task('Something', 'paused'), parked: true }] }), false);
  assert.equal(scanPausedFrom({ tasks: [task('Judging moments', 'paused')], parked: [] }), true, 'unexplained, so it stays a scan pause');
});

test('parked stages are only read while a run is active', () => {
  const album = withSettings({ native_stage_pause: ['identify:ocr'] });
  assert.deepEqual(parkedStageIds({ album, active: false }), []);
  assert.deepEqual(parkedStageIds({ album, active: true }), ['identify:ocr']);
  assert.equal(parkedSentence(['identify:ocr', 'identify:vision']), 'OCR and Vision are paused');
  assert.equal(parkedSentence(['identify:decision']), 'Decision is paused');
  assert.equal(parkedSentence([]), '');
});

test('the stage_paused word reads like a pause but is its own state', () => {
  assert.equal(runStateWord('stage_paused'), 'Stage paused');
  assert.equal(toneFor('stage_paused'), 'paused');
  const chip = jobChip(job(), { activeJob: 7, stageParkedJobIds: [7] });
  assert.equal(chip.state, 'stage_paused');
  assert.equal(jobChip(job(), { activeJob: 7, pausedJobIds: [7], stageParkedJobIds: [7] }).state, 'paused', 'a paused scan wins the chip');
});

test('Resume on a parked stage goes to the stage control, never resume_scan', () => {
  const titlebar = read('components/TitleBar.tsx');
  assert.match(titlebar, /actions\.resumeStage\(id\)/);
  const app = read('App.tsx');
  assert.match(app, /engine\.stageControl\('stage_pause', id, stage, \{ paused: false \}\)/);
  // The stage Resume is its own function: it must not call the scan's resume.
  const fn = app.slice(app.indexOf('const resumeStage'), app.indexOf('/** Pause or resume all Identify'));
  assert.doesNotMatch(fn, /resumeScan/);
  // The flowchart's own Resume is the stage control too.
  const node = buildSurface({ stages: nineStages({ 'identify:decision': { status: 'paused' } }), paused: false, view: {} }, { settings: resolvedSettings(), jobId: 7 })
    .nodes.find((n) => n.id === 'identify:decision');
  assert.equal(node.controls.pause.label, 'Resume');
  assert.equal(node.controls.pause.action, 'stage_pause');
  assert.deepEqual(node.controls.pause.args, { jobId: 7, stage: 'identify:decision', paused: false });
});

// ---- F-P2-1: a header skip persists, and has an undo ----

test('a stage skipped on the album is listed so both pages can offer the undo', () => {
  const album = withSettings({ native_stage_skip: ['identify:vision', 'identify:vision', 'group'] });
  assert.deepEqual(skippedStageIds({ album }), ['identify:vision', 'group']);
  assert.deepEqual(skippedStageIds({ album: job() }), []);
  assert.deepEqual(skippedStageIds({}), []);
});

test('the skip says it persists, and the undo is named', () => {
  const chip = skippedChip('identify:vision');
  assert.equal(chip.label, 'Vision skipped');
  assert.match(chip.note, /later runs/i);
  assert.equal(chip.undo, 'Undo skip Vision');
  const note = skipHeaderNote('identify:decision', 'Skipping Decision: no shoot-type suggestion.');
  assert.match(note, /Skipping Decision/);
  assert.match(note, /stays skipped on later runs of this album until you undo it/);
  assert.equal(stageName('identify:ocr'), 'OCR');
});

test('the header Skip asks first and the Undo sends stage_skip with skip false', () => {
  const holds = read('components/StageHolds.tsx');
  assert.match(holds, /role="alertdialog"/);
  assert.match(holds, /onStageControl\('stage_skip', id, \{ skip: false \}\)/);
  const ingest = read('screens/Ingest.tsx');
  assert.match(ingest, /<SkipStageButton/, 'the header uses the confirming button');
  assert.doesNotMatch(ingest, /onClick=\{\(\) => onStageControl\('stage_skip', activeIdentifyStage\.id, \{ skip: true \}\)\}/, 'no one-click skip');
  assert.match(ingest, /<StageHolds/);
  assert.match(read('screens/Identify.tsx'), /IngestScreen/, 'the Identify page is the same screen, so it shows the same chips');
});

// ---- F-P2-2: the flowchart follows the album's identify_order ----

test('the default flow is sequential OCR, Decision, Vision', () => {
  assert.deepEqual([...FLOW_ORDER], ['read', 'detect', 'sharpness', 'faces', 'identify:ocr', 'identify:decision', 'identify:vision', 'group', 'pick', 'done']);
  assert.deepEqual([...flowOrder(null)], [...FLOW_ORDER]);
  assert.deepEqual([...flowOrder(['bogus'])], [...FLOW_ORDER], 'an unusable order falls back to the default');
});

test('identify_order drives the drawn order and the stage tally follows it', () => {
  const snap = snapshot({ settings: resolvedSettings({ identify_order: ['vision', 'ocr', 'decision'] }) });
  const ids = snap.surface.nodes.map((n) => n.id);
  assert.deepEqual(ids, ['read', 'detect', 'sharpness', 'faces', 'identify:vision', 'identify:ocr', 'identify:decision', 'group', 'pick', 'done']);
  assert.deepEqual(snap.surface.nodes.map((n) => n.number), ids.map((_, i) => String(i + 1).padStart(2, '0')));
  const partial = buildSurface({ stages: nineStages(), view: {} }, { settings: resolvedSettings({ identify_order: ['decision'] }) });
  assert.deepEqual(partial.nodes.slice(4, 7).map((n) => n.id), ['identify:decision', 'identify:ocr', 'identify:vision'], 'missing stages keep default order after it');
});

test('parallel is opt-in and carries the cue; sequential stays the default', () => {
  assert.equal(snapshot().surface.parallel, false);
  assert.equal(snapshot({ settings: resolvedSettings({ identify_parallel: true }) }).surface.parallel, true);
  assert.match(PARALLEL_NOTE, /can change results/);
  assert.match(PARALLEL_NOTE, /slower to verify/);
  assert.match(FIELDS.identify_parallel.hint, /slower to verify/);
  assert.match(read('components/WorkflowSettings.tsx'), /PARALLEL_NOTE/);
  assert.match(read('screens/Ingest.tsx'), /Parallel identify is on/);
});

test('the app hands the album order to the engine-facing screens, not a hardcoded one', () => {
  const app = read('App.tsx');
  assert.match(app, /albumSetting\(currentAlbum, 'identify_order'/);
  assert.match(app, /albumSetting\(currentAlbum, 'identify_parallel'/);
  assert.match(app, /identifyOrder=\{albumIdentifyOrder\}/);
});

// ---- F-P2-4: the decider lane belongs to its own album ----

const feed = (over = {}) => ({ state: 'running', enabled: true, message: '', model: 'jev', device: 'GPU', quant: 'int8', capacity: 2, busy: 1, queued: 0, done: 3, errors: 0, avg_ms: 400, current: null, recent: [], install: null, ...over });

test('album A\'s global decider feed never shows on album B', () => {
  const global = feed({ done: 11 });
  assert.equal(deciderForJob({ global, jobId: 2, activeJob: 1, identifying: false }), null, 'B is not the album running');
  assert.equal(deciderForJob({ global, jobId: 1, activeJob: 1 }), global, 'A is running, so its feed is the global one');
  assert.equal(deciderForJob({ global, jobId: 1, activeJob: null, identifying: true }), global, 'an identify operation counts too');
  assert.equal(deciderForJob({ global, jobId: null, activeJob: null }), null);
});

test('a job id on the event binds it exactly, and the album\'s own copy always wins', () => {
  const named = { ...feed(), job_id: 5 };
  assert.equal(deciderForJob({ global: named, jobId: 5, activeJob: 9 }), named);
  assert.equal(deciderForJob({ global: named, jobId: 6, activeJob: 6, identifying: true }), null);
  const own = feed({ done: 99 });
  assert.equal(deciderForJob({ perJob: own, global: feed({ done: 1 }), jobId: 2, activeJob: 2 }), own, 'the per-job copy beats the global slot');
});

test('a finished run leaves nothing live in the decider feed', () => {
  const settled = deciderForJob({ global: feed({ busy: 2, queued: 1, current: { label: 'x', thumb: null, queued: false, elapsed_ms: 1, asking: [] } }), jobId: 1, activeJob: 1, ended: true });
  assert.equal(settled.busy, 0);
  assert.equal(settled.queued, 0);
  assert.equal(settled.current, null);
  assert.equal(settled.state, 'ready');
  assert.equal(settleDecider(feed({ state: 'error' })).state, 'error', 'a real error stays an error');
  assert.equal(settleDecider(null), null);
});

test('Ingest binds the decider through deciderForJob', () => {
  const ingest = read('screens/Ingest.tsx');
  assert.match(ingest, /deciderForJob\(/);
  assert.doesNotMatch(ingest, /decider \?\? idLive\?\.decider/, 'the global slot no longer beats the per-job copy');
});

// ---- F-P2-5: a failed Shoot Settings save is visible and honest ----

test('saving runs the steps in order, stops at the first failure and says what saved', async () => {
  const ran = [];
  const outcome = await runSaveSteps([
    { label: 'Shoot settings', run: async () => { ran.push('settings'); } },
    { label: 'Faces', run: async () => { ran.push('faces'); throw new Error('Error: regroup refused while a scan runs'); } },
    { label: 'Regroup stacks', run: async () => { ran.push('regroup'); } },
  ]);
  assert.deepEqual(ran, ['settings', 'faces'], 'nothing runs after the failure');
  assert.deepEqual(outcome.saved, ['Shoot settings']);
  assert.deepEqual(outcome.failed, { label: 'Faces', message: 'regroup refused while a scan runs' });
  assert.deepEqual(outcome.notRun, ['Regroup stacks']);
  const message = saveFailureMessage(outcome);
  assert.match(message, /Faces could not be saved: regroup refused while a scan runs\./);
  assert.match(message, /Already saved: Shoot settings\./);
  assert.match(message, /Not applied: Regroup stacks\./);
});

test('a clean save reports nothing, and a first-step failure says nothing changed', async () => {
  const ok = await runSaveSteps([{ label: 'a', run: () => 1 }, { label: 'b', run: () => 2 }]);
  assert.deepEqual(ok, { saved: ['a', 'b'], failed: null, notRun: [] });
  assert.equal(saveFailureMessage(ok), '');
  const first = await runSaveSteps([{ label: 'Shoot settings', run: () => { throw new Error('disk full.'); } }]);
  assert.equal(saveFailureMessage(first), 'Shoot settings could not be saved: disk full. Nothing was changed.');
  assert.equal(featureLabel('closed_eyes'), 'Closed eyes');
  assert.equal(featureLabel(''), 'Setting');
});

test('the modal catches the failure, shows it, and keeps the edits and the sheet open', () => {
  const modal = read('components/ShootSettingsModal.tsx');
  assert.match(modal, /catch \(e\)/);
  assert.match(modal, /role="alert"/);
  assert.match(modal, /setSaveError/);
  // onClose only runs after a successful save.
  assert.ok(modal.indexOf('onClose();\n') < modal.indexOf('catch (e)') || modal.indexOf('onClose();\r\n') < modal.indexOf('catch (e)'));
  const review = read('screens/Review.tsx');
  assert.match(review, /runSaveSteps\(steps\)/);
  assert.match(review, /throw new Error\(saveFailureMessage\(outcome\)\)/);
});

// ---- F-P2-6: one shared sheet shell for every overlay ----

test('the eight overlay sheets all sit on the one Sheet shell', () => {
  const files = [
    'components/ShootSettingsModal.tsx', 'components/ExportSheet.tsx', 'components/GlobalStatsModal.tsx', 'components/Identifications.tsx',
    'components/LightroomImport.tsx', 'components/ShootStatsModal.tsx', 'components/WhatsNewSheet.tsx', 'screens/Import.tsx',
  ];
  for (const f of files) {
    const src = read(f);
    assert.match(src, /<Sheet /, `${f} uses Sheet`);
    assert.doesNotMatch(src, /className="import-overlay"/, `${f} has no private overlay`);
    assert.doesNotMatch(src, /useBackdropClose/, `${f} has no private backdrop handler`);
  }
  const sheet = read('components/Sheet.tsx');
  assert.match(sheet, /role="dialog"/);
  assert.match(sheet, /aria-modal="true"/);
  assert.match(sheet, /aria-labelledby/);
  assert.match(sheet, /e\.key === 'Escape'/);
  assert.match(sheet, /returnTo/, 'focus returns to where it was');
  assert.match(sheet, /trapTarget/);
});

test('Tab wraps inside a dialog and stays free in the middle', () => {
  assert.equal(trapTarget(4, 3, false), 0, 'last control, Tab: back to the first');
  assert.equal(trapTarget(4, 0, true), 3, 'first control, Shift+Tab: on to the last');
  assert.equal(trapTarget(4, 1, false), null, 'the middle is the browser\'s');
  assert.equal(trapTarget(4, 2, true), null);
  assert.equal(trapTarget(4, -1, false), 0, 'focus outside: pulled to the first control');
  assert.equal(trapTarget(4, -1, true), 3);
  assert.equal(trapTarget(0, -1, false), null);
});

test('only the sheet on top answers Esc', () => {
  const stack = createSheetStack();
  const a = {}, b = {};
  stack.push(a); stack.push(b);
  assert.equal(stack.isTop(b), true);
  assert.equal(stack.isTop(a), false);
  stack.remove(b);
  assert.equal(stack.isTop(a), true, 'closing the top sheet hands Esc back to the one beneath');
  stack.remove(a);
  assert.equal(stack.size, 0);
  assert.equal(stack.isTop(a), false);
});

// ---- carried: hotkeys, bounded progress cache, decision models ----

const el = (matches) => ({ closest: (selector) => (matches.some((m) => selector.includes(m)) ? {} : null) });

test('Enter and Space press a focused Inspector, zoom-tool or toolbar button', () => {
  assert.equal(ownsActivationKeys(el(['.inspector', 'button'])), true);
  assert.equal(ownsActivationKeys(el(['.stage-pill-btn', 'button'])), true);
  assert.equal(ownsActivationKeys(el(['.toolbar', 'button'])), true);
  assert.equal(ownsActivationKeys(el(['button'])), false, 'a button in the photo grid keeps Enter for opening the burst');
  assert.equal(ownsActivationKeys(null), false);
  assert.match(read('state/useHotkeys.ts'), /ownsActivationKeys\(target\)/);
});

test('Viewer and Compare leave Space to a focused button and every key to a select', () => {
  const select = el(['select']);
  const button = el(['button']);
  const nothing = el([]);
  assert.equal(zoomKeyIgnored(select, 'l'), true);
  assert.equal(zoomKeyIgnored(select, ' '), true);
  assert.equal(zoomKeyIgnored(button, ' '), true, 'Space presses the button');
  assert.equal(zoomKeyIgnored(button, 'Enter'), true);
  assert.equal(zoomKeyIgnored(button, 'l'), false, 'a letter still toggles labels');
  assert.equal(zoomKeyIgnored(nothing, ' '), false, 'Space zooms when nothing owns it');
  assert.equal(zoomKeyIgnored(el(['textarea']), 'z'), true);
  assert.match(read('screens/Viewer.tsx'), /zoomKeyIgnored\(e\.target, e\.key\)/);
  assert.match(read('screens/Compare.tsx'), /zoomKeyIgnored\(e\.target, e\.key\)/);
});

test('the identify progress cache keeps the last few albums and never the running one out', () => {
  let all = {}, order = [];
  const put = (job_id, state = 'done') => { ({ all, order } = rememberProgress(all, order, { job_id, state })); };
  put(1, 'running');
  for (let id = 2; id <= PROGRESS_KEEP + 3; id += 1) put(id);
  assert.equal(Object.keys(all).length, PROGRESS_KEEP, 'bounded');
  assert.ok(all[1], 'the album still running is kept however old');
  assert.ok(all[PROGRESS_KEEP + 3], 'the newest is kept');
  assert.equal(all[2], undefined, 'the oldest finished album went');
  put(5, 'done');
  assert.equal(order[order.length - 1], 5, 'a repeat payload moves to the newest end');
  assert.equal(Object.keys(all).length, order.length);
  assert.match(read('state/useIdentifyProgress.ts'), /rememberProgress/);
});

test('decision models keep what the engine said: sees photos, unverified, free text', () => {
  const live = [{ id: 'jev-1.13', name: 'jev-1.13', image: true, unverified: false }, { id: 'tiny-text', name: 'tiny-text', image: false, unverified: true }, { id: 'new-one', name: 'new-one', image: null }];
  const dc = decisionCandidates('opencode', [], live, { freeText: true });
  assert.equal(dc.freeText, true, 'the engine\'s own free-text answer is kept');
  assert.deepEqual(dc.models.map((m) => [m.id, m.image, m.unverified]), [['jev-1.13', true, false], ['tiny-text', false, true], ['new-one', null, undefined]]);
  assert.equal(decisionCandidates('opencode', [], live).freeText, false);
  assert.equal(decisionFlag(dc.models[0]), '');
  assert.equal(decisionFlag(dc.models[1]), ' (text only, unverified)');
  assert.equal(decisionMeta(dc.models[0]), 'sees photos');
  assert.equal(decisionMeta(dc.models[1]), 'text only: cannot see photos, unverified');
  assert.equal(decisionMeta(dc.models[2]), '', 'unknown says nothing rather than "sees photos"');
  const card = read('screens/AiSettingsCard.tsx');
  assert.match(card, /image: m\.image, unverified: m\.unverified/);
  assert.match(card, /freeText: engineFreeText/);
  assert.match(card, /\(not in the list\)/, 'a saved model missing from the list is not shown as None');
});

// ---- P3-16: vehicle wording follows the shoot ----

test('only a vehicle shoot says make, plate and tyre smoke', () => {
  assert.equal(SUBJECT_WORDS.vehicles.fields.notRead, 'Make not read');
  assert.match(SUBJECT_WORDS.vehicles.fields.action, /tyre smoke/);
  for (const kind of ['animals', 'people', 'subjects']) {
    const f = SUBJECT_WORDS[kind].fields;
    for (const text of Object.values(f)) assert.doesNotMatch(text, /\b(make|plates?|tyre|wheel|vehicles?|race)\b/i, `${kind}: ${text}`);
  }
});

test('the identify cards, shoot settings and confirms take their words from the subject', () => {
  const parts = read('components/IdentifyParts.tsx');
  for (const hard of ['Make not read', 'Make / Model', 'Reading plates & numbers']) assert.doesNotMatch(parts, new RegExp(hard.replace(/[/&]/g, '\\$&')), hard);
  assert.match(parts, /w\.fields\.makeModel/);
  const settings = read('components/ShootSettingsModal.tsx');
  assert.doesNotMatch(settings, /tyre smoke|wheel lift|Group vehicles/);
  assert.match(settings, /subject\.fields\.action/);
  assert.doesNotMatch(read('screens/Ingest.tsx'), /'Make not read'|Reading plates and numbers/);
  assert.doesNotMatch(read('components/TitleBar.tsx'), /Settings and vehicles/);
  assert.doesNotMatch(read('components/WorkflowSettings.tsx'), /registration plates, race numbers/);
  assert.doesNotMatch(read('screens/Review.tsx'), /All vehicle identifications removed|removes all vehicle plates/);
});
