// Pure logic for the decision model's two surfaces: the DECIDER node of the ingest flowchart (and its lane) and the
// Settings card. The engine fills `DeciderLive` / `DeciderStatus`; see crates/conrod-engine/src/decider.rs.

/** States in which the model cannot run on this machine as things stand: the node is drawn dimmed. */
const UNAVAILABLE = new Set(['off', 'not_installed', 'installing', 'no_gpu', 'no_vram']);

const WORDS = {
  off: 'off', not_installed: 'not installed', installing: 'installing', no_gpu: 'no GPU', no_vram: 'low VRAM',
  ready: 'ready', loading: 'loading', running: 'deciding', error: 'error',
};

/** The short word for a state, as the node's subline. */
export function deciderWord(state) {
  return WORDS[state] ?? 'off';
}

/** Does the decision model have anything to show in the workers grid: it is awake, or it answered something. */
export function deciderActive(live) {
  if (!live) return false;
  return live.state === 'running' || live.state === 'loading' || live.busy > 0 || live.recent.length > 0 || live.current != null;
}

/** "420 ms" under a second, "1.3 s" above; '' for nothing. */
export function msWords(ms) {
  if (ms == null || !Number.isFinite(ms)) return '';
  return ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(1)} s`;
}

/** The DECIDER node of the flowchart, in the shape of the other nodes (see ingest.mjs `scanNode`). */
export function deciderNode(live) {
  const state = live?.state ?? 'off';
  const busy = (live?.busy ?? 0) > 0 || state === 'loading' || state === 'running';
  const answered = live?.done ?? 0;
  const off = UNAVAILABLE.has(state) && !busy && answered === 0;
  // The box says what the model is for; the file, precision and speed live in the meta line and the tooltip.
  const sub = off ? `Decision model · ${deciderWord(state)}` : 'Decision model';
  const frames = answered > 0 || busy ? `${answered.toLocaleString('en-US')} frames` : '';
  const failed = live?.errors > 0 ? `${live.errors.toLocaleString('en-US')} failed` : '';
  const meta = off ? '' : [frames, failed, live?.quant, live?.avg_ms ? msWords(live.avg_ms) : ''].filter(Boolean).join(' · ');
  const title = [sub, live?.message, live?.device, live?.model].filter(Boolean).join(' · ');
  return {
    id: 'decider', number: '', label: 'DECIDER', sub, meta, title,
    state: busy ? 'active' : answered > 0 ? 'done' : 'pending', off, filter: 'decider',
  };
}

/** A probability as "87%"; under 1% as "<1%". */
export function percent(p) {
  const v = Math.max(0, Math.min(1, Number(p) || 0)) * 100;
  return v > 0 && v < 1 ? '<1%' : `${Math.round(v)}%`;
}

/** The tone of an answer's bar: a confident one is good, a coin toss is neutral. */
export function barTone(p) {
  return p >= 0.8 ? 'good' : p >= 0.6 ? 'warn' : 'none';
}

/** Bytes as "5.9 GB" / "640 MB". */
export function sizeWords(bytes) {
  const b = Number(bytes) || 0;
  return b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.round(b / 1e6)} MB`;
}

/** The settings card's headline: tone, title and detail for a status. */
export function statusLine(status) {
  if (!status) return { tone: 'idle', title: 'Checking…', detail: '' };
  const gpu = status.gpu;
  switch (status.state) {
    case 'no_gpu': return { tone: 'bad', title: 'Not available', detail: status.message };
    case 'no_vram': return { tone: 'bad', title: 'Not enough video memory', detail: status.message };
    case 'error': return { tone: 'bad', title: 'Failed', detail: status.message };
    case 'installing': {
      const pct = status.install && status.install.total > 0 ? Math.round((status.install.done / status.install.total) * 100) : 0;
      return { tone: 'warn', title: `Installing… ${pct}%`, detail: status.install ? `${sizeWords(status.install.done)} of ${sizeWords(status.install.total)}` : '' };
    }
    case 'not_installed': return { tone: 'idle', title: 'Not installed', detail: `${sizeWords(status.download_bytes)} to download${status.quant === 'int4' ? ' (then prepared for your GPU)' : ''}` };
    case 'off': return { tone: 'idle', title: 'Off', detail: status.installed ? 'Installed. Turn it on below.' : 'Install it, then turn it on.' };
    default: return { tone: 'ok', title: status.state === 'running' ? 'Running' : 'Ready', detail: [status.quant, gpu?.name].filter(Boolean).join(' · ') };
  }
}

/** Can the Install button be offered: something is missing, nothing is installing, and the GPU can hold it. */
export function canInstall(status) {
  if (!status) return false;
  if (status.state === 'no_gpu' || status.state === 'no_vram' || status.state === 'installing') return false;
  return Boolean(status.needs_install);
}