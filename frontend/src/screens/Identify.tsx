import { useEffect, useRef, useState } from 'react';
import { Empty, Icon } from '../components/basics';
import { asset } from '../lib/api';
import {
  actionLabel, confidencePercent, confidenceTone, displayItem, displayPath, elapsed, fileName, flowNodes,
  prettyReply, progressPercent, rateLabel, sourceLabel, workerLanes,
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

const hash = (number: string | null | undefined) => (number ? (number.startsWith('#') ? number : `#${number}`) : '—');

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
 * One worker's page: its car, what the model was asked, what it said, and the number that came
 * out. Several workers run at once, so the screen stacks one of these per worker; a finished
 * worker keeps its last car until the next one starts.
 */
function WorkerLane({ worker, item, prompt, live, onOpen }: {
  worker: number; item: IdentifyItem | null; prompt?: string; live: boolean; onOpen: () => void;
}) {
  const image = item ? asset(displayPath(item)) : '';
  const title = [item?.make, item?.model].filter(Boolean).join(' ') || (item ? 'Make not read' : 'Waiting for a car');
  const source = sourceLabel(item?.number_source);
  // Confidence is about the number: a car with no number read is not a 0% reading.
  const conf = item?.number ? confidencePercent(item.number_conf) : null;
  const tone = confidenceTone(item?.number_conf);
  return (
    <section className={`id-lane${live ? ' live' : item ? ' done' : ' idle'}`} aria-label={`Worker ${worker + 1}`}>
      <header className="id-lane-head">
        <span className="id-lane-name mono">WORKER {String(worker + 1).padStart(2, '0')}</span>
        <span className={`id-lane-stage ${item?.stage ?? 'idle'}`}>{item ? item.stage : 'idle'}</span>
        {live && <i className="id-lane-dot" aria-hidden="true" />}
      </header>

      <div className="id-view id-view-lane">
        {image && <span className="id-view-fill" style={{ backgroundImage: `url("${image}")` }} aria-hidden="true" />}
        {image
          ? <img src={image} alt="" draggable={false} loading="eager" />
          : <span className="id-view-empty">{live ? 'Opening the RAW…' : 'This worker has no car yet.'}</span>}
        <span className="id-corner tl" aria-hidden="true" /><span className="id-corner tr" aria-hidden="true" />
        <span className="id-corner bl" aria-hidden="true" /><span className="id-corner br" aria-hidden="true" />
        {live && <span className="id-scanline" aria-hidden="true" />}
        <span className={`id-badge${live ? ' live' : ''}`}>{live ? 'LIVE' : item ? 'DONE' : 'IDLE'}</span>
        {item && <span className="id-file" title={displayPath(item)}>{fileName(item)}</span>}
      </div>

      <div className="id-lane-result" key={item?.detection_id ?? 'empty'}>
        <span className="id-lane-number mono">{hash(item?.number)}</span>
        <div className="id-lane-facts">
          <b>{title}</b>
          <span className="muted">{[item?.colour, item?.plate].filter(Boolean).join(' · ') || source || '—'}</span>
        </div>
        {conf != null
          ? <span className={`id-lane-conf mono ${tone}`}>{conf}%</span>
          : <span className="id-lane-conf muted">—</span>}
      </div>

      <section className="id-panel id-lane-panel">
        <h4>ASKING</h4>
        {prompt
          ? <pre className="mono">{prompt}</pre>
          : <p className="muted id-panel-empty">{item ? 'No prompt for this car.' : 'Waiting…'}</p>}
      </section>
      <section className="id-panel id-lane-panel">
        <h4>MODEL SAYS</h4>
        {item?.reply
          ? <pre className="mono">{prettyReply(item.reply)}</pre>
          : <p className="muted id-panel-empty">{live ? 'Waiting for the model…' : 'No reply.'}</p>}
        {item?.action?.length ? (
          <div className="id-action-tags">{item.action.map((tag) => <span className="tag" key={tag}>{actionLabel(tag)}</span>)}</div>
        ) : null}
        {item?.failures?.length ? <span className="id-failures">{item.failures.join(' · ')}</span> : null}
      </section>

      {item?.trace?.length ? (
        <details className="id-lane-trace">
          <summary>Trace <span className="mono">{item.trace.length}</span></summary>
          <ol className="id-trace-lines">
            {item.trace.map((line, i) => (
              <li className="id-trace-line" key={`${i}:${line}`}>
                <span className="id-trace-n">{String(i + 1).padStart(2, '0')}</span>
                <span>{line}</span>
              </li>
            ))}
          </ol>
        </details>
      ) : null}

      <button type="button" className="ghost small id-lane-open" disabled={!item} onClick={onOpen}>Open detail</button>
    </section>
  );
}

/**
 * An album while its identification runs (and after): the flow, one page per worker with its own
 * car, question and reply, a detail view for any finished car, and every car read so far.
 */
export function IdentifyScreen({ job, live, status, model, paused, onCancel, onReview }: Props) {
  const [picked, setPicked] = useState<number | null>(null);
  // Prompts only ride on the cars being read and on the snapshot's active list, not on the
  // finished strip; keep the last one per car so a finished car can still show what was asked.
  const prompts = useRef<Record<number, string>>({});
  useEffect(() => {
    for (const car of live?.active ?? []) {
      if (car.prompt) prompts.current[car.detection_id] = car.prompt;
    }
    const current = live?.current;
    if (current?.prompt) prompts.current[current.detection_id] = current.prompt;
  }, [live]);
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
  const nodes = flowNodes(live, live?.model || model);
  const lanes = workerLanes(live);
  const rate = live ? rateLabel(live.done, live.elapsed_ms) : '';
  const finished = state !== 'running' && !starting;

  // A recent card opens the full page for that car; otherwise the screen is the worker grid.
  const detail = picked != null ? displayItem(live, picked) : null;
  const showDetail = picked != null && detail != null;
  const detailPrompt = detail ? detail.prompt ?? prompts.current[detail.detection_id] : undefined;
  const detailLive = Boolean(detail && detail.stage !== 'done' && running);
  const detailNumber = hash(detail?.number);
  const detailTitle = [detail?.make, detail?.model].filter(Boolean).join(' ') || (detail ? 'Make not read' : 'No car read yet');
  const detailSource = sourceLabel(detail?.number_source);
  const detailConf = detail?.number ? confidencePercent(detail.number_conf) : null;
  const detailTone = confidenceTone(detail?.number_conf);
  const detailImage = detail ? asset(displayPath(detail)) : '';

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
            <div><dt>Workers</dt><dd className="mono">{lanes.length || '–'}</dd></div>
            <div><dt>Elapsed</dt><dd className="mono">{live ? elapsed(live.elapsed_ms) || '–' : '–'}</dd></div>
            <div><dt>Rate</dt><dd className="mono">{rate || '–'}</dd></div>
          </dl>
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

        {showDetail ? (
          <div className="id-main">
            <section className="id-left" aria-label="Car being read">
              <div className="id-view">
                {detailImage && <span className="id-view-fill" style={{ backgroundImage: `url("${detailImage}")` }} aria-hidden="true" />}
                {detailImage
                  ? <img src={detailImage} alt="" draggable={false} loading="eager" />
                  : <span className="id-view-empty">No picture for this car yet.</span>}
                <span className="id-corner tl" aria-hidden="true" /><span className="id-corner tr" aria-hidden="true" />
                <span className="id-corner bl" aria-hidden="true" /><span className="id-corner br" aria-hidden="true" />
                {detailLive && <span className="id-scanline" aria-hidden="true" />}
                <span className={`id-badge${detailLive ? ' live' : ''}`}>{detailLive ? 'LIVE' : 'DONE'}</span>
                {detail && <span className="id-file" title={displayPath(detail)}>{fileName(detail)}</span>}
              </div>

              <div className="id-result" key={detail?.detection_id ?? 'empty'}>
                <div className="id-number">
                  <span>{detailNumber}</span>
                  {detailSource && <span className={`id-src${detailSource === 'VLM' ? ' vlm' : ''}`}>{detailSource}</span>}
                </div>
                <div className="id-facts">
                  <b>{detailTitle}</b>
                  {detail?.team && <span>{detail.team}</span>}
                  <span className="muted">{[detail?.colour, detail?.plate].filter(Boolean).join(' · ') || 'No colour or plate read'}</span>
                  {detail?.failures?.length ? <span className="id-failures">{detail.failures.join(' · ')}</span> : null}
                </div>
                <div className="id-conf">
                  <span className="id-conf-label">Confidence</span>
                  {detailConf != null ? (
                    <>
                      <span className="id-conf-bar"><i className={detailTone} style={{ width: `${detailConf}%` }} /></span>
                      <span className={`id-conf-pct mono ${detailTone}`}>{detailConf}%</span>
                    </>
                  ) : <span className="muted">—</span>}
                </div>
              </div>
            </section>

            <aside className="id-right" aria-label="What the model was asked and said">
              <section className="id-panel id-ask">
                <h4>ASKING</h4>
                {detailPrompt
                  ? <pre className="mono">{detailPrompt}</pre>
                  : <p className="muted id-panel-empty">{detail ? 'No prompt for this car.' : 'Waiting…'}</p>}
              </section>
              <section className="id-panel id-reply">
                <h4>MODEL SAYS</h4>
                {detail?.reply
                  ? <pre className="mono">{prettyReply(detail.reply)}</pre>
                  : <p className="muted id-panel-empty">{detail ? 'OCR/entry list only.' : 'Waiting…'}</p>}
                {detail?.action?.length ? (
                  <div className="id-action-tags">{detail.action.map((tag) => <span className="tag" key={tag}>{actionLabel(tag)}</span>)}</div>
                ) : null}
                {detail && <Chips item={detail} />}
              </section>
              <section className="id-panel id-trace">
                <h4>TRACE</h4>
                {detail?.trace?.length ? (
                  <ol className="id-trace-lines">
                    {detail.trace.map((line, i) => (
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
        ) : lanes.length ? (
          <div className="id-lanes">
            {lanes.map((lane) => (
              <WorkerLane
                key={lane.worker}
                worker={lane.worker}
                item={lane.item}
                live={lane.live}
                prompt={lane.item ? lane.item.prompt ?? prompts.current[lane.item.detection_id] : undefined}
                onOpen={() => lane.item && setPicked(lane.item.detection_id)}
              />
            ))}
          </div>
        ) : (
          <p className="muted id-lanes-empty">Waiting for the workers to pick up the first cars…</p>
        )}

        <section className="id-recent" aria-label="Cars already read">
          <div className="id-recent-head">
            <span className="section-label">Recent cars<span>{live?.recent.length ? 'newest first' : ''}</span></span>
            <button type="button" className={`id-live-return${picked == null ? ' on' : ''}`} disabled={picked == null}
              title="Back to the workers" onClick={() => setPicked(null)}>
              <i aria-hidden="true" />WORKERS
            </button>
          </div>
          {live?.recent.length ? (
            <div className="id-recent-row">
              {live.recent.map((car) => {
                const carConf = car.number ? confidencePercent(car.number_conf) : null;
                const carTone = confidenceTone(car.number_conf);
                const thumb = asset(car.thumb_path ?? car.preview_path ?? car.path);
                const selected = picked === car.detection_id;
                return (
                  <button type="button" className={`id-card${selected ? ' sel' : ''}`} key={car.detection_id}
                    title={[car.make, car.model, car.team].filter(Boolean).join(' · ') || 'Car read'}
                    onClick={() => setPicked(car.detection_id)}>
                    {thumb ? <img src={thumb} alt="" loading="lazy" draggable={false} /> : <span className="id-card-noimg" aria-hidden="true" />}
                    <span className="id-card-num">{hash(car.number)}</span>
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
