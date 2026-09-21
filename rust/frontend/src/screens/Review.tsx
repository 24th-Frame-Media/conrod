import { useEffect, useState } from 'react';
import { Empty, Icon } from '../components/basics';
import { Facets } from '../components/Facets';
import { Inspector } from '../components/Inspector';
import { PhotoGrid } from '../components/PhotoGrid';
import { LoadableImage } from '../components/Media';
import { defaultFilters, frameStars, parseAttributes } from '../lib/review';
import { asset, engine } from '../lib/api';
import type { Frame, Job, KnownVehicle, Settings, Status, Task } from '../lib/types';
import type { ReviewModel } from '../state/useReview';

function SuspectedCars({ rv, onOpen, identifying }: { rv: ReviewModel; onOpen: () => void; identifying: boolean }) {
  const visible = new Set(rv.frames.map((frame) => frame.id));
  const groups = new Map<string, typeof rv.review.detections>();
  for (const detection of rv.review.detections.filter((d) => visible.has(d.image_id) && (d.region_type ?? 'vehicle') === 'vehicle')) {
    const attrs = parseAttributes(detection);
    const identity = attrs.plate || (attrs.race_number && `${attrs.race_number}-${attrs.make ?? ''}-${attrs.model ?? ''}`);
    const frame = rv.review.frames.find((item) => item.id === detection.image_id);
    const key = detection.group_key != null ? `group-${detection.group_key}` : identity ? `identity-${identity}` : identifying && frame?.burst_key != null ? `processing-burst-${frame.burst_key}` : `single-${detection.id}`;
    const list = groups.get(key); if (list) list.push(detection); else groups.set(key, [detection]);
  }
  const cards = [...groups.entries()].map(([key, detections]) => {
    const frames = detections.map((d) => rv.review.frames.find((frame) => frame.id === d.image_id)).filter((frame): frame is Frame => Boolean(frame));
    const pick = frames.find((frame) => rv.facts.get(frame.id)?.pick) ?? [...frames].sort((a, b) => frameStars(b) - frameStars(a))[0];
    return { key, detections, frames, pick, attrs: parseAttributes(detections[0]), processing: identifying && key.startsWith('processing-') };
  }).filter((group) => group.pick).sort((a, b) => a.pick.id - b.pick.id);
  return <div className="suspect-grid" role="list" aria-label="Suspected vehicles">
    {cards.map((group) => <button key={group.key} role="listitem" className={`suspect-card${group.processing ? ' processing' : ''}${group.frames.some((f) => f.id === rv.selected) ? ' selected' : ''}`} onClick={() => rv.setSelected(group.pick.id)} onDoubleClick={onOpen}>
      <div className="suspect-image"><LoadableImage src={asset(group.pick.thumb_path)} alt={group.attrs.plate || group.attrs.race_number || 'Suspected vehicle'} /><span className="stack-count">{group.frames.length}</span></div>
      <div><b>{group.processing ? 'Being identified' : [group.attrs.make, group.attrs.model].filter(Boolean).join(' ') || 'Needs identification'}</b><span>{group.processing ? `Analysing ${group.frames.length} photos from this burst` : group.attrs.plate || (group.attrs.race_number && `#${group.attrs.race_number}`) || 'No reliable plate or number found'}{!group.processing && group.attrs.team ? ` · ${group.attrs.team}` : ''}</span></div>
    </button>)}
    {!cards.length && <Empty icon="images" title="No vehicles need checking." />}
  </div>;
}

function currentItem(task: Task): string {
  const detail = task.detail?.trim();
  if (!detail) return 'Preparing the next photograph…';
  return detail.split(/[\\/]/).at(-1) ?? detail;
}

function IdentificationBanner({ task, settings, onSkip }: { task: Task; settings: Settings; onSkip: () => void }) {
  const targets = [
    settings.read_plates && 'registration plates',
    settings.read_numbers && 'race numbers',
    settings.use_vlm && 'make, model, colour, team, driver and country',
    settings.group_vehicles && 'similar-looking vehicles',
  ].filter(Boolean) as string[];
  const models = [
    settings.read_plates && 'plate reader',
    (settings.read_plates || settings.read_numbers) && 'OCR',
    settings.use_vlm && String(settings.vlm_model || 'vision model'),
    settings.group_vehicles && 'visual matcher',
  ].filter(Boolean) as string[];
  const percent = task.total > 0 ? Math.min(100, Math.round(task.done / task.total * 100)) : 0;
  return <section className="identify-banner" aria-live="polite">
    <div className="identify-copy">
      <div className="identify-title"><span className="work-spinner" /><div><b>Identifying vehicles</b><span>Results below are provisional and will merge as this pass finishes.</span></div><strong>{percent}%</strong></div>
      <div className="identify-current"><span>Working on</span><b title={task.detail}>{currentItem(task)}</b><small>{task.done.toLocaleString()} of {task.total.toLocaleString()} subjects</small></div>
      <div className="identify-targets"><span>Reading</span>{targets.map((target) => <em key={target}>{target}</em>)}</div>
      <div className="identify-footer"><small className="identify-models">Using {models.join(' · ') || 'available local models'}</small><button className="ghost small" title="Stop reading identity details, keep the culling results, and continue to the album." onClick={onSkip}>Skip identification</button></div>
    </div>
    <div className="identify-progress"><i style={{ width: `${percent}%` }} /></div>
  </section>;
}

