import test from 'node:test';
import assert from 'node:assert/strict';
import {
  FLOW_NODES, IDENTIFY_STAGES, actionLabel, activeStage, confidencePercent, confidenceTone, deciderStepNode, displayItem,
  displayPath, elapsed, flowNodes, phaseLabel, prettyReply, progressPercent, rateLabel, sourceLabel, visibleWorkerLanes, workerGridMode, workerLanes,
} from '../src/lib/identifyFlow.mjs';
import { SUBJECT_WORDS } from '../src/lib/subjects.mjs';
const vehicles = SUBJECT_WORDS.vehicles;

test('the stages are the engine contract, in flow order', () => {
  assert.deepEqual(IDENTIFY_STAGES, ['read', 'crop', 'vision', 'merge', 'done']);
  assert.equal(FLOW_NODES.length, 6);
  assert.deepEqual(FLOW_NODES.map((node) => node.label), ['LOAD', 'CROP', 'READ', 'VISION', 'ENTRY LIST', 'RESULT']);
  assert.deepEqual(FLOW_NODES.map((node) => node.number), ['01', '02', '03', '04', '05', '06']);
});

test('a stage maps onto the node that is lit', () => {
  assert.equal(activeStage('read'), 0);
  assert.equal(activeStage('crop'), 1);
  assert.equal(activeStage('vision'), 2);
  assert.equal(activeStage('merge'), 4);
  assert.equal(activeStage('done'), 5);
  assert.equal(activeStage(null), 0, 'before the first car the flow is at the start');
  assert.equal(activeStage(undefined), 0);
  assert.equal(activeStage('nonsense'), 0);
});

test('flow nodes mark done, active and pending; vision lights both AI nodes', () => {
  const nodes = flowNodes({ stage: 'vision', state: 'running' });
  assert.deepEqual(nodes.map((node) => node.state), ['done', 'done', 'active', 'active', 'pending', 'pending']);
  assert.equal(nodes[2].label, 'READ');
  assert.equal(nodes[3].label, 'VISION');
  assert.equal(flowNodes({ stage: 'vision' }, 'qwen2.5vl:7b', null, vehicles)[3].sub, 'Reading vehicles — Qwen2.5-VL', 'the VISION node says what it does and names the model plainly');
  assert.equal(flowNodes({ stage: 'vision' }, 'qwen2.5vl:7b')[2].sub, 'plate + OCR', 'the other sublines stay put');
  // During the OCR phase, only READ lights up while VISION stays pending.
  const ocrNodes = flowNodes({ stage: 'vision', state: 'running', phase: { name: 'ocr', done: 10, total: 50, state: 'running' } }, 'qwen3-vl:8b-instruct-q4_K_M', null, vehicles);
  assert.deepEqual(ocrNodes.map((node) => node.state), ['done', 'done', 'active', 'pending', 'pending', 'pending']);
  // During Decision (moments/peaks), LOAD/CROP/READ are done and VISION/ENTRY LIST/RESULT stay pending (no stale CROP pulse).
  const deciderNodes = flowNodes({ stage: 'crop', state: 'running', phase: { name: 'moments', done: 1221, total: 1572, state: 'running' } }, 'qwen3-vl:8b-instruct-q4_K_M', null, vehicles);
  assert.deepEqual(deciderNodes.map((node) => node.state), ['done', 'done', 'done', 'pending', 'pending', 'pending']);
});

test('a finished run has nothing left to pulse, and no snapshot starts at LOAD', () => {
  assert.ok(flowNodes({ stage: 'done', state: 'done' }).every((node) => node.state === 'done'));
  assert.ok(flowNodes({ stage: 'done', state: 'stopped' }).every((node) => node.state === 'done'));
  assert.deepEqual(flowNodes(null).map((node) => node.state), ['active', 'pending', 'pending', 'pending', 'pending', 'pending']);
  assert.equal(flowNodes(undefined)[0].label, 'LOAD');
});

test('the flow follows the car on screen, and a held result completes it', () => {
  // The live car's own stage wins over the run-wide stage while it is being read.
  const live = { stage: 'vision', current: { detection_id: 1, stage: 'crop' }, state: 'running' };
  assert.deepEqual(flowNodes(live).map((node) => node.state), ['done', 'active', 'pending', 'pending', 'pending', 'pending']);
  // A held finished car shows every node done, even while other workers read on.
  assert.ok(flowNodes({ ...live, current: { detection_id: 2, stage: 'vision' } }, null, 'done').every((node) => node.state === 'done'));
});

