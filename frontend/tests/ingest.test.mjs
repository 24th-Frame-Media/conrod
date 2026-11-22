import test from 'node:test';
import assert from 'node:assert/strict';
import {
  boxLabel, boxRole, boxStyle, comparingBursts, etaLines, frameRate, identifyEta, identifyNodes, ingestFraction, ingestIdentify, ingestNodes, ingestPhase,
  laneBadge, laneFrame, laneKey, laneNumber, laneState, laneText, leftLabel, mainVehicle, phaseRunning, phaseTone, phaseWord, roleLabel,
  selectLanes, stageOfStep, starsText, stepIndex, stripItems, verdictText, visionExpected, tagFits, tagWidth, afLabel, afTagSpot, failedWords,
  TAG_HEIGHT,
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
  assert.equal(nodes[1].sub, 'Car and person detector', 'the box says the job, not the model file');
  assert.match(nodes[1].meta, /412 \/ 847/);
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
  assert.equal(vision.sub, 'Reading cars — Qwen2.5-VL · this PC');
  assert.match(vision.meta, /3 workers/);
  assert.match(vision.meta, /12 \/ 60 cars/);
  assert.match(vision.title, /qwen2\.5vl:7b/, 'the raw file stays in the tooltip');
  const idle = ingestNodes({ stages }).find((n) => n.id === 'vision');
  assert.equal(idle.state, 'pending');
  assert.equal(idle.sub, 'Reading cars — identify-model');
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
  assert.equal(nodes[3].sub, 'Reading cars — Qwen · this PC');
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
  assert.equal(phaseWord('peaks'), 'Comparing bursts');
  assert.equal(phaseWord('scan', true), 'Paused');
  assert.equal(phaseWord('done', true), 'Complete');
  assert.equal(phaseTone('scan'), 'running');
  assert.equal(phaseTone('peaks'), 'running');
  assert.equal(phaseTone('scan', true), 'stopped');
  assert.equal(phaseTone('error'), 'error');
});

