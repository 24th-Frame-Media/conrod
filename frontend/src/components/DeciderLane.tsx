import { asset } from '../lib/api';
import { barTone, deciderWord, msWords, percent } from '../lib/decider.mjs';
import type { DeciderLive, DeciderSaid } from '../lib/types';
import '../styles/ingest.css';

/** The file name of a frame path, for the lane's caption. */
const fileOf = (label: string) => label.split(/[\\/]/).pop() || label;

/** One answered question: what was asked, the model's pick, and the top options as small bars. */
function Answer({ said }: { said: DeciderSaid }) {
  const tone = barTone(said.p);
  return (
    <li className="dec-answer">
      <span className="dec-q" title={said.text}>{said.text}</span>
      <span className={`dec-pick mono ${tone}`}>{said.label} <b>{percent(said.p)}</b></span>
      <span className="dec-bars" aria-hidden="true">
        {said.bars.map((bar) => (
          <span className="dec-bar" key={bar.label} title={`${bar.label} ${percent(bar.p)}`}>
            <i className={bar.label === said.label ? `top ${tone}` : ''} style={{ width: `${Math.round(bar.p * 100)}%` }} />
          </span>
        ))}
      </span>
    </li>
  );
}
/**
 * The decision model's HUD card in the workers grid: the frame it is looking at, its counters,
 * and a unified MOMENTS panel showing the structured verdicts + inline probability bars (plus any
 * pending checks in flight). Supports `hero` (solo side-by-side), `card` (vertical), and `tile`
 * (horizontal-split) layouts.
 */