test('one lane per worker: the live car, else its last finished car', () => {
  const live = {
    workers: 3,
    active: [
      { detection_id: 10, worker: 0, stage: 'vision' },
      { detection_id: 11, worker: 2, stage: 'crop' },
    ],
    recent: [
      { detection_id: 9, worker: 0, stage: 'done' },
      { detection_id: 8, worker: 1, stage: 'done' },
      { detection_id: 7, worker: 1, stage: 'done' },
    ],
  };
  const lanes = workerLanes(live);
  assert.deepEqual(lanes.map((lane) => lane.worker), [0, 1, 2]);
  assert.deepEqual(lanes.map((lane) => lane.item?.detection_id), [10, 8, 11], 'a live car beats the last finished one, and the newest finished car represents its worker');
  assert.deepEqual(lanes.map((lane) => lane.live), [true, false, true]);
  assert.equal(workerLanes(null).length, 0);
  assert.equal(workerLanes({ active: [], recent: [] }).length, 0);
  // Snapshots without a worker count still lay out one lane per worker seen.
  assert.equal(workerLanes({ active: [{ detection_id: 5, worker: 1 }], recent: [] }).length, 2);
});

test('visibleWorkerLanes hides empty placeholders and hides non-live cards during Decision', () => {
  const lanes = [
    { worker: 0, item: { detection_id: 1 }, live: true },
    { worker: 1, item: { detection_id: 2 }, live: false },
    { worker: 2, item: null, live: false },
  ];
  // Normal OCR/Vision: keeps live and finished lanes, drops blank idle placeholders.
  assert.deepEqual(visibleWorkerLanes(lanes, { name: 'ocr', state: 'running' }).map((l) => l.worker), [0, 1]);
  assert.deepEqual(visibleWorkerLanes(lanes, null).map((l) => l.worker), [0, 1]);
  // Decision (moments/peaks): only live lanes survive, so blank/stale vision cards never crowd DECIDER.
  assert.deepEqual(visibleWorkerLanes(lanes, { name: 'moments', state: 'running' }).map((l) => l.worker), [0]);
  assert.deepEqual(visibleWorkerLanes([{ worker: 0, item: null, live: false }], { name: 'peaks', state: 'running' }), []);
  // Once Decision ends, finished lanes are visible again.
  assert.deepEqual(visibleWorkerLanes(lanes, { name: 'peaks', state: 'done' }).map((l) => l.worker), [0, 1]);
});

test('workerGridMode adapts hero / cards / tiles automatically and honours density overrides', () => {
  assert.equal(workerGridMode(1, { soloDecider: true }), 'hero');
  assert.equal(workerGridMode(1, { soloDecider: false }), 'cards');
  assert.equal(workerGridMode(3, { soloDecider: false }), 'cards');
  assert.equal(workerGridMode(4, { soloDecider: false }), 'tiles');
  assert.equal(workerGridMode(9, { soloDecider: false }), 'tiles');
  assert.equal(workerGridMode(6, { density: 'cards' }), 'cards');
  assert.equal(workerGridMode(2, { density: 'tiles' }), 'tiles');
  assert.equal(workerGridMode(1, { soloDecider: true, density: 'tiles' }), 'tiles');
});

test('an in-flight car can be pinned before it finishes', () => {
  const live = {
    current: { detection_id: 10 },
    active: [{ detection_id: 10 }, { detection_id: 11 }],
    recent: [{ detection_id: 9 }],
  };
  assert.equal(displayItem(live, 11).detection_id, 11, 'a pinned worker car shows while it is read');
  assert.equal(displayItem(live, 9).detection_id, 9, 'a finished car still shows');
  assert.equal(displayItem(live, 99).detection_id, 10, 'a stale pick falls back to the narrated car');
});

