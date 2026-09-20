import { asset } from '../lib/api';
import { filename, frameStars, parseAttributes } from '../lib/review';
import { ATTRIBUTE_KEYS, type AttributeKey, type Detection, type Frame, type MarkValues } from '../lib/types';
import { StarControl } from './Stars';

const LABELS: Record<AttributeKey, string> = { plate: 'Plate', race_number: 'No.', make: 'Make', model: 'Model', colour: 'Colour', team: 'Team', plate_state: 'State', body_type: 'Body' };

/** One detected subject with its readable fields editable in place; a change is saved when the field loses focus. */
export function DetectionEditor({ detection: d, save }: { detection: Detection; save: (id: number, updates: Record<string, string | string[] | null>) => unknown }) {
  const attrs = parseAttributes(d);
  return (
    <section className="subject">
      <div className="subject-head">
        <h4>{d.cls}</h4>
        <span className="tag" title="Sharpness measured on the subject">focus {d.sharpness.toFixed(3)}</span>
        {d.panning ? <span className="tag manual" title="Subject sharp, background panned">panned</span> : null}
        {d.burst_pick ? <span className="tag vlm">keeper</span> : null}
      </div>
      {d.cull_reason && <p className="cull-reason">{d.cull_reason}</p>}
      <div className="chips">{(attrs.sponsors ?? []).map(s => <span className="tag" key={s}>{s}</span>)}</div>
      <label className="read"><span className="lbl">Sponsors / livery</span><input key={`${d.id}-${(attrs.sponsors ?? []).join(',')}`} defaultValue={(attrs.sponsors ?? []).join(', ')} placeholder="Comma-separated names" onBlur={e => { const sponsors = e.target.value.split(',').map(s => s.trim()).filter(Boolean); if(JSON.stringify(sponsors) !== JSON.stringify(attrs.sponsors ?? [])) save(d.id, {sponsors}); }} /></label>
      <div className="readout">
        {ATTRIBUTE_KEYS.map((key) => {
          const value = attrs[key] ?? '';
          return (
            <label className="read" key={`${d.id}-${key}-${value}`}>
              <span className="lbl">{LABELS[key]}</span>
              <input defaultValue={value} placeholder="—" autoComplete="off" spellCheck={false}
                onKeyDown={(e) => { if (e.key === 'Enter') e.currentTarget.blur(); }}
                onBlur={(e) => { if (e.target.value !== value) save(d.id, { [key]: e.target.value || null }); }} />
            </label>
          );
        })}
      </div>
    </section>
  );
}

type Props = {
  frame: Frame | null; detections: Detection[]; scanning: boolean;
  onMark: (values: MarkValues) => void; onOpen: () => void; onEdit: (id: number, updates: Record<string, string | string[] | null>) => unknown;
};

/** The selected photograph: thumbnail, stars, reject, measurements and its subjects. */
export function Inspector({ frame, detections, scanning, onMark, onOpen, onEdit }: Props) {
  if (!frame) {
    return <aside className="inspector"><p className="muted">{scanning ? 'Results appear here while the scan runs.' : 'Select a photograph.'}</p></aside>;
  }
  const stars = frameStars(frame);
  return (
    <aside className="inspector" aria-label="Selected photograph">
      <div className="frame-side-head">
        <h3 title={frame.path}>{filename(frame.path)}</h3>
        {frame.status !== 'done' && <span className="tag">{frame.status}</span>}
      </div>
      <button className="inspect-image" onClick={onOpen} title="Open the full frame (Enter)">
        <img src={asset(frame.thumb_path)} alt={filename(frame.path)} />
      </button>
      <div className="frame-side-foot">
        <StarControl value={stars} onChange={(n) => onMark({ stars: n })} />
        <button className="ghost small" title="Back to the measured rating (U)" onClick={() => onMark({ stars: null, rejected: false })}>Clear</button>
        <div className="spacer" />
        <button className={`ghost danger cut${frame.rejected ? ' on' : ''}`} onClick={() => onMark({ rejected: !frame.rejected })}>
          {frame.rejected ? 'Put back' : 'Reject'}
        </button>
      </div>
      <dl className="meta">
        <dt>Size</dt><dd>{frame.width ? `${frame.width} × ${frame.height}` : '—'}</dd>
        <dt>Focus</dt><dd>{frame.rating != null ? frame.rating.toFixed(3) : '—'}</dd>
        <dt>Burst</dt><dd>{frame.burst_key ?? '—'}</dd>
      </dl>
      {frame.error && <p className="error-text">{frame.error}</p>}
      {detections.map((d) => <DetectionEditor key={d.id} detection={d} save={onEdit} />)}
      {!detections.length && frame.status === 'done' && <p className="muted small-print">No subjects were found in this frame.</p>}
    </aside>
  );
}
