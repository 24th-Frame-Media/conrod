import assert from 'node:assert/strict';
import test from 'node:test';
import {
  barTone, canInstall, deciderActive, deciderNode, deciderWord, msWords, percent, sizeWords, statusLine,
} from '../src/lib/decider.mjs';
import { identifyNodes, ingestNodes } from '../src/lib/ingest.mjs';
import { FIELDS, SECTIONS } from '../src/lib/settings.mjs';

const live = (extra = {}) => ({
  state: 'ready', enabled: true, message: 'Ready.', model: 'pplx-decider-flash-v1.2', device: 'RTX 3070 Ti', quant: 'int8',
  capacity: 1, busy: 0, queued: 0, done: 0, errors: 0, avg_ms: null, current: null, recent: [], install: null, ...extra,
});

test('the node says why it is dimmed and counts frames once it works', () => {
  for (const [state, word] of [['off', 'off'], ['not_installed', 'not installed'], ['no_vram', 'low VRAM'], ['no_gpu', 'no GPU']]) {
    const node = deciderNode(live({ state }));
    assert.equal(node.off, true, state);
    assert.equal(node.sub, word);
    assert.equal(node.state, 'pending');
    assert.equal(node.meta, '', 'no counters on an off node');
  }
  const idle = deciderNode(live());
  assert.deepEqual([idle.off, idle.state, idle.sub], [false, 'pending', 'ready']);
  const busy = deciderNode(live({ state: 'running', busy: 1, done: 12, errors: 1, avg_ms: 340 }));
  assert.deepEqual([busy.off, busy.state, busy.sub], [false, 'active', '12 frames']);
  assert.match(busy.meta, /1 failed/);
  assert.match(busy.meta, /int8/);
  assert.match(busy.meta, /340 ms/);
  const finished = deciderNode(live({ done: 40, avg_ms: 1300 }));
  assert.deepEqual([finished.state, finished.sub], ['done', '40 frames']);
  assert.match(finished.meta, /1\.3 s/);
  assert.equal(busy.filter, 'decider');
});

test('a node that worked is not dimmed after the model is turned off', () => {
  assert.equal(deciderNode(live({ state: 'off', done: 3 })).off, false);
});

test('the DECIDER node sits after VISION and before DONE, and only when the engine reports the model', () => {
  const stages = [{ id: 'read', label: 'Read', done: 5, total: 10, workers_total: 2, workers_busy: 1 }, { id: 'identify', label: 'Identify', done: 0, total: 3 }];
  const without = ingestNodes({ stages }).map((n) => n.id);
  assert.equal(without.includes('decider'), false);
  const nodes = ingestNodes({ stages, decider: live({ state: 'off' }) });
  const ids = nodes.map((n) => n.id);
  assert.deepEqual(ids.slice(-3), ['vision', 'decider', 'done']);
  assert.equal(nodes.find((n) => n.id === 'decider').filter, 'decider');
  assert.deepEqual(nodes.map((n) => n.number), nodes.map((_, i) => String(i + 1).padStart(2, '0')));
});

test('the identify flow shows the decision model beside vision, with its question and answer', () => {
  const asking = live({
    state: 'running', busy: 1, done: 12, avg_ms: 340,
    current: { label: 'a.CR3', thumb: null, queued: false, elapsed_ms: 10, asking: [{ id: 'm_wheel_lift', kind: 'noul', text: 'Is a wheel off the ground?', options: 2 }] },
  });
  const nodes = identifyNodes({ stage: 'vision', state: 'running' }, 'qwen', asking);
  assert.equal(nodes.length, 7);
  assert.equal(nodes[3].label, 'VISION');
  const node = nodes[4];
  assert.equal(node.label, 'DECIDER');
  assert.equal(node.sub, 'Is a wheel off the ground?');
  assert.equal(node.state, 'active');
  assert.match(node.meta, /12 frames/);
  assert.match(node.meta, /340 ms\/frame/);
  assert.deepEqual(nodes.map((n) => n.number), ['01', '02', '03', '04', '05', '06', '07']);
  // Once it has answered, the node keeps the frame count and adds its pick.
  const answered = identifyNodes({ stage: 'vision' }, 'qwen', live({
    done: 30, avg_ms: 900,
    recent: [{ label: 'a.CR3', thumb: null, ms: 900, tokens: 900, ok: true, says: [{ id: 'm_wheel_lift', kind: 'noul', text: 'Is a wheel off the ground?', label: 'yes', p: 0.91, bars: [] }] }],
  }))[4];
  assert.equal(answered.state, 'done');
  assert.match(answered.meta, /30 frames/);
  assert.match(answered.meta, /yes 91%/);
  // Off: the node is dimmed and says why, never empty.
  const off = identifyNodes({ stage: 'vision' }, 'qwen', live({ state: 'off' }))[4];
  assert.equal(off.off, true);
  assert.equal(off.sub, 'off');
  // No decider in the snapshot: the original six nodes are unchanged.
  assert.equal(identifyNodes({ stage: 'vision' }, 'qwen').length, 6);
});

