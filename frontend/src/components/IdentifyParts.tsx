import { asset } from '../lib/api';
import {
  actionLabel, confidencePercent, confidenceTone, displayPath, fileName, prettyReply, sourceLabel,
} from '../lib/identifyFlow.mjs';
import { visionLaneWords, type VisionAction, type VisionOffReason } from '../lib/vision.mjs';
import { SUBJECT_WORDS } from '../lib/subjects.mjs';
import type { IdentifyItem } from '../lib/types';

/** The album's resolved subject words (subjects.mjs), or the neutral default. */
type Subject = { one: string; many: string; title: string; rate: string; each: string };

/** Why a subject got no make, colour or team when vision is off, with the way to change that. */
function VisionWhy({ reason, onAction }: { reason: VisionOffReason; onAction?: (id: VisionAction['id']) => void }) {
  const { why, action } = visionLaneWords(reason);
  return (
    <>
      {why}
      {action && onAction && (
        <button type="button" className="ghost small id-vision-action" onClick={() => onAction(action.id)}>{action.label}</button>
      )}
    </>
  );
}

/** The photo the subject was read from, by file name; a crop's own file name says nothing. */
const caption = (item: IdentifyItem) => item.path?.split(/[\\/]/).pop() || fileName(item);

export const hash = (number: string | null | undefined) => (number ? (number.startsWith('#') ? number : `#${number}`) : '—');