type Props = {
  rv: ReviewModel; jobs: Job[]; jobId: number | null; scanning: boolean; status: Status; settings: Settings;
  onPickJob: (id: number) => void; onIdentify: () => void;
  onHelp: () => void; onOpenViewer: () => void; onLibrary: () => void;
  onEdit: (detectionId: number, updates: Record<string, string | string[] | null>) => unknown;
  onAcceptAll: () => void; onSkipIdentify: () => void;
};

/** Toolbar (album, view, search, stars, sort, actions), facets, the card grid and the inspector. */
export function ReviewScreen({ rv, jobs, jobId, scanning, status, settings, onPickJob, onIdentify, onHelp, onOpenViewer, onLibrary, onEdit, onAcceptAll, onSkipIdentify }: Props) {
  const [known, setKnown] = useState<KnownVehicle[]>([]);
  useEffect(() => { void engine.known().then(setKnown); }, [jobId]);
  const { filters, patchFilters, frames, review, facts, selected, frame } = rv;
  if (jobId == null) {
    return (
      <div className="screen">
        <Empty icon="folder" title="Choose a shoot to get started.">
          <p>Open an album from the library to review it here.</p>
          <button className="primary" onClick={onLibrary}>Open library</button>
        </Empty>
      </div>
    );
  }
  const filtered = filters.search !== '' || filters.facet !== null;
  const identifyTask = status.tasks.find((task) => (task.state === 'running' || task.state === 'paused') && /^Identifying\b/.test(task.label) && task.label.includes(`album ${jobId}`));
  const identifying = Boolean(identifyTask || status.operations.some((key) => key === `Identifying:${jobId}`));
  return (
    <div className="review">
      <div className="toolbar">
        <select aria-label="Album" value={jobId} onChange={(e) => onPickJob(Number(e.target.value))}>
          {jobs.map((j) => <option key={j.id} value={j.id}>{j.label}</option>)}
        </select>
        <input id="search" type="search" aria-label="Search photos" placeholder="Search make, team, plate…" spellCheck={false}
          value={filters.search} onChange={(e) => patchFilters({ search: e.target.value })} />
        <div className="spacer" />
        <span className="stats"><b>{frames.length.toLocaleString()}</b> photos {identifying ? 'will need checking' : 'still need checking'}</span>
        <button className="ghost icon" title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts" onClick={onHelp}><Icon name="keyboard" /></button>
        <button className="ghost" title="Read vehicle details and prepare detected faces for name suggestions." disabled={identifying} onClick={onIdentify}>{identifying ? 'Identifying…' : 'Identify'}</button>
        <button className="primary" title={identifying ? 'Wait for identification to finish before accepting results.' : 'Confirm every detection in this album and add identified cars to Known vehicles.'} disabled={identifying || !review.detections.some((d) => !d.reviewed)} onClick={onAcceptAll}>Accept all</button>
      </div>
      {identifyTask && <IdentificationBanner task={identifyTask} settings={settings} onSkip={onSkipIdentify} />}
      {scanning && <div className="review-note">This album is still being scanned. Photographs appear as they are measured.</div>}
      <div className="review-main">
        <Facets facets={rv.facets} facet={filters.facet} onPick={(facet) => patchFilters({ facet })} />
        {!filters.facet ? <SuspectedCars rv={rv} onOpen={onOpenViewer} identifying={identifying} /> : <PhotoGrid frames={frames} facts={facts} selected={selected} onSelect={rv.setSelected} onOpen={onOpenViewer}
          onReject={(id, rejected) => void rv.mark({ rejected }, id)}
          empty={
            <Empty icon="images" title={review.frames.length ? 'Nothing matches these filters.' : scanning ? 'Waiting for the first frames…' : 'No photographs to show yet.'}>
              {filtered && <button className="ghost" onClick={() => patchFilters({ ...defaultFilters })}>Clear filters</button>}
            </Empty>
          } />}
        <Inspector frame={frame} detections={frame ? facts.get(frame.id)?.dets ?? [] : []} allDetections={review.detections} allFrames={review.frames} known={known} scanning={scanning}
          onMark={(values) => void rv.mark(values)} onOpen={onOpenViewer} onEdit={onEdit} />
      </div>
    </div>
  );
}