test('past the cars, the run is its own phase: comparing bursts, with the count', () => {
  const live = {
    state: 'running', done: 5, total: 5, stage: 'done', workers: 0, active: [], recent: [],
    phase: { name: 'peaks', done: 12, total: 214, state: 'running' },
    decider: { state: 'ready', done: 5, busy: 0, current: null, recent: [] },
  };
  assert.equal(comparingBursts(live), true);
  assert.equal(ingestPhase({ mode: 'identify', identify: live }), 'peaks');
  assert.equal(ingestPhase({ scan: { state: 'done' }, identify: live }), 'peaks', 'a scan that finished its cars follows the identify run');
  assert.equal(phaseRunning('peaks'), true);
  // The chart names the work, and the DECIDER node does not tick until it ends.
  const nodes = identifyNodes(live, 'qwen');
  const decider = nodes.find((node) => node.id === 'decider');
  assert.equal(decider.state, 'active');
  assert.match(decider.meta, /Comparing bursts 12\/214/);
  // The phase's terminal is the only tick, and the run's own state follows it.
  const ended = { ...live, state: 'done', phase: { name: 'peaks', done: 214, total: 214, state: 'done' } };
  assert.equal(comparingBursts(ended), false);
  assert.equal(identifyNodes(ended, 'qwen').find((node) => node.id === 'decider').state, 'done');
  assert.equal(ingestPhase({ mode: 'identify', identify: ended }), 'done');
  // A run still on its cars has no phase of its own.
  assert.equal(comparingBursts({ ...live, phase: null }), false);
  assert.equal(comparingBursts(null), false);
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


test('a lane takes the real fields of the engine payload: boxes, autofocus kind, rating', () => {
  const current = {
    image_id: 42, file: 'IMG_1.CR3', step: 'sharpness', thumb_url: 'cache/native/thumb-42.jpg',
    boxes: [{ cls: 'car', conf: 0.92, box: [0.2, 0.4, 0.7, 0.8] }], af: [0.4, 0.5, 0.5, 0.6], af_kind: 'grid', stars: 4, verdict: 'soft', panning: true,
  };
  const frame = laneFrame(worker('sharpness', 0, 'busy', { lane: 3, file: 'IMG_1.CR3', current }), []);
  assert.deepEqual(frame.boxes, current.boxes);
  assert.equal(frame.af_kind, 'grid');
  assert.equal(frame.stars, 4);
  assert.equal(frame.verdict, 'soft');
  assert.equal(frame.panning, true);
  assert.equal(frame.step, 'sharpness');
  const point = laneFrame(worker('read', 0, 'busy', { current: { image_id: 1, file: 'a', step: 'detect', af: [0.1, 0.1, 0.2, 0.2] } }), []);
  assert.equal(point.af_kind, 'point', 'an area with no kind is a point');
  const none = laneFrame(worker('read', 0, 'busy', { current: { image_id: 1, file: 'a', step: 'read' } }), []);
  assert.equal(none.af_kind, null);
  const finished = laneFrame(worker('read', 0, 'idle', { file: 'IMG_9.CR3', current: null }), [{ file: 'IMG_9.CR3', af: [0.1, 0.1, 0.3, 0.3], af_kind: 'grid', thumb_url: 't.jpg', verdict: 'sharp' }]);
  assert.equal(finished.af_kind, 'grid', 'the last finished frame fills the lane that has none');
  assert.equal(laneText(worker('read', 0, 'waiting', { waiting_for: 'faces', current })), 'waiting for faces');
  assert.equal(laneText(worker('read', 0, 'waiting', { waiting_for: 'detector', current })), 'waiting for detector');
});

test('a stage that lost frames says how many failed', () => {
  assert.equal(failedWords(0), '');
  assert.equal(failedWords(undefined), '');
  assert.equal(failedWords(2), '2 failed');
  assert.equal(failedWords(1200), '1,200 failed');
  const [read, detect] = ingestNodes({ stages: [stage('read', 497, 497, 0, 2, { errors: 3 }), stage('detect', 100, 497, 2)] });
  assert.equal(read.sub, 'Opens each RAW');
  assert.match(read.meta, /^497 \/ 497 · 3 failed/, 'the counters reach the total: failed frames are out of it');
  assert.match(read.meta, /3 failed/);
  assert.match(read.title, /3 failed/);
  assert.doesNotMatch(detect.meta, /failed/);
});

test('the autofocus label reads AF, or AF grid for a grid union', () => {
  assert.equal(afLabel('point'), 'AF');
  assert.equal(afLabel(undefined), 'AF');
  assert.equal(afLabel('grid'), 'AF grid');
});

test('the autofocus tag keeps to the bottom-left unless a detection label is there', () => {
  const frame = { width: 400, height: 300 };
  const af = [0.4, 0.4, 0.6, 0.6];
  assert.equal(afTagSpot(af, [], frame), 'bl', 'nothing in the way');
  assert.equal(afTagSpot(af, [{ cls: 'car', conf: 0.9, box: [0.1, 0.1, 0.9, 0.9] }], frame), 'bl', 'a car label at the car box top-left is far away');
  // A detection whose top-left corner lands on the AF box bottom-left: its label sits where the AF tag would.
  const onTag = [{ cls: 'car', conf: 0.92, box: [0.4, 0.55, 0.9, 0.95] }];
  assert.equal(afTagSpot(af, onTag, frame), 'br', 'moves to the bottom-right');
  const both = [...onTag, { cls: 'car', conf: 0.88, box: [0.5, 0.55, 0.95, 0.95] }];
  assert.equal(afTagSpot(af, both, frame), 'tr', 'both bottom corners taken');
});

test('a detection label too narrow to be drawn does not push the autofocus tag away', () => {
  const frame = { width: 400, height: 300 };
  const af = [0.4, 0.4, 0.6, 0.6];
  const thin = [{ cls: 'person', conf: 0.5, box: [0.4, 0.55, 0.42, 0.9] }];
  assert.equal(tagFits(thin[0].box, frame.width, boxLabel(thin[0])), false);
  assert.equal(afTagSpot(af, thin, frame), 'bl');
});

test('the autofocus tag never overlaps a drawn label, and is dropped rather than collide', () => {
  const frame = { width: 400, height: 300 };
  const af = [0.3, 0.45, 0.55, 0.62];
  const w = tagWidth('AF');
  const hit = (a, b) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
  const spots = (x1, y1, x2, y2) => ({
    bl: { x: x1, y: y2 - TAG_HEIGHT }, br: { x: x2 - w, y: y2 - TAG_HEIGHT }, tr: { x: x2 - w, y: y1 }, tl: { x: x1, y: y1 },
    below: { x: x1, y: y2 }, above: { x: x1, y: y1 - TAG_HEIGHT },
  });
  const [x1, y1, x2, y2] = [af[0] * 400, af[1] * 300, af[2] * 400, af[3] * 300];
  const where = spots(x1, y1, x2, y2);
  // Labels over every spot but 'above': the tag lands there.
  const over = ['bl', 'br', 'tr', 'tl', 'below'].map((s, i) => ({ cls: 'car', conf: 0.9 - i / 100, box: [where[s].x / 400, where[s].y / 300, where[s].x / 400 + 0.3, where[s].y / 300 + 0.2] }));
  const spot = afTagSpot(af, over, frame);
  assert.equal(spot, 'above');
  const tag = { ...where[spot], w, h: TAG_HEIGHT };
  for (const b of over) assert.equal(hit(tag, { x: b.box[0] * 400, y: b.box[1] * 300, w: tagWidth(boxLabel(b)), h: TAG_HEIGHT }), false);
  // Now the spot above is taken too.
  const all = [...over, { cls: 'car', conf: 0.5, box: [where.above.x / 400, where.above.y / 300, where.above.x / 400 + 0.3, where.above.y / 300 + 0.1] }];
  assert.equal(afTagSpot(af, all, frame), null, 'every spot taken: the dashed box alone says where the camera focused');
  assert.equal(afTagSpot([0.3, 0.45, 0.34, 0.62], [], frame), null, 'a box too narrow for its tag has none');
  assert.equal(afTagSpot(null, [], frame), null);
  assert.equal(afTagSpot(af, [], { width: 0, height: 0 }), null);
});

test('the autofocus tag goes outside a box too short for it, and never off the picture', () => {
  const frame = { width: 400, height: 300 };
  assert.equal(afTagSpot([0.3, 0.5, 0.6, 0.52], [], frame), 'below', 'a 6 px tall box cannot hold a 15 px tag');
  assert.equal(afTagSpot([0.3, 0.97, 0.6, 0.99], [], frame), 'above', 'below would fall off the picture');
});

test('the camera-focus car is main, and every other car is background', () => {
  const boxes = [
    { cls: 'car', conf: 0.9, box: [0.0, 0.2, 0.2, 0.6] },
    { cls: 'car', conf: 0.8, box: [0.4, 0.3, 0.9, 0.8] },
    { cls: 'person', conf: 0.5, box: [0.1, 0.1, 0.2, 0.5] },
  ];
  const main = mainVehicle(boxes, [0.5, 0.5, 0.55, 0.6]);
  assert.deepEqual(main, { index: 1, byAf: true });
  assert.equal(boxRole(boxes[1], 1, boxes, [0.5, 0.5, 0.55, 0.6]), 'main');
  assert.equal(boxRole(boxes[0], 0, boxes, [0.5, 0.5, 0.55, 0.6]), 'background');
  assert.equal(boxRole(boxes[2], 2, boxes, [0.5, 0.5, 0.55, 0.6]), null, 'a person is never the judged car');
  assert.equal(roleLabel(boxes[1], 'main', true), 'MAIN · AF');
  assert.equal(roleLabel(boxes[0], 'background'), 'background');
  // Focus on empty ground, or no focus data: the biggest vehicle is the main one.
  assert.deepEqual(mainVehicle(boxes, [0.95, 0.05, 0.99, 0.1]), { index: 1, byAf: false });
  assert.deepEqual(mainVehicle(boxes, null), { index: 1, byAf: false });
  assert.equal(roleLabel(boxes[1], 'main', false), 'MAIN · biggest');
  assert.deepEqual(mainVehicle([], null), { index: -1, byAf: false });
});

test('the time left names the scan and the identify pass separately', () => {
  assert.deepEqual(etaLines({ scanEta: 300, scanStable: true }), ['about 5 min left']);
  const both = etaLines({ vision: true, scanEta: 300, scanStable: true, identify: { state: 'running', done: 0, total: 3000 } });
  assert.equal(both.length, 2);
  assert.equal(both[0], 'about 5 min left');
  assert.match(both[1], /^Then identify estimating… \(3,000 cars\)$/);
  const reading = etaLines({ vision: true, scanDone: true, identify: { state: 'running', done: 10, total: 30, elapsed_ms: 20000 } });
  assert.equal(reading[0], 'Scan done');
  assert.match(reading[1], /^Identify about 40 s left \(30 cars\)$/);
  assert.deepEqual(etaLines({ vision: true, scanDone: true, identify: { state: 'done', done: 30, total: 30 } }), ['Scan done', 'Identify done']);
});
