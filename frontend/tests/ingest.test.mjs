import test from 'node:test';
import assert from 'node:assert/strict';
import {
  boxLabel, boxStyle, frameRate, identifyEta, identifyNodes, ingestFraction, ingestIdentify, ingestNodes, ingestPhase, laneBadge, laneFrame, laneKey,
  laneNumber, laneState, laneText, leftLabel, phaseRunning, phaseTone, phaseWord, selectLanes, stageOfStep, starsText, stepIndex, stripItems,
  verdictText, visionExpected, tagFits, tagWidth,
} from '../src/lib/ingest.mjs';

const stage = (id, done, total, busy = 0, workers = 2, extra = {}) => ({ id, label: id[0].toUpperCase() + id.slice(1), model: `${id}-model`, device: 'CPU', workers_total: workers, workers_busy: busy, queue_len: 0, done, total, rate_per_s: 0, eta_s: null, ...extra });
const worker = (stage, worker_index, state = 'busy', extra = {}) => ({ stage, worker_index, state, waiting_for: null, file: null, step: null, ms_in_step: 0, ...extra });

test('steps map onto the flowchart stage they belong to', () => {
  assert.equal(stageOfStep('decode'), 'read');
  assert.equal(stageOfStep('persist'), 'faces');
  assert.equal(stageOfStep('nonsense', 'detect'), 'detect');
  assert.equal(stepIndex('sharpness'), 3);
  assert.equal(stepIndex('x'), -1);
});

test('the merged flowchart runs read to pick, then vision, then done', () => {
  const stages = [stage('read', 847, 847, 0), stage('detect', 412, 847, 2), stage('sharpness', 380, 847, 1), stage('faces', 0, 847, 0), stage('group', 0, 847, 0, 1), stage('pick', 0, 847, 0, 1), stage('identify', 0, 60, 0, 3)];
  const nodes = ingestNodes({ stages });
  assert.deepEqual(nodes.map((n) => n.label), ['READ', 'DETECT', 'SHARPNESS', 'FACES', 'GROUP', 'PICK', 'VISION', 'DONE']);
  assert.deepEqual(nodes.map((n) => n.number), ['01', '02', '03', '04', '05', '06', '07', '08']);
  assert.deepEqual(nodes.map((n) => n.state), ['done', 'active', 'active', 'pending', 'pending', 'pending', 'pending', 'pending']);
  assert.equal(nodes[1].sub, '412 / 847');
  assert.match(nodes[1].meta, /2\/2 busy/);
  assert.match(nodes[1].meta, /detect-model/);
  assert.equal(nodes[1].filter, 'detect');
  assert.equal(nodes[4].filter, null, 'group has no workers to filter to');
  assert.equal(nodes[6].filter, 'vision');
});

test('the vision node takes its count from the identify run once it reports', () => {
  const stages = [stage('read', 10, 10), stage('identify', 0, 60, 0, 3)];
  const live = { state: 'running', done: 12, total: 60, workers: 3, model: 'qwen2.5vl:7b', stage: 'vision', current: null, active: [], recent: [], errors: [] };
  const vision = ingestNodes({ stages, identify: live }).find((n) => n.id === 'vision');
  assert.equal(vision.state, 'active');
  assert.equal(vision.sub, '12 / 60 cars');
  assert.match(vision.meta, /3 workers · qwen2\.5vl:7b/);
  const idle = ingestNodes({ stages }).find((n) => n.id === 'vision');
  assert.equal(idle.state, 'pending');
  assert.equal(idle.sub, '0 / 60 cars');
});

test('no vision node when the run does not identify, and an off stage is dimmed', () => {
  const stages = [stage('read', 5, 10), stage('faces', 0, 0, 0, 0), stage('identify', 0, 0, 0, 0)];
  const nodes = ingestNodes({ stages });
  assert.equal(nodes.some((n) => n.id === 'vision'), false);
  const faces = nodes.find((n) => n.id === 'faces');
  assert.equal(faces.off, true);
  assert.equal(faces.sub, 'not used');
  assert.equal(faces.filter, null);
  assert.equal(visionExpected(stages, null), false);
  assert.equal(visionExpected(stages, { state: 'running' }), true);
});