/** A field chip under the model's reply, only when the engine read something. */
export function Chips({ item }: { item: IdentifyItem }) {
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
 * One vision worker's page: its subject, what the model was asked, what it said, and the number that came
 * out. Several workers run at once, so the screen shows one of these per worker; a finished
 * worker keeps its last subject until the next one starts.
 */
export function IdentifyLane({ worker, item, prompt, live, name = 'WORKER', ocrOnly = false, visionOff, onVisionAction, onOpen, subject }: {
  worker: number; item: IdentifyItem | null; prompt?: string; live: boolean; name?: string; ocrOnly?: boolean; onOpen: () => void;
  subject?: Subject;
  /** Why the vision model is not asked, when it is not: the card says so instead of "Make not read". */
  visionOff?: VisionOffReason | null;
  onVisionAction?: (id: VisionAction['id']) => void;
}) {
  const w = subject ?? SUBJECT_WORDS.subjects;
  const image = item ? asset(displayPath(item)) : '';
  const title = [item?.make, item?.model].filter(Boolean).join(' ')
    || (item ? (ocrOnly ? (live ? 'Reading plates & numbers' : 'OCR complete') : visionOff ? visionLaneWords(visionOff).title : 'Make not read') : `Waiting for a ${w.one}`);
  const source = sourceLabel(item?.number_source);
  // Confidence is about the number: a subject with no number read is not a 0% reading.
  const conf = item?.number ? confidencePercent(item.number_conf) : null;
  const tone = confidenceTone(item?.number_conf);
  const label = `${name} ${String(worker + 1).padStart(2, '0')}`;
  return (
    <section className={`id-lane${live ? ' live' : item ? ' done' : ' idle'}`} aria-label={`${name === 'WORKER' ? 'Worker' : name === 'OCR' ? 'OCR worker' : 'Vision worker'} ${worker + 1}`}>
      <header className="id-lane-head">
        <span className="id-lane-name mono">{label}</span>
        <span className={`id-lane-stage ${item?.stage ?? 'idle'}`}>{item ? item.stage : 'idle'}</span>
        {live && <i className="id-lane-dot" aria-hidden="true" />}
      </header>

      <div className="id-view id-view-lane">
        {image && <span className="id-view-fill" style={{ backgroundImage: `url("${image}")` }} aria-hidden="true" />}
        {image
          ? <img src={image} alt="" draggable={false} loading="eager" />
          : <span className="id-view-empty">{live ? 'Opening the RAW…' : `This worker has no ${w.one} yet.`}</span>}
        <span className="id-corner tl" aria-hidden="true" /><span className="id-corner tr" aria-hidden="true" />
        <span className="id-corner bl" aria-hidden="true" /><span className="id-corner br" aria-hidden="true" />
        {live && <span className="id-scanline" aria-hidden="true" />}
        <span className={`id-badge${live ? ' live' : ''}`}>{live ? 'LIVE' : item ? 'DONE' : 'IDLE'}</span>
        {item && <span className="id-file" title={item.path || displayPath(item)}>{caption(item)}</span>}
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
          : <p className="muted id-panel-empty">{item ? (ocrOnly ? 'Reading plate and number (vision runs next).' : visionOff ? <VisionWhy reason={visionOff} onAction={onVisionAction} /> : `No prompt for this ${w.one}.`) : 'Waiting…'}</p>}
      </section>
      <section className="id-panel id-lane-panel">
        <h4>MODEL SAYS</h4>
        {item?.reply
          ? <pre className="mono">{prettyReply(item.reply)}</pre>
          : <p className="muted id-panel-empty">{ocrOnly ? 'Vision model waits until OCR finishes.' : visionOff ? 'Vision is off, so there is no reply.' : live ? 'Waiting for the model…' : 'No reply.'}</p>}
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

/** The full page for one subject: the picture with its number, the question, the reply and the trace. */
export function IdentifyDetail({ detail, prompt, live, visionOff, onVisionAction, subject }: {
  detail: IdentifyItem | null; prompt?: string; live: boolean; subject?: Subject;
  visionOff?: VisionOffReason | null; onVisionAction?: (id: VisionAction['id']) => void;
}) {
  const w = subject ?? SUBJECT_WORDS.subjects;
  const number = hash(detail?.number);
  const title = [detail?.make, detail?.model].filter(Boolean).join(' ') || (detail ? (visionOff ? visionLaneWords(visionOff).title : 'Make not read') : `No ${w.one} read yet`);
  const source = sourceLabel(detail?.number_source);
  const conf = detail?.number ? confidencePercent(detail.number_conf) : null;
  const tone = confidenceTone(detail?.number_conf);
  const image = detail ? asset(displayPath(detail)) : '';
  return (
    <div className="id-main">
      <section className="id-left" aria-label={`${w.title} being read`}>
        <div className="id-view">
          {image && <span className="id-view-fill" style={{ backgroundImage: `url("${image}")` }} aria-hidden="true" />}
          {image
            ? <img src={image} alt="" draggable={false} loading="eager" />
            : <span className="id-view-empty">No picture for this {w.one} yet.</span>}
          <span className="id-corner tl" aria-hidden="true" /><span className="id-corner tr" aria-hidden="true" />
          <span className="id-corner bl" aria-hidden="true" /><span className="id-corner br" aria-hidden="true" />
          {live && <span className="id-scanline" aria-hidden="true" />}
          <span className={`id-badge${live ? ' live' : ''}`}>{live ? 'LIVE' : 'DONE'}</span>
          {detail && <span className="id-file" title={detail.path || displayPath(detail)}>{caption(detail)}</span>}
        </div>

        <div className="id-result" key={detail?.detection_id ?? 'empty'}>
          <div className="id-number">
            <span>{number}</span>
            {source && <span className={`id-src${source === 'VLM' ? ' vlm' : ''}`}>{source}</span>}
          </div>
          <div className="id-facts">
            <b>{title}</b>
            {detail?.team && <span>{detail.team}</span>}
            <span className="muted">{[detail?.colour, detail?.plate].filter(Boolean).join(' · ') || 'No colour or plate read'}</span>
            {detail?.failures?.length ? <span className="id-failures">{detail.failures.join(' · ')}</span> : null}
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
            : <p className="muted id-panel-empty">{detail ? (visionOff ? <VisionWhy reason={visionOff} onAction={onVisionAction} /> : `No prompt for this ${w.one}.`) : 'Waiting…'}</p>}
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
  );
}
