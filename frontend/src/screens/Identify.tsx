import { useEffect, useRef, useState } from 'react';
import { Empty, Icon } from '../components/basics';
import { asset } from '../lib/api';
import {
  confidencePercent, confidenceTone, displayItem, displayPath, elapsed, fileName, flowNodes,
  justFinished, prettyReply, progressPercent, rateLabel, sourceLabel,
} from '../lib/identifyFlow.mjs';
import type { IdentifyItem, IdentifyLive, Job, Status } from '../lib/types';

type Props = {
  job: Job | undefined; live: IdentifyLive | undefined; status: Status;
  /** The vision model this album would call, for the VISION node's subline. */
  model?: string;
  paused: boolean;
  onCancel: (key: string) => void;
  onReview: () => void;
};

/** A field chip under the model's reply, only when the engine read something. */
function Chips({ item }: { item: IdentifyItem }) {
  const fields: [string, string | null | undefined][] = [
    ['no', item.number], ['make', item.make], ['model', item.model],
    ['team', item.team], ['colour', item.colour], ['plate', item.plate],
  ];
  const shown = fields.filter(([, value]) => Boolean(value && String(value).trim()));
  if (!shown.length) return null;
  return (
    <div className="id-chips">
      {shown.map(([key, value]) => <span className="id-chip" key={key}><b>{key}</b> {value}</span>)}
    </div>
  );
}

/**
 * An album while its identification runs (and after): the flow, the car being read, the exact
 * question and reply, the trace and every car read so far. The cull screen's live counterpart.
 */
