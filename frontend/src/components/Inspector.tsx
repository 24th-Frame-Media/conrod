import { asset } from '../lib/api';
import { filename, frameStars, parseAttributes } from '../lib/review';
import { ATTRIBUTE_KEYS, type AttributeKey, type Detection, type Frame, type KnownVehicle, type MarkValues } from '../lib/types';
import { StarControl } from './Stars';
import { LoadableImage } from './Media';

const LABELS: Record<AttributeKey, string> = { plate: 'Plate', race_number: 'No.', make: 'Make', model: 'Model', colour: 'Colour', team: 'Team', driver: 'Driver', country: 'Country', plate_state: 'State', body_type: 'Body' };

/** One detected subject with its readable fields editable in place; a change is saved when the field loses focus. */
export function DetectionEditor({ detection: d, save }: { detection: Detection; save: (id: number, updates: Record<string, string | string[] | null>) => unknown }) {
  const attrs = parseAttributes(d);
  const person = d.region_type === 'person' || d.region_type === 'face' || d.cls === 'person' || d.cls === 'face';
  const keys = person ? (['person_name', 'country'] as const) : ATTRIBUTE_KEYS;
  return (
    <section className="subject">
      <div className="subject-head">
        <h4>{d.cls}</h4>
        <span className="tag" title="Sharpness measured on the subject">focus {d.sharpness.toFixed(3)}</span>
        {d.panning ? <span className="tag manual" title="Subject sharp, background panned">panned</span> : null}
        {d.burst_pick ? <span className="tag vlm">keeper</span> : null}
      </div>
      {d.cull_reason && <p className="cull-reason">{d.cull_reason}</p>}
      {person && d.known_person_match && !attrs.person_name && <div className="seen-before person-match"><span><b>Seen before?</b> {d.known_person_match.name}{d.known_person_match.country ? ` · ${d.known_person_match.country}` : ''} · {Math.round((d.known_person_match.similarity ?? 0) * 100)}% visual match</span><button className="small" onClick={() => save(d.id, { person_name: d.known_person_match!.name, country: d.known_person_match!.country ?? null })}>Use this name</button></div>}
      {!person && <><div className="chips">{(attrs.sponsors ?? []).map(s => <span className="tag" key={s}>{s}</span>)}</div>
      <label className="read"><span className="lbl">Sponsors / livery</span><input key={`${d.id}-${(attrs.sponsors ?? []).join(',')}`} defaultValue={(attrs.sponsors ?? []).join(', ')} placeholder="Comma-separated names" onBlur={e => { const sponsors = e.target.value.split(',').map(s => s.trim()).filter(Boolean); if(JSON.stringify(sponsors) !== JSON.stringify(attrs.sponsors ?? [])) save(d.id, {sponsors}); }} /></label></>}
      <div className="readout">
        {keys.map((key) => {
          const value = attrs[key] ?? '';
          return (
            <label className="read" key={`${d.id}-${key}-${value}`}>
              <span className="lbl">{key === 'person_name' ? 'Name' : LABELS[key as AttributeKey]}</span>
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
  allDetections?: Detection[]; allFrames?: Frame[]; known?: KnownVehicle[];
  onMark: (values: MarkValues) => void; onOpen: () => void; onEdit: (id: number, updates: Record<string, string | string[] | null>) => unknown;
};

/** The selected photograph: thumbnail, stars, reject, measurements and its subjects. */
const EVIDENCE_FIELDS = ['plate', 'race_number', 'make', 'model', 'team', 'driver', 'country'] as const;

function VehicleEvidence({ detection, peers, known, apply }: { detection: Detection; peers: Detection[]; known: KnownVehicle[]; apply: (updates: Record<string, string | null>) => unknown }) {
  const readings = peers.map(parseAttributes);
  const attrs = parseAttributes(detection);
  const suggestions = EVIDENCE_FIELDS.map((key) => {
    const counts = new Map<string, number>();
    readings.map((a) => a[key]).filter(Boolean).forEach((value) => counts.set(value!, (counts.get(value!) ?? 0) + 1));
    return { key, values: [...counts.entries()].sort((a, b) => b[1] - a[1]) };
  }).filter((item) => item.values.length);
  const seen = detection.known_match ?? known.find((car) =>
    (attrs.plate && car.plate.toUpperCase() === attrs.plate.toUpperCase()) ||
    (attrs.race_number && car.race_number === attrs.race_number && (!attrs.make || !car.make || car.make.toLowerCase() === attrs.make.toLowerCase())) ||
    (attrs.make && attrs.model && car.make?.toLowerCase() === attrs.make.toLowerCase() && car.model?.toLowerCase() === attrs.model.toLowerCase() && (!attrs.colour || !car.colour || car.colour.toLowerCase() === attrs.colour.toLowerCase()))
  );
  const similarity = detection.known_match?.similarity;
  return (
    <section className="evidence">
      <div className="subject-head"><h4>Group evidence</h4><span className="tag">{peers.length} sightings</span>{detection.group_agreement != null && <span className="tag">{Math.round(detection.group_agreement * 100)}% agreement</span>}</div>
      {seen && <div className="seen-before"><span><b>Seen before</b>{similarity ? ` · ${Math.round(similarity * 100)}% visual match` : ''} · {seen.plate} {[seen.make, seen.model].filter(Boolean).join(' ')}</span><button className="small" onClick={() => apply({ plate: seen.plate, make: seen.make ?? null, model: seen.model ?? null, colour: seen.colour ?? null, team: seen.team ?? null, race_number: seen.race_number ?? null, driver: seen.driver ?? null, country: seen.country ?? null })}>Fill known details</button></div>}
      {suggestions.map(({ key, values }) => <div className="evidence-row" key={key}><span className="lbl">{LABELS[key]}</span><div className="chips">{values.map(([value, count], index) => <button className={`evidence-choice${index === 0 ? ' likely' : ''}`} key={value} title={`Seen in ${count} image${count === 1 ? '' : 's'}`} onClick={() => apply({ [key]: value })}>{value} <small>{count}</small></button>)}</div></div>)}
    </section>
  );
}

export function Inspector({ frame, detections, scanning, allDetections = [], allFrames = [], known = [], onMark, onOpen, onEdit }: Props) {
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
        <LoadableImage src={asset(frame.thumb_path)} alt={filename(frame.path)} />
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
      {detections.filter((d) => (d.region_type ?? 'vehicle') === 'vehicle').map((d) => {
        const burst = frame.burst_key;
        const burstImages = new Set(allFrames.filter((f) => burst != null && f.burst_key === burst).map((f) => f.id));
        const identity = parseAttributes(d);
        const sameIdentity = (other: Detection) => {
          const candidate = parseAttributes(other);
          return Boolean(
            (identity.plate && candidate.plate === identity.plate) ||
            (identity.race_number && candidate.race_number === identity.race_number) ||
            (identity.make && identity.model && candidate.make === identity.make && candidate.model === identity.model)
          );
        };
        const peers = allDetections.filter((other) => other.id === d.id || (d.group_key != null ? other.group_key === d.group_key : burstImages.has(other.image_id) && sameIdentity(other)));
        return <VehicleEvidence key={`evidence-${d.id}`} detection={d} peers={peers} known={known} apply={(updates) => onEdit(d.id, updates)} />;
      })}
      {detections.map((d) => <DetectionEditor key={d.id} detection={d} save={onEdit} />)}
      {!detections.length && frame.status === 'done' && <p className="muted small-print">No subjects were found in this frame.</p>}
    </aside>
  );
}