test('nothing to show stays empty instead of crashing', () => {
  assert.equal(displayPath(null), '');
  assert.equal(displayPath(undefined), '');
  assert.equal(displayPath({}), '');
  assert.equal(displayItem(null), null);
  assert.equal(displayItem(undefined, 42), null);
  assert.equal(displayItem({ current: null, recent: [] }), null);
  assert.equal(prettyReply(null), '');
  assert.equal(prettyReply(undefined), '');
  assert.equal(rateLabel(0, 1000), '');
  assert.equal(rateLabel(12, 0), '');
});

test('the stage prefers the crop, then the preview, then the thumb', () => {
  const item = { crop_path: 'crop.jpg', preview_path: 'preview.jpg', thumb_path: 'thumb.jpg', path: 'raw.cr3' };
  assert.equal(displayPath(item), 'crop.jpg');
  assert.equal(displayPath({ ...item, crop_path: null }), 'preview.jpg');
  assert.equal(displayPath({ ...item, crop_path: null, preview_path: null }), 'thumb.jpg');
  assert.equal(displayPath({ ...item, crop_path: null, preview_path: null, thumb_path: null }), 'raw.cr3');
  assert.equal(displayPath({ crop_path: '', preview_path: 'preview.jpg' }), 'preview.jpg');
});

test('percent helpers clamp to 0..100 and never print NaN', () => {
  assert.equal(progressPercent(12, 1090), 1);
  assert.equal(progressPercent(545, 1090), 50);
  assert.equal(progressPercent(1090, 1090), 100);
  assert.equal(progressPercent(2000, 1090), 100);
  assert.equal(progressPercent(-5, 100), 0);
  assert.equal(progressPercent(1, 0), 0);
  assert.equal(progressPercent(null, null), 0);
  assert.equal(progressPercent(undefined, undefined), 0);
  assert.equal(confidencePercent(0.95), 95);
  assert.equal(confidencePercent(1.2), 100);
  assert.equal(confidencePercent(-0.2), 0);
  assert.equal(confidencePercent(null), null);
  assert.equal(confidencePercent(undefined), null);
});

test('confidence tone crosses at 0.8 and 0.5', () => {
  assert.equal(confidenceTone(0.95), 'good');
  assert.equal(confidenceTone(0.8), 'good');
  assert.equal(confidenceTone(0.79), 'warn');
  assert.equal(confidenceTone(0.5), 'warn');
  assert.equal(confidenceTone(0.49), 'bad');
  assert.equal(confidenceTone(0), 'bad');
  assert.equal(confidenceTone(null), 'none');
});

test('the model reply prints as pretty JSON, null-safe', () => {
  assert.equal(prettyReply({ number: '96', make: 'Toyota' }), '{\n  "number": "96",\n  "make": "Toyota"\n}');
  assert.equal(prettyReply('{"number":"96"}'), '{\n  "number": "96"\n}');
  assert.equal(prettyReply('not json'), 'not json');
  assert.equal(prettyReply(null), '');
});

test('the source chip says where the number came from', () => {
  assert.equal(sourceLabel('vlm'), 'VLM');
  assert.equal(sourceLabel('ocr+entry'), 'OCR+entry');
  assert.equal(sourceLabel('entry'), 'entry');
  assert.equal(sourceLabel(''), '');
  assert.equal(sourceLabel(null), '');
  assert.equal(sourceLabel(undefined), '');
});

test('action tags read as words, and an unknown tag passes through', () => {
  assert.equal(actionLabel('airborne'), 'Airborne');
  assert.equal(actionLabel('tyre_smoke'), 'Tyre smoke');
  assert.equal(actionLabel('wheel_lift'), 'Wheel lift');
  assert.equal(actionLabel('mystery'), 'mystery');
  assert.equal(actionLabel(null), '');
});

const live = {
  current: { detection_id: 55, number: '96' },
  recent: [{ detection_id: 54, number: '5' }, { detection_id: 53, number: '7' }],
};

test('the strip can pin a finished car, and falls back to the live one', () => {
  assert.equal(displayItem(live, null).detection_id, 55);
  assert.equal(displayItem(live, undefined).detection_id, 55);
  assert.equal(displayItem(live, 54).detection_id, 54);
  assert.equal(displayItem(live, 999).detection_id, 55, 'a stale pick falls back to the current car');
  assert.equal(displayItem({ current: null, recent: live.recent }, null).detection_id, 54, 'before the first live car, the newest finished one shows');
});