test('the lane shows once the model is awake or has answered something', () => {
  assert.equal(deciderActive(null), false);
  assert.equal(deciderActive(live({ state: 'off' })), false);
  assert.equal(deciderActive(live()), false);
  assert.equal(deciderActive(live({ state: 'loading' })), true);
  assert.equal(deciderActive(live({ busy: 1 })), true);
  assert.equal(deciderActive(live({ recent: [{ label: 'x', thumb: null, ms: 1, tokens: 1, ok: true, says: [] }] })), true);
});

test('small formatters', () => {
  assert.equal(percent(0.874), '87%');
  assert.equal(percent(0.004), '<1%');
  assert.equal(percent(0), '0%');
  assert.equal(percent(2), '100%');
  assert.equal(barTone(0.9), 'good');
  assert.equal(barTone(0.65), 'warn');
  assert.equal(barTone(0.5), 'none');
  assert.equal(msWords(420), '420 ms');
  assert.equal(msWords(1340), '1.3 s');
  assert.equal(msWords(null), '');
  assert.equal(sizeWords(5_868_000_000), '5.9 GB');
  assert.equal(sizeWords(672_423_040), '672 MB');
  assert.equal(deciderWord('running'), 'deciding');
});

test('the settings card explains each state and offers install only where it can work', () => {
  const status = (extra = {}) => ({
    state: 'not_installed', enabled: false, message: '', quant: 'int8', forced: null, installed: false, needs_install: true, download_bytes: 5_868_000_000,
    gpu: { name: 'GPU', total: 8 * 2 ** 30, free: 7 * 2 ** 30 }, needs: { int8: 6.5 * 2 ** 30, int4: 4.5 * 2 ** 30 }, install: null, ...extra,
  });
  const fresh = statusLine(status());
  assert.equal(fresh.title, 'Not installed');
  assert.match(fresh.detail, /5\.9 GB/);
  assert.equal(canInstall(status()), true);
  const low = status({ state: 'no_vram', quant: null, message: 'Not enough video memory: 3.0 GB free, needs 4.5 GB.' });
  assert.equal(statusLine(low).tone, 'bad');
  assert.match(statusLine(low).detail, /3\.0 GB free/);
  assert.equal(canInstall(low), false);
  assert.equal(canInstall(status({ state: 'no_gpu', quant: null, gpu: null })), false);
  const going = status({ state: 'installing', install: { done: 2_934_000_000, total: 5_868_000_000 } });
  assert.equal(statusLine(going).title, 'Installing… 50%');
  assert.equal(canInstall(going), false);
  assert.equal(canInstall(status({ state: 'ready', installed: true, needs_install: false, download_bytes: 0 })), false);
  // Off but not installed: install is offered so it can be turned on afterwards.
  assert.equal(canInstall(status({ state: 'off' })), true);
  // int4 on a card that already has the int8 file: the button prepares the smaller file (nothing to download).
  assert.equal(canInstall(status({ state: 'not_installed', installed: true, download_bytes: 0, quant: 'int4' })), true);
  assert.equal(statusLine(null).title, 'Checking…');
});

test('the settings page has the decision toggle and precision override in the AI models section', () => {
  const section = SECTIONS.find((s) => s.id === 'ai-models');
  assert.ok(section.keys.includes('use_decider') && section.keys.includes('decider_quant'));
  assert.ok(section.keys.includes('moments_transport'), 'and where it runs');
  assert.equal(FIELDS.use_decider.kind, 'toggle');
  assert.deepEqual(FIELDS.decider_quant.options.map((o) => o.id), ['auto', 'int8', 'int4']);
  assert.deepEqual(FIELDS.moments_transport.options.map((o) => o.id), ['auto', 'in_process', 'openrouter']);
});