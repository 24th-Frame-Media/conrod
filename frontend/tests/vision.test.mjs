import test from 'node:test';
import assert from 'node:assert/strict';
import {
  VISION_OFF_REASONS, albumVisionChoice, albumVisionHint, albumVisionOn, nextAlbumVision, visionHeader, visionLaneWords, visionModelWords, visionNodeSub,
  visionOffReason,
} from '../src/lib/vision.mjs';
import { flowNodes } from '../src/lib/identifyFlow.mjs';
import { identifyNodes, ingestNodes } from '../src/lib/ingest.mjs';

const job = (settings) => ({ settings_json: JSON.stringify(settings) });

test('only a reason the engine knows counts as vision being off', () => {
  assert.deepEqual([...VISION_OFF_REASONS], ['album', 'settings', 'no_service', 'lanes']);
  assert.equal(visionOffReason({ vision_off_reason: 'album' }), 'album');
  assert.equal(visionOffReason({ vision_off_reason: null }), null);
  assert.equal(visionOffReason({ vision_off_reason: 'because' }), null, 'junk is not a reason');
  assert.equal(visionOffReason({}), null, 'an older snapshot has none');
  assert.equal(visionOffReason(undefined), null);
});

test('the header says why vision is off, in the photographer\'s words', () => {
  assert.equal(visionHeader('album'), 'Reading plates and numbers \u2014 vision model off for this album');
  assert.match(visionHeader('settings'), /off in Settings/);
  assert.match(visionHeader('no_service'), /no vision service set up/);
  assert.match(visionHeader('lanes'), /does not ask the vision model/);
  for (const reason of VISION_OFF_REASONS) assert.doesNotMatch(visionHeader(reason), /\boff\b$|\bnull\b|undefined/i);
});

test('the VISION node and the worker card never call "off" a model', () => {
  assert.equal(visionNodeSub('album'), 'off for this album');
  assert.equal(visionNodeSub('no_service'), 'no service set up');
  const album = visionLaneWords('album');
  assert.equal(album.title, 'Vision off');
  assert.match(album.why, /turned vision off for this album/);
  assert.deepEqual(album.action, { id: 'album', label: 'Turn vision on for this album' });
  assert.deepEqual(visionLaneWords('settings').action, { id: 'album', label: 'Turn vision on for this album' }, 'Settings off: the album can still turn it on');
  assert.deepEqual(visionLaneWords('no_service').action, { id: 'service', label: 'Set up a vision service' });
  assert.equal(visionLaneWords('lanes').action, null, 'nothing to switch on: the shoot type does not ask');
  for (const reason of VISION_OFF_REASONS) {
    const words = visionLaneWords(reason);
    assert.doesNotMatch(`${words.title} ${words.why}`, /Make not read|No prompt/);
  }
});

test('the friendly model name reads Qwen3-VL for the local default', () => {
  assert.equal(visionModelWords('qwen3-vl:8b-instruct-q4_K_M'), 'Qwen3-VL · this PC');
  assert.equal(visionModelWords('qwen3-vl:8b-instruct-q4_K_M via ollama'), 'Qwen3-VL · this PC');
  assert.equal(visionModelWords('qwen2.5vl:7b'), 'Qwen2.5-VL · this PC', 'the older tag keeps its old name');
});

test('the flow row and the ingest flowchart show the reason where the model name goes', () => {
  const live = { state: 'running', stage: 'vision', model: null, vision_off_reason: 'album', done: 3, total: 12, workers: 3, current: null, active: [], recent: [], errors: [] };
  const vision = flowNodes(live, 'qwen2.5vl:7b')[3];
  assert.equal(vision.sub, 'off for this album', 'the reason wins over a fallback model name');
  assert.equal(vision.off, true);
  assert.equal(flowNodes({ stage: 'vision' }, 'qwen2.5vl:7b')[3].off, false);
  assert.equal(flowNodes(live, 'qwen')[2].off, false, 'only the VISION node is dimmed');

  const stages = [{ id: 'identify', label: 'Identify', model: '', workers_total: 3, total: 12, done: 3 }];
  const node = ingestNodes({ stages, identify: live, model: 'qwen2.5vl:7b' }).find((n) => n.id === 'vision');
  assert.equal(node.sub, 'off for this album');
  assert.equal(node.off, true);
  assert.doesNotMatch(node.meta, /qwen|off/, 'no model name, and "off" is never one');
  assert.match(node.meta, /3 workers/);
  const on = ingestNodes({ stages, identify: { ...live, model: 'glm-5.3-flash', vision_off_reason: null }, model: 'x' }).find((n) => n.id === 'vision');
  assert.equal(on.sub, 'Reading cars — glm-5.3-flash');
  assert.equal(on.off, false);
  assert.match(on.meta, /3 \/ 12 cars/);

  const row = identifyNodes(live, 'qwen').find((n) => n.id === 'vision');
  assert.equal(row.sub, 'off for this album');
  assert.equal(row.off, true);
});

test('an album follows Settings until it has a choice of its own', () => {
  assert.equal(albumVisionChoice(job({ vlm_inherit: 1 })), null);
  assert.equal(albumVisionChoice(job({ vlm_inherit: 1, use_vlm: false })), false);
  assert.equal(albumVisionChoice(job({ vlm_inherit: 1, use_vlm: true })), true);
  assert.equal(albumVisionChoice(job({ use_vlm: false })), null, 'an old album\'s use_vlm is only a stale copy');
  assert.equal(albumVisionChoice({ settings_json: 'not json' }), null);
  assert.equal(albumVisionChoice(undefined), null);

  assert.equal(albumVisionOn(job({ vlm_inherit: 1 }), { use_vlm: true }), true);
  assert.equal(albumVisionOn(job({ vlm_inherit: 1 }), { use_vlm: false }), false, 'follows Settings off');
  assert.equal(albumVisionOn(job({ vlm_inherit: 1, use_vlm: true }), { use_vlm: false }), true, 'on for this album alone');
  assert.equal(albumVisionOn(job({ vlm_inherit: 1, use_vlm: false }), { use_vlm: true }), false);
  assert.equal(albumVisionOn(job({ use_vlm: false }), { use_vlm: true }), true, 'a stale copy is ignored');
});

test('the switch stores a choice only when it differs from Settings', () => {
  assert.equal(nextAlbumVision(true, true), null);
  assert.equal(nextAlbumVision(false, false), null);
  assert.equal(nextAlbumVision(false, true), false);
  assert.equal(nextAlbumVision(true, false), true);
});

test('the switch caption says where the answer comes from', () => {
  const globals = { use_vlm: true, vlm_model: 'qwen2.5vl:7b', vlm_services: [{ enabled: false, model: 'old' }, { enabled: true, model: 'glm-5.3-flash' }] };
  assert.equal(albumVisionHint(null, globals), 'Follows Settings: on, using glm-5.3-flash.');
  assert.equal(albumVisionHint(null, { use_vlm: true, vlm_model: 'qwen2.5vl:7b' }), 'Follows Settings: on, using qwen2.5vl:7b.');
  assert.match(albumVisionHint(null, { use_vlm: false }), /Off in Settings/);
  assert.match(albumVisionHint(false, globals), /Off for this album/);
  assert.match(albumVisionHint(true, { use_vlm: false }), /although it is off in Settings/);
  assert.equal(albumVisionHint(true, globals), 'On for this album.');
});