test('elapsed and rate read like a stopwatch, not a debug dump', () => {
  assert.equal(elapsed(1234), '1.2 s');
  assert.equal(elapsed(42000), '42 s');
  assert.equal(elapsed(123456), '2 min 03 s');
  assert.equal(elapsed(3723000), '1 h 02 min');
  assert.equal(elapsed(null), '');
  assert.equal(elapsed(-5), '');
  assert.equal(rateLabel(12, 30000, vehicles), '24 vehicles/min');
  assert.equal(rateLabel(1, 8000, vehicles), '8.0 s/vehicle');
  assert.equal(rateLabel(0, 8000), '');
});

test('a phase reads as a count in words, and an unknown one as nothing', () => {
  assert.equal(phaseLabel({ name: 'ocr', done: 82, total: 499, state: 'running' }), 'Reading plates and numbers 82/499');
  assert.equal(phaseLabel({ name: 'peaks', done: 12, total: 214, state: 'running' }), 'Comparing bursts 12/214');
  assert.equal(phaseLabel({ name: 'moments', done: 3, total: 40, state: 'running' }), 'Judging moments 3/40');
  assert.equal(phaseLabel({ name: 'peaks', done: 0, total: 0 }), 'Comparing bursts', 'before the first burst there is still a name');
  assert.equal(phaseLabel({ name: 'nonsense', total: 3 }), '');
  assert.equal(phaseLabel(null), '');
  assert.equal(phaseLabel(undefined), '');
});

test('the DECIDER node ignores the OCR phase and ticks only when the moments lane has finished', () => {
  const decider = { state: 'ready', done: 5, busy: 0, current: null, recent: [] };
  // OCR is not a decider phase: a waiting decider must not falsely pulse active during OCR.
  const duringOcr = deciderStepNode({ ...decider, done: 0 }, null, { name: 'ocr', done: 82, total: 499, state: 'running' });
  assert.equal(duringOcr.state, 'pending');
  assert.equal(duringOcr.sub, 'waiting');
  // Idle between frames, but the lane has not finished: no tick yet.
  const between = deciderStepNode(decider, null, { name: 'moments', done: 5, total: 5, state: 'running' });
  assert.equal(between.state, 'active');
  // The last car is read; the bursts still to be compared carry their own count.
  const peaks = deciderStepNode(decider, null, { name: 'peaks', done: 12, total: 214, state: 'running' });
  assert.equal(peaks.state, 'active');
  assert.equal(peaks.sub, 'Comparing bursts 12/214');
  assert.match(peaks.meta, /Comparing bursts 12\/214/);
  // Only the phase's own terminal checks off.
  assert.equal(deciderStepNode(decider, null, { name: 'peaks', done: 214, total: 214, state: 'done' }).state, 'done');
  // A run that stopped mid-phase leaves work undone: the node is pending, not pulsing.
  const stopped = deciderStepNode(decider, null, { name: 'peaks', done: 12, total: 214, state: 'stopped' });
  assert.equal(stopped.state, 'pending');
  assert.equal(stopped.sub, 'Comparing bursts 12/214', 'the remaining work stays visible');
  // A cloud service names the node even while the local model is not installed, and its tick waits too.
  const cloud = deciderStepNode({ ...decider, state: 'not_installed' }, 'jev-1.13 via OpenCode', { name: 'peaks', done: 12, total: 214, state: 'running' });
  assert.deepEqual([cloud.off, cloud.sub, cloud.state], [false, 'jev-1.13 via OpenCode', 'active']);
  assert.doesNotMatch(cloud.title, /not installed/i);
  assert.equal(deciderStepNode(decider, 'jev-1.13 via OpenCode', { name: 'peaks', done: 214, total: 214, state: 'done' }).state, 'done');
  // A serving node with no phase still ticks on its own frames, as before.
  assert.equal(deciderStepNode(decider, 'jev-1.13 via OpenCode').state, 'done');
  // Without a phase (an older engine) the old rule stands, so nothing else changes.
  assert.equal(deciderStepNode(decider).state, 'done');
  assert.equal(deciderStepNode({ ...decider, done: 0 }).state, 'pending');
  assert.equal(deciderStepNode(null), null);
});