test('before the first snapshot every node is drawn pending; a finished run is all done', () => {
  const empty = ingestNodes({ stages: [] });
  assert.equal(empty.length, 7);
  assert.ok(empty.every((n) => n.state === 'pending'));
  const finished = ingestNodes({ stages: [stage('read', 5, 5), stage('detect', 5, 5), stage('identify', 3, 3)], finished: true, identify: { state: 'done', done: 3, total: 3 } });
  assert.ok(finished.every((n) => n.state === 'done'));
  assert.equal(finished.at(-1).sub, 'ready to review');
});

test('the identify-only chart keeps the six identify nodes and none is a filter', () => {
  const nodes = identifyNodes({ stage: 'vision', state: 'running' }, 'qwen');
  assert.deepEqual(nodes.map((n) => n.label), ['LOAD', 'CROP', 'READ', 'VISION', 'ENTRY LIST', 'RESULT']);
  assert.ok(nodes.every((n) => n.filter === null));
  assert.equal(nodes[3].sub, 'qwen');
});

test('the phase follows the scan, then the identify run', () => {
  assert.equal(ingestPhase({ scan: { state: 'idle' } }), 'idle');
  assert.equal(ingestPhase({ scan: { state: 'starting' } }), 'starting');
  assert.equal(ingestPhase({ scan: { state: 'running', stage: 'detect' } }), 'scan');
  assert.equal(ingestPhase({ scan: { state: 'running', stage: 'group' } }), 'group');
  assert.equal(ingestPhase({ scan: { state: 'running', stage: 'pick' } }), 'pick');
  assert.equal(ingestPhase({ scan: { state: 'running', stage: 'identify' } }), 'vision', 'a one-shot scan sits in its identify stage');
  assert.equal(ingestPhase({ scan: { state: 'running', stage: 'faces' }, identify: { state: 'running' } }), 'vision');
  assert.equal(ingestPhase({ scan: { state: 'done' } }), 'done');
  assert.equal(ingestPhase({ scan: { state: 'done' }, identify: { state: 'running' } }), 'vision', 'identify keeps the run going after the scan');
  assert.equal(ingestPhase({ scan: { state: 'done' }, identifying: true }), 'vision');
  assert.equal(ingestPhase({ scan: { state: 'done' }, identify: { state: 'done' } }), 'done');
  assert.equal(ingestPhase({ scan: { state: 'done' }, identify: { state: 'error' } }), 'error');
  assert.equal(ingestPhase({ scan: { state: 'stopped' } }), 'stopped');
  assert.equal(ingestPhase({ mode: 'identify' }), 'idle');
  assert.equal(ingestPhase({ mode: 'identify', identifying: true }), 'starting');
  assert.equal(ingestPhase({ mode: 'identify', identify: { state: 'running' } }), 'vision');
  assert.equal(ingestPhase({ mode: 'identify', identify: { state: 'stopped' } }), 'stopped');
  assert.equal(phaseRunning('vision'), true);
  assert.equal(phaseRunning('done'), false);
});

test('phase words and tones: paused wins while work is in flight', () => {
  assert.equal(phaseWord('scan'), 'Scanning');
  assert.equal(phaseWord('vision'), 'Identifying');
  assert.equal(phaseWord('scan', true), 'Paused');
  assert.equal(phaseWord('done', true), 'Complete');
  assert.equal(phaseTone('scan'), 'running');
  assert.equal(phaseTone('scan', true), 'stopped');
  assert.equal(phaseTone('error'), 'error');
});

test('the one bar: scan alone, or 70/30 with a vision phase', () => {
  assert.equal(ingestFraction({ scanFraction: 0.5 }), 0.5);
  assert.equal(ingestFraction({ scanFraction: 1, vision: true }), 0.7);
  assert.equal(ingestFraction({ scanFraction: 0.5, vision: true }), 0.35);
  assert.equal(ingestFraction({ scanFraction: 1, vision: true, identify: { state: 'running', done: 30, total: 60 } }), 0.85);
  assert.equal(ingestFraction({ scanFraction: 1, vision: true, identify: { state: 'done', done: 3, total: 3 } }), 1);
  assert.equal(ingestFraction({ scanFraction: 7 }), 1, 'never past the end');
  assert.equal(ingestFraction({ scanFraction: NaN }), 0);
});