export function IdentifyScreen({ job, live, status, model, paused, onCancel, onReview }: Props) {
  const [picked, setPicked] = useState<number | null>(null);
  // The prompt only rides on the car being read, and the prompt is most of an item's bytes.
  // Keep the last one per car here, so a result still on screen after the next car takes over
  // can show what was asked.
  const prompts = useRef<Record<number, string>>({});
  const currentId = live?.current?.detection_id ?? null;
  const currentPrompt = live?.current?.prompt ?? null;
  useEffect(() => {
    if (currentId != null && currentPrompt) prompts.current[currentId] = currentPrompt;
  }, [currentId, currentPrompt]);
  const operation = job
    ? status.operations.find((key) => key.startsWith('Identifying:') && key.endsWith(`:${job.id}`))
    : undefined;
  if (!job || (!live && !operation)) {
    return (
      <div className="screen">
        <Empty icon="eye" title="No identification is running">
          <p>Start one from an album with <b>Identify</b>, or watch a finished one from Review.</p>
          <button className="primary" onClick={onReview}>Back to review</button>
        </Empty>
      </div>
    );
  }

  const state = live?.state ?? 'running';
  const running = state === 'running';
  const starting = !live;
  const done = live?.done ?? 0;
  const total = live?.total ?? 0;
  const pct = progressPercent(done, total);
  const stateWord = starting ? 'Starting' : state === 'running' ? (paused ? 'Paused' : 'Identifying') : state === 'done' ? 'Complete' : state === 'stopped' ? 'Stopped' : 'Failed';
  // Hold the newest finished car for a beat. Three workers run at once, so the next car is
  // usually already mid-read, and its number and fields deserve more than one frame. A manual
  // pick, a pruned result or the run's end all fall through to the live car.
  const held = picked == null ? justFinished(live, Date.now()) : null;
  const item = picked != null ? displayItem(live, picked) : held ?? live?.current ?? live?.recent?.[0] ?? null;
  const nodes = flowNodes(live, live?.model || model, item?.stage);
  const prompt = item ? item.prompt ?? prompts.current[item.detection_id] : undefined;
  const active = live?.active ?? [];
  const trace = item?.trace ?? [];
  const number = item?.number ? (item.number.startsWith('#') ? item.number : `#${item.number}`) : '—';
  const title = [item?.make, item?.model].filter(Boolean).join(' ') || (item ? 'Make not read' : 'No car read yet');
  const source = sourceLabel(item?.number_source);
  // Confidence is about the number: a car with no number read is not a 0% reading.
  const conf = item?.number ? confidencePercent(item.number_conf) : null;
  const tone = confidenceTone(item?.number_conf);
  const selected = picked != null && item != null && picked === item.detection_id;
  // The scanline and LIVE badge belong to the car actually being read, not a held result.
  const liveCard = !selected && item != null && item.detection_id === live?.current?.detection_id;
  const image = asset(displayPath(item));
  const rate = live ? rateLabel(live.done, live.elapsed_ms) : '';
  const finished = state !== 'running' && !starting;

  return (
    <div className="screen identify-screen">
      <div className="identify-page">
        <header className={`id-head ${state}`}>
          <div className="id-head-main">
            <span className="id-eyebrow mono">IDENTIFY / LIVE</span>
            <h2>{job.label}</h2>
            <span className={`id-state ${state}`}><i />{stateWord}</span>
          </div>
          <dl className="id-stats">
            <div><dt>Cars</dt><dd className="mono">{live ? `${done.toLocaleString()} / ${total.toLocaleString()}` : '–'}</dd></div>
            <div><dt>Elapsed</dt><dd className="mono">{live ? elapsed(live.elapsed_ms) || '–' : '–'}</dd></div>
            <div><dt>Rate</dt><dd className="mono">{rate || '–'}</dd></div>
          </dl>
          {active.length > 0 && (
            <div className="id-flight" aria-label="Cars being read right now">
              <span className="id-flight-label mono">READING NOW</span>
              <div className="id-flight-list">
                {active.map((car) => (
                  <button type="button" key={car.detection_id}
                    className={`id-flight-card${item?.detection_id === car.detection_id ? ' sel' : ''}`}
                    title={`${car.stage.toUpperCase()} · ${fileName(car)}`}
                    onClick={() => setPicked(car.detection_id)}>
                    {asset(displayPath(car))
                      ? <img src={asset(displayPath(car))} alt="" draggable={false} />
                      : <span className="id-flight-noimg" aria-hidden="true" />}
                    <span className="mono">{car.stage}</span>
                  </button>
                ))}
              </div>
            </div>
          )}
          {running && (
            <div className="id-actions">
              <button className="ghost danger" disabled={!operation} onClick={() => operation && onCancel(operation)}>
                Stop identification
              </button>
            </div>
          )}
        </header>

        <div className="id-progress">
          <div className="bar id-bar" role="progressbar" aria-valuenow={starting ? undefined : pct} aria-valuemin={0} aria-valuemax={100} aria-label="Identification progress">
            {live ? <div className="fill" style={{ width: `${pct}%` }} /> : <div className="fill indeterminate" />}
          </div>
          <span className="mono id-progress-count">{starting ? 'Waiting for the first car…' : `${pct}% · ${done.toLocaleString()} / ${total.toLocaleString()}`}</span>
        </div>

        <ol className={`id-flow${running ? '' : ' still'}`} aria-label="Identification stages">
          {nodes.map((node, i) => (
            <li className="id-flow-cell" key={node.number}>
              {i > 0 && <span className={`id-link${node.state === 'active' ? ' flow' : node.state === 'done' ? ' done' : ''}`} aria-hidden="true" />}
              <div className={`id-node ${node.state}`}>
                <span className="id-node-top">
                  <span className="id-node-num mono">{node.number}</span>
                  {node.state === 'done' && <Icon name="check" size={12} />}
                  {node.state === 'active' && <i className="id-node-dot" aria-hidden="true" />}
                </span>
                <b>{node.label}</b>
                <small title={node.sub}>{node.sub}</small>
              </div>
            </li>
          ))}
        </ol>

        {finished && (
          <section className={`scan-summary id-summary ${state}`} aria-label="Identification summary">
            <div className="ss-mark"><Icon name={state === 'done' ? 'check' : 'x'} size={20} /></div>
            <div className="ss-text">
              <b>{state === 'done'
                ? `${done.toLocaleString()} ${done === 1 ? 'car' : 'cars'} read in ${elapsed(live?.elapsed_ms) || 'no time at all'}`
                : state === 'stopped' ? `Stopped at ${pct}% · ${done.toLocaleString()} cars read` : 'Identification did not finish.'}</b>
              <span className="muted">{state === 'error'
                ? (live?.errors?.[0] ?? 'Check the activity log in the top bar.')
                : 'Identifications are suggestions until you accept them. Nothing is written until you say so.'}</span>
            </div>
            <button className="primary" onClick={onReview}>Back to review</button>
          </section>
        )}

        <div className="id-main">
          <section className="id-left" aria-label="Car being read">
            <div className="id-view">
              {image && <span className="id-view-fill" style={{ backgroundImage: `url("${image}")` }} aria-hidden="true" />}
              {image
                ? <img src={image} alt="" draggable={false} loading="eager" />
                : <span className="id-view-empty">{starting ? 'The first car appears here as it is read.' : 'No picture for this car yet.'}</span>}
              <span className="id-corner tl" aria-hidden="true" /><span className="id-corner tr" aria-hidden="true" />
              <span className="id-corner bl" aria-hidden="true" /><span className="id-corner br" aria-hidden="true" />
              {running && liveCard && <span className="id-scanline" aria-hidden="true" />}
              <span className={`id-badge${running && liveCard ? ' live' : ''}`}>{running && liveCard ? 'LIVE' : 'DONE'}</span>
              {item && <span className="id-file" title={displayPath(item)}>{fileName(item)}</span>}
            </div>

            <div className="id-result" key={item?.detection_id ?? 'empty'}>
              <div className="id-number">
                <span>{number}</span>
                {source && <span className={`id-src${source === 'VLM' ? ' vlm' : ''}`}>{source}</span>}
              </div>
              <div className="id-facts">
                <b>{title}</b>
                {item?.team && <span>{item.team}</span>}
                <span className="muted">{[item?.colour, item?.plate].filter(Boolean).join(' · ') || 'No colour or plate read'}</span>
                {item?.failures?.length ? <span className="id-failures">{item.failures.join(' · ')}</span> : null}
              </div>
              <div className="id-conf">
                <span className="id-conf-label">Confidence</span>
                {conf != null ? (
                  <>
                    <span className="id-conf-bar"><i className={tone} style={{ width: `${conf}%` }} /></span>
                    <span className={`id-conf-pct mono ${tone}`}>{conf}%</span>
                  </>
                ) : <span className="muted">—</span>}
              </div>
            </div>
          </section>

          <aside className="id-right" aria-label="What the model was asked and said">
            <section className="id-panel id-ask">
              <h4>ASKING</h4>
              {prompt
                ? <pre className="mono">{prompt}</pre>
                : <p className="muted id-panel-empty">{starting ? 'Waiting for the first car…' : 'No vision model for this shoot.'}</p>}
            </section>
            <section className="id-panel id-reply">
              <h4>MODEL SAYS</h4>
              {item?.reply
                ? <pre className="mono">{prettyReply(item.reply)}</pre>
                : <p className="muted id-panel-empty">{starting ? 'Waiting for the first reply…' : 'OCR/entry list only.'}</p>}
              {item && <Chips item={item} />}
            </section>
            <section className="id-panel id-trace">
              <h4>TRACE</h4>
              {trace.length ? (
                <ol className="id-trace-lines">
                  {trace.map((line, i) => (
                    <li className="id-trace-line" key={`${i}:${line}`}>
                      <span className="id-trace-n">{String(i + 1).padStart(2, '0')}</span>
                      <span>{line}</span>
                    </li>
                  ))}
                </ol>
              ) : <p className="muted id-panel-empty">No trace yet.</p>}
            </section>
          </aside>
        </div>

        <section className="id-recent" aria-label="Cars already read">
          <div className="id-recent-head">
            <span className="section-label">Recent cars<span>{live?.recent.length ? 'newest first' : ''}</span></span>
            <button type="button" className={`id-live-return${liveCard ? ' on' : ''}`} disabled={liveCard} title="Show the car being read now" onClick={() => setPicked(null)}>
              <i aria-hidden="true" />LIVE
            </button>
          </div>
          {live?.recent.length ? (
            <div className="id-recent-row">
              {live.recent.map((car) => {
                const carConf = car.number ? confidencePercent(car.number_conf) : null;
                const carTone = confidenceTone(car.number_conf);
                const thumb = asset(car.thumb_path ?? car.preview_path ?? car.path);
                const active = item?.detection_id === car.detection_id;
                return (
                  <button type="button" className={`id-card${active ? ' sel' : ''}`} key={car.detection_id}
                    title={[car.make, car.model, car.team].filter(Boolean).join(' · ') || 'Car read'}
                    onClick={() => setPicked(car.detection_id)}>
                    {thumb ? <img src={thumb} alt="" loading="lazy" draggable={false} /> : <span className="id-card-noimg" aria-hidden="true" />}
                    <span className="id-card-num">{car.number ? (car.number.startsWith('#') ? car.number : `#${car.number}`) : '—'}</span>
                    <span className="id-card-make">{[car.make, car.model].filter(Boolean).join(' ') || car.plate || 'Make not read'}</span>
                    <span className={`id-card-conf ${car.number ? carTone : 'none'}`}>{carConf != null ? `${carConf}%` : car.number ? 'no confidence' : 'no number'}</span>
                  </button>
                );
              })}
            </div>
          ) : <p className="muted id-recent-empty">Finished cars appear here, with the number the engine read.</p>}
        </section>
      </div>
    </div>
  );
}