export function DeciderLane({ live, serving = null, layout = 'card', paused = false }: {
  live: DeciderLive; serving?: string | null; layout?: 'hero' | 'card' | 'tile'; paused?: boolean;
}) {
  const current = live.current;
  const last = live.recent[0] ?? null;
  const busy = live.busy > 0 || live.state === 'loading' || (current != null && !current.queued);
  // A cloud decision service (e.g. JEV via OpenCode) serves with a key: the local model's
  // "not installed" state is irrelevant, so the lane never reads as off or names the local model.
  const unavailable = !serving && ['off', 'not_installed', 'installing', 'no_gpu', 'no_vram'].includes(live.state) && !busy && !last;
  const shown = current ?? last;
  const image = shown?.thumb ? asset(shown.thumb) : undefined;
  const asking = current?.asking ?? [];
  const says = last?.says ?? [];
  const answeredIds = new Set(says.map((s) => s.id));
  const pendingAsks = asking.filter((a) => !answeredIds.has(a.id));
  const isPaused = paused && !unavailable;
  const cls = isPaused ? ' paused' : busy ? ' live' : last ? ' done' : ' idle';

  const viewfinder = (
    <div className="id-view id-view-lane">
      {image && <span className="id-view-fill" style={{ backgroundImage: `url("${image}")` }} aria-hidden="true" />}
      {image
        ? <img src={image} alt="" draggable={false} loading="eager" />
        : <span className="id-view-empty">{unavailable ? live.message : live.state === 'loading' ? 'Loading the model onto the GPU…' : 'Waiting for a frame…'}</span>}
      <span className="id-corner tl" aria-hidden="true" /><span className="id-corner tr" aria-hidden="true" />
      <span className="id-corner bl" aria-hidden="true" /><span className="id-corner br" aria-hidden="true" />
      {busy && !isPaused && <span className="id-scanline" aria-hidden="true" />}
    </div>
  );
  // The file name lives under the picture, not on it.
  const file = shown ? <span className="ing-file mono muted" title={shown.label}>{fileOf(shown.label)}</span> : null;

  const counters = (
    <dl className={`dec-counters${layout === 'tile' ? ' compact' : ''}`}>
      <div><dt>Frames</dt><dd className="mono">{live.done.toLocaleString('en-US')}</dd></div>
      <div><dt>Failed</dt><dd className={live.errors ? 'mono bad' : 'mono'}>{live.errors.toLocaleString('en-US')}</dd></div>
      <div><dt>Per frame</dt><dd className="mono">{live.avg_ms ? msWords(live.avg_ms) : '-'}</dd></div>
      <div><dt>GPU slots</dt><dd className="mono">{live.busy}/{live.capacity}{live.queued > 0 && <small> +{live.queued}</small>}</dd></div>
    </dl>
  );

  const momentsPanel = (
    <section className="id-panel id-lane-panel dec-moments-panel">
      <div className="dec-moments-head">
        <h4>Moments</h4>
        {asking.length > 0 && <span className="mono muted dec-check-count">{asking.length} {asking.length === 1 ? 'check' : 'checks'}</span>}
      </div>
      {says.length > 0 || pendingAsks.length > 0 ? (
        <ul className={`dec-answers${busy && says.length > 0 && pendingAsks.length === 0 ? ' stale' : ''}`}>
          {says.map((said) => <Answer said={said} key={said.id} />)}
          {pendingAsks.map((ask) => (
            <li className="dec-answer pending" key={`ask:${ask.id}`}>
              <span className="dec-q" title={ask.text}>{ask.text}</span>
              <span className="dec-pick mono id-field-pending">deciding…</span>
              <span className="dec-bars" aria-hidden="true">
                <span className="dec-bar"><i style={{ width: '35%' }} /></span>
              </span>
            </li>
          ))}
        </ul>
      ) : unavailable ? (
        <p className="muted id-panel-empty">{live.message}</p>
      ) : (
        <p className="muted id-panel-empty">{busy ? 'Deciding…' : 'Waiting for the first frame…'}</p>
      )}
      {last && !last.ok && <span className="id-failures">The last frame failed; the scan went on without it.</span>}
    </section>
  );

  const foot = (
    <p className="muted dec-foot mono">
      {serving || [live.model, live.quant, live.device].filter(Boolean).join(' · ') || 'pplx-decider-flash'}
      {last?.ms ? ` · ${msWords(last.ms)} · ${last.tokens} tokens` : ''}
    </p>
  );

  return (
    <section className={`id-lane decider-lane ${layout}${cls}${unavailable ? ' off' : ''}`} aria-label="Decision model">
      <header className="id-lane-head">
        <span className="id-lane-name mono">Decider</span>
        <span className={`id-lane-stage ${isPaused ? 'ing-stage paused' : busy ? 'ing-stage busy' : 'ing-stage idle'}`}>{isPaused ? 'paused' : busy ? (live.state === 'loading' ? 'loading' : 'deciding') : serving ? 'ready' : deciderWord(live.state)}</span>
        {busy && !isPaused && <i className="id-lane-dot" aria-hidden="true" />}
      </header>

      {/* Very short screens (below 680 px) fold the card into one row. */}
      <div className="decider-row">
        <b>{serving ? 'ready' : deciderWord(live.state)}</b>
        <span className="mono">{live.avg_ms ? `${msWords(live.avg_ms)} / frame` : 'no frame yet'}</span>
        <span className="muted">{last ? (last.says[0] ? `${last.says[0].text}: ${last.says[0].label} ${percent(last.says[0].p)}` : `${live.done.toLocaleString('en-US')} frames judged`) : live.message}</span>
      </div>

      {layout === 'hero' ? (
        <div className="dec-hero-grid">
          <div className="dec-hero-left">
            {viewfinder}{file}
            {counters}
          </div>
          <div className="dec-hero-right">
            {momentsPanel}
            {foot}
          </div>
        </div>
      ) : layout === 'tile' ? (
        <div className="id-lane-split">
          <div className="id-lane-split-left">
            {viewfinder}{file}
            {counters}
          </div>
          <div className="id-lane-split-right">
            {momentsPanel}
          </div>
        </div>
      ) : (
        <>
          {viewfinder}{file}
          {counters}
          {momentsPanel}
          {foot}
        </>
      )}
    </section>
  );
}