test('identify time left comes from its own pace', () => {
  assert.equal(identifyEta({ state: 'running', done: 10, total: 30, elapsed_ms: 20000 }), 40);
  assert.equal(identifyEta({ state: 'running', done: 0, total: 30, elapsed_ms: 20000 }), null);
  assert.equal(identifyEta({ state: 'done', done: 30, total: 30, elapsed_ms: 20000 }), null);
  assert.equal(leftLabel('about 6 min 05 s'), 'about 6 min 05 s left');
  assert.equal(leftLabel('estimating…'), 'estimating…');
  assert.equal(frameRate('1.2/s'), '1.2 fr/s');
  assert.equal(frameRate(''), '');
});

test('lane state: paused and waiting never look busy, idle stays idle', () => {
  assert.equal(laneState(worker('detect', 0, 'busy')), 'busy');
  assert.equal(laneState(worker('detect', 0, 'busy'), true), 'paused', 'a global pause freezes a busy lane');
  assert.equal(laneState(worker('detect', 0, 'idle'), true), 'idle');
  assert.equal(laneState(worker('detect', 0, 'paused')), 'paused');
  assert.equal(laneState(worker('detect', 0, 'waiting', { waiting_for: 'paused' })), 'paused');
  assert.equal(laneState(worker('detect', 0, 'waiting', { waiting_for: 'gpu' })), 'waiting');
  assert.equal(laneState(null), 'idle');
});

test('lane words say why a lane waits, and what a busy one does', () => {
  assert.equal(laneText(worker('detect', 0, 'waiting', { waiting_for: 'gpu' })), 'waiting for detector');
  assert.equal(laneText(worker('detect', 0, 'waiting', { waiting_for: 'faces' })), 'waiting for faces');
  assert.equal(laneText(worker('detect', 0, 'waiting')), 'waiting');
  assert.equal(laneText(worker('detect', 0, 'busy'), true), 'paused');
  assert.equal(laneText(worker('detect', 0, 'idle')), 'idle');
  assert.equal(laneText(worker('detect', 0, 'busy', { step: 'YOLO' })), 'YOLO');
  assert.equal(laneText(worker('detect', 0, 'busy', { step: 'YOLO', current: { step: 'detect' } })), 'detect');
  assert.equal(laneBadge('busy'), 'LIVE');
  assert.equal(laneBadge('paused'), 'PAUSED');
});

test('a lane is keyed by its stable id when the engine sends one', () => {
  assert.equal(laneKey(worker('detect', 1)), 'detect:1');
  assert.equal(laneKey(worker('detect', 1, 'busy', { lane: 4 })), 'lane:4');
  assert.equal(laneNumber(worker('detect', 1, 'busy', { lane: 4 })), 5);
  assert.equal(laneNumber(worker('detect', 1)), 2);
});

test('the lane grid drops barriers, filters by stage, and keeps the busiest past the cap in stable order', () => {
  const workers = [
    worker('read', 0, 'idle'), worker('read', 1, 'idle'), worker('detect', 0), worker('detect', 1, 'waiting'), worker('sharpness', 0, 'paused'),
    worker('faces', 0), worker('group', 0), worker('pick', 0), worker('identify', 0),
  ];
  const all = selectLanes(workers, { cap: 100 });
  assert.equal(all.lanes.length, 6, 'group, pick and the scan identify lane are not cards');
  assert.deepEqual(all.lanes.map((w) => w.stage), ['read', 'read', 'detect', 'detect', 'sharpness', 'faces']);
  assert.equal(selectLanes(workers, { filter: 'detect' }).lanes.length, 2);
  assert.equal(selectLanes(workers, { identify: true, cap: 100 }).lanes.length, 7);
  const capped = selectLanes(workers, { cap: 4 });
  assert.equal(capped.hidden, 2);
  assert.deepEqual(capped.lanes.map((w) => `${w.stage}${w.worker_index}`), ['detect0', 'detect1', 'sharpness0', 'faces0'], 'the two idle lanes go');
  assert.equal(selectLanes(workers, { cap: 4, all: true }).hidden, 0);
  const ided = [worker('x', 0, 'busy', { lane: 2 }), worker('y', 1, 'busy', { lane: 0 })];
  assert.deepEqual(selectLanes(ided).lanes.map((w) => w.lane), [0, 2]);
  assert.deepEqual(selectLanes(null).lanes, []);
});

