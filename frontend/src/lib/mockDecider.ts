/* Dev-only stand-in for the engine's decision-model feed (`?mock=1`): a frame every couple of seconds, six yes/no questions and
   one choice, answered with plausible probabilities, so the DECIDER node and lane animate in `vite dev` like the real one.
   `&decider=off|novram|nogpu|install|none` shows the other states. Imported by mock.ts only, so it is not in `dist`. */
import type { DeciderAsk, DeciderLive, DeciderResult, DeciderSaid, DeciderState, DeciderStatus } from './types';

const QUESTIONS: DeciderAsk[] = [
  { id: 'sharp', kind: 'noul', text: 'Is the main subject in sharp focus?', options: 2 },
  { id: 'action', kind: 'noul', text: 'Is something dramatic happening in this frame?', options: 2 },
  { id: 'overtake', kind: 'noul', text: 'Are two subjects side by side or passing?', options: 2 },
  { id: 'crowd', kind: 'noul', text: 'Is there a crowd or spectator in view?', options: 2 },
  { id: 'dust', kind: 'noul', text: 'Is dust, spray or smoke thrown up?', options: 2 },
  { id: 'panning', kind: 'noul', text: 'Is the background streaked by a pan?', options: 2 },
  { id: 'moment', kind: 'choice', text: 'Which kind of moment is this?', options: 4 },
];
const MOMENTS = ['apex', 'overtake', 'incident', 'ordinary'];

/** A deterministic 0..1 from two integers, so the same frame always gets the same answer. */
const noise = (k: number, j: number) => { const x = Math.sin(k * 12.9898 + j * 78.233) * 43758.5453; return x - Math.floor(x); };

function said(k: number, ask: DeciderAsk, j: number): DeciderSaid {
  if (ask.kind === 'choice') {
    const raw = MOMENTS.map((_, i) => 0.15 + noise(k, 40 + i) * (i === k % 4 ? 3 : 1));
    const sum = raw.reduce((a, b) => a + b, 0);
    const bars = MOMENTS.map((label, i) => ({ label, p: raw[i] / sum })).sort((a, b) => b.p - a.p);
    return { id: ask.id, kind: ask.kind, text: ask.text, label: bars[0].label, p: bars[0].p, bars };
  }
  const yes = Math.min(0.99, Math.max(0.01, noise(k, j) < 0.5 ? noise(k, j + 9) * 0.35 : 0.6 + noise(k, j + 9) * 0.39));
  return {
    id: ask.id, kind: ask.kind, text: ask.text, label: yes >= 0.5 ? 'yes' : 'no', p: Math.max(yes, 1 - yes),
    bars: [{ label: 'yes', p: yes }, { label: 'no', p: 1 - yes }],
  };
}

const variant = () => new URLSearchParams(location.search).get('decider') ?? 'run';

const STATE_OF: Record<string, DeciderState> = { off: 'off', novram: 'no_vram', nogpu: 'no_gpu', install: 'not_installed', none: 'ready' };

const GB = 2 ** 30;

/** The Settings card's status, in the same variants. */
export function mockDeciderStatus(): DeciderStatus {
  const v = variant();
  const state: DeciderState = v === 'run' ? 'ready' : STATE_OF[v] ?? 'ready';
  const lowVram = state === 'no_vram';
  return {
    state, enabled: state !== 'off', quant: lowVram || state === 'no_gpu' ? null : 'int8', forced: null,
    installed: state === 'ready' || state === 'off', needs_install: state === 'not_installed',
    download_bytes: state === 'not_installed' ? 5_868_000_000 : 0,
    gpu: state === 'no_gpu' ? null : { name: 'NVIDIA GeForce RTX 3070 Ti Laptop GPU', total: 8 * GB, free: lowVram ? 3 * GB : 7.1 * GB },
    needs: { int8: 6.5 * GB, int4: 4.5 * GB },
    message: lowVram ? 'Not enough video memory: 3.0 GB free, needs 4.5 GB.' : state === 'no_gpu' ? 'No graphics card was found for the decision model.' : state === 'off' ? 'Turned off in Settings.' : state === 'not_installed' ? 'Not installed yet.' : 'Ready.',
    install: null,
  };
}

/** Drives `emit` every 250 ms with a request in flight for ~1.3 s out of every ~2.6 s; `thumb(k)` is the frame picture. */
export function startDeciderSim(emit: (p: DeciderLive) => void, thumb: (k: number) => string): () => void {
  const v = variant();
  const base = (state: DeciderState, message: string): DeciderLive => ({
    state, enabled: state !== 'off', message, model: 'pplx-decider-flash-v1.2', device: 'NVIDIA GeForce RTX 3070 Ti Laptop GPU', quant: 'int8', capacity: 1,
    busy: 0, queued: 0, done: 0, errors: 0, avg_ms: null, current: null, recent: [], install: null,
  });
  if (v !== 'run' && v !== 'none') {
    const status = mockDeciderStatus();
    emit({ ...base(status.state, status.message), device: null, quant: status.quant });
    return () => undefined;
  }
  if (v === 'none') { emit(base('ready', 'Ready.')); return () => undefined; }
  const recent: DeciderResult[] = [];
  let k = 0, done = 0, total = 0;
  const started = Date.now();
  const timer = window.setInterval(() => {
    const t = (Date.now() - started) % 2600;
    const frame = Math.floor((Date.now() - started) / 2600);
    if (frame !== k) { k = frame; }
    const label = 'D:\\Photos\\2026\\10\\Challenge Bathurst\\IMG_' + (4200 + k) + '.CR3';
    const live = base('running', 'Answering questions.');
    if (t < 1300) {
      emit({ ...live, busy: 1, done, avg_ms: done ? Math.round(total / done) : null, recent: [...recent],
        current: { label, thumb: thumb(k), queued: false, elapsed_ms: t, asking: QUESTIONS } });
      return;
    }
    if (recent.length === 0 || recent[0].label !== label) {
      const ms = 300 + Math.round(noise(k, 3) * 120);
      recent.unshift({ label, thumb: thumb(k), ms, tokens: 560 + (k % 7), ok: true, says: QUESTIONS.map((q, j) => said(k, q, j)) });
      recent.length = Math.min(recent.length, 8);
      done += 1; total += ms;
    }
    emit({ ...base('ready', 'Ready.'), done, avg_ms: Math.round(total / done), recent: [...recent] });
  }, 250);
  return () => window.clearInterval(timer);
}