test('a lane draws its own frame, else its file in the finished list, else nothing but the name', () => {
  const recent = [{ image_id: 1, file: 'IMG_1.CR3', thumb_url: 't1', verdict: 'soft', stars: 2, boxes: [{ cls: 'car', conf: 0.9, box: [0, 0, 1, 1] }], af: [0.1, 0.1, 0.2, 0.2] }];
  const own = laneFrame(worker('detect', 0, 'busy', { file: 'IMG_9.CR3', current: { image_id: 9, file: 'IMG_9.CR3', step: 'detect', thumb_url: 't9', stars: 4, verdict: 'sharp', panning: true } }), recent);
  assert.deepEqual([own.thumb_url, own.stars, own.verdict, own.panning, own.step], ['t9', 4, 'sharp', true, 'detect']);
  const mapped = laneFrame(worker('detect', 0, 'busy', { file: 'IMG_1.CR3' }), recent);
  assert.deepEqual([mapped.thumb_url, mapped.stars, mapped.boxes.length, mapped.verdict], ['t1', 2, 1, 'soft']);
  const bare = laneFrame(worker('detect', 0, 'busy', { file: 'IMG_5.CR3', step: 'YOLO' }), recent);
  assert.deepEqual([bare.thumb_url, bare.file, bare.stars, bare.boxes, bare.af, bare.step], ['', 'IMG_5.CR3', null, [], null, 'YOLO']);
  assert.equal(laneFrame(worker('detect', 0, 'idle'), undefined).file, '');
});

test('boxes become clamped percentages; junk is not drawn', () => {
  assert.deepEqual(boxStyle([0.25, 0.1, 0.75, 0.5]), { left: '25%', top: '10%', width: '50%', height: '40%' });
  assert.deepEqual(boxStyle([0.75, 0.5, 0.25, 0.1]), { left: '25%', top: '10%', width: '50%', height: '40%' }, 'corners may come in either order');
  assert.deepEqual(boxStyle([-0.2, 0.5, 1.5, 0.9]), { left: '0%', top: '50%', width: '100%', height: '40%' });
  assert.equal(boxStyle([0.2, 0.2, 0.2, 0.5]), null);
  assert.equal(boxStyle(null), null);
  assert.equal(boxStyle([0.1, 'x', 0.5, 0.5]), null);
  assert.equal(boxLabel({ cls: 'car', conf: 0.924 }), 'car 92%');
  assert.equal(boxLabel({ cls: 'car' }), 'car');
});

test('stars and verdict words', () => {
  assert.equal(starsText(4), '★★★★☆');
  assert.equal(starsText(0), '☆☆☆☆☆');
  assert.equal(starsText(9), '★★★★★');
  assert.equal(starsText(null), '');
  assert.equal(verdictText('sharp', true), 'Sharp · pan');
  assert.equal(verdictText('blurred'), 'Blurred');
  assert.equal(verdictText(null), '');
  assert.equal(verdictText(null, true), 'pan');
});

test('an identify snapshot counts for an ingest run only when it is this run', () => {
  const done = { state: 'done' }, running = { state: 'running' };
  assert.equal(ingestIdentify(done, {}), undefined, 'a stale finished run is ignored');
  assert.equal(ingestIdentify(done, { planned: true }), done);
  assert.equal(ingestIdentify(running, {}), running);
  assert.equal(ingestIdentify(done, { identifying: true }), done);
  assert.equal(ingestIdentify(undefined, { planned: true }), undefined);
});

test('the strip leads with cars, then culled frames, capped', () => {
  const items = stripItems({ cars: [{ detection_id: 7 }, { detection_id: 6 }], frames: [{ image_id: 1 }, { image_id: 2 }], limit: 3 });
  assert.deepEqual(items.map((i) => i.key), ['car:7', 'car:6', 'frame:1']);
  assert.deepEqual(stripItems().length, 0);
});

test('a label is drawn only when its box is wide enough for it', () => {
  assert.ok(tagWidth('car 92%') > tagWidth('AF'));
  assert.equal(tagFits([0.2, 0.4, 0.7, 0.8], 300, 'car 92%'), true, 'a car box is wide');
  assert.equal(tagFits([0.9, 0.5, 0.95, 0.8], 300, 'person 52%'), false, 'a narrow person box keeps only its outline');
  assert.equal(tagFits([0.3, 0.47, 0.42, 0.58], 300, 'AF'), true);
  assert.equal(tagFits([0.3, 0.47, 0.32, 0.58], 300, 'AF'), false);
  assert.equal(tagFits(null, 300, 'AF'), false);
});
