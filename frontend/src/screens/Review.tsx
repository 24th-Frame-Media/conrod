import { Fragment, useMemo, CSSProperties, useEffect, useState } from 'react';
import { Empty, Icon } from '../components/basics';
import { Facets } from '../components/Facets';
import { Inspector } from '../components/Inspector';
import { PhotoGrid } from '../components/PhotoGrid';
import { LoadableImage } from '../components/Media';
import { defaultFilters, frameStars, parseAttributes, filename } from '../lib/review';
import { StarPill } from '../components/Stars';
import { AlbumTools } from '../components/AlbumTools';
import { asset, engine } from '../lib/api';
import type { Frame, Job, KnownVehicle, Settings, Status, Task } from '../lib/types';
import { profileLabel } from '../lib/types';
import type { ReviewModel } from '../state/useReview';
import { subjectExcluded } from '../review.mjs';

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
  const models = [
    settings.read_plates && 'plate reader',
    (settings.read_plates || settings.read_numbers) && 'OCR',
    settings.use_vlm && String(settings.vlm_model || 'vision model'),
    settings.group_vehicles && 'visual matcher',
  ].filter(Boolean) as string[];
  const percent = task.total > 0 ? Math.min(100, Math.round((task.done / task.total) * 100)) : 0;

  return (
    <section className="identify-strip" aria-live="polite">
      <div className="identify-strip-main">
        <div className="identify-strip-left">
          <span className="work-spinner" />
          <span className="identify-strip-title">Identifying vehicles</span>
          <span className="identify-strip-pct">{percent}%</span>
          <span className="identify-strip-sep">·</span>
          <span className="identify-strip-file" title={task.detail}>{currentItem(task)}</span>
          <span className="identify-strip-count">({task.done.toLocaleString()} of {task.total.toLocaleString()})</span>
        </div>
        <div className="identify-strip-right">
          {models.length > 0 && <span className="identify-strip-models">Using {models.join(' · ')}</span>}
          <button
            type="button"
            className="ghost small identify-strip-skip"
            title="Stop reading identity details, keep current results, and continue to the album."
            onClick={onSkip}
          >
            Skip
          </button>
        </div>
      </div>
      <div className="identify-progress"><i style={{ width: `${percent}%` }} /></div>
    </section>
  );
}

function PipelineStepper({ job, scanning, identified, identifying, written }: { job: Job | undefined; scanning: boolean; identified: boolean; identifying: boolean; written: boolean }) {
  const steps = [
    { key: 'scan', label: 'Scanned', done: Boolean(job && job.status !== 'scanning'), active: scanning },
    { key: 'cull', label: 'Culled', done: Boolean(job && (job.status === 'done' || job.status === 'indexed')), active: false },
    { key: 'identify', label: 'Identified', done: identified && !identifying, active: identifying },
    { key: 'write', label: 'XMP written', done: written, active: false },
  ];
  return (
    <nav className="pipeline" aria-label="Processing pipeline">
      {steps.map((step, i) => (
        <span key={step.key} className={`pipeline-step${step.done ? ' done' : ''}${step.active ? ' active' : ''}`}>
          <span className="pipeline-dot">{step.done ? '✓' : i + 1}</span>
          <span className="pipeline-label">{step.label}</span>
        </span>
      ))}
    </nav>
  );
}

type Burst = { key: string; frames: Frame[]; pick: Frame };

function BurstStacks({ rv, onOpenViewer }: { rv: ReviewModel; onOpenViewer: () => void }) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const bursts = useMemo(() => {
    const map = new Map<string, Frame[]>();
    for (const frame of rv.review.frames.filter((item) => item.status === 'done')) {
      const key = frame.burst_key == null ? `frame-${frame.id}` : `burst-${frame.burst_key}`;
      const list = map.get(key); if (list) list.push(frame); else map.set(key, [frame]);
    }
    const scoreOf = (f: Frame) => {
      // Manual stars outrank everything (50 max)
      const stars = (f.manual_stars ?? 0) * 10;
      // Detector burst pick bonus
      const isPick = (rv.facts.get(f.id)?.pick || Boolean(f.burst_pick)) ? 2.0 : 0.0;
      // Measured rating
      const rating = (f.rating ?? 0) * 5.0;
      return stars + isPick + rating;
    };
    return [...map].map(([key, rawFrames]) => {
      const sortedByBest = [...rawFrames].sort((a, b) => scoreOf(b) - scoreOf(a));
      const pick = sortedByBest[0];
      const remaining = rawFrames.filter((f) => f.id !== pick.id).sort((a, b) => a.id - b.id);
      const frames = [pick, ...remaining];
      return { key, frames, pick };
    }).sort((a, b) => a.pick.id - b.pick.id);
  }, [rv.review.frames, rv.facts]);
  return <div className="burst-grid" role="list" aria-label="Burst stacks">
    {bursts.map((burst) => {
      const facts = rv.facts.get(burst.pick.id);
      const open = expanded === burst.key;
      return <Fragment key={burst.key}>
        <article className={`burst-stack${open ? ' open' : ''}`} onClick={() => { rv.setSelected(burst.pick.id); setExpanded(open ? null : burst.key); }}>
          <div className="burst-layers" style={{ '--layers': Math.min(burst.frames.length, 4) } as CSSProperties}>
            <LoadableImage src={asset(burst.pick.thumb_path)} alt={filename(burst.pick.path)} />
            <StarPill frame={burst.pick} />
            <span className="stack-count">{burst.frames.length}</span>
            <span className="focus keeper">Best</span>
          </div>
          <div className="burst-caption">
            <b>{facts?.vehicle || facts?.plate || (facts?.number && `#${facts.number}`) || filename(burst.pick.path)}</b>
            <span>{burst.frames.length} shots{facts?.plate ? ` · ${facts.plate}` : ''}{facts?.team ? ` · ${facts.team}` : ''}</span>
          </div>
        </article>
        {open && <div className="burst-expanded">
          <div className="burst-expanded-head">
            <b>{burst.frames.length} frames</b>
            <span className="muted" style={{ marginLeft: 8 }}>· Best photo first</span>
          </div>
          <div className="burst-strip">
            {burst.frames.map((frame, index) => (
              <button key={frame.id} className={`${rv.selected === frame.id ? 'selected' : ''}${index === 0 ? ' burst-best-card' : ''}`} onClick={() => rv.setSelected(frame.id)} onDoubleClick={onOpenViewer}>
                <div style={{ position: 'relative' }}>
                  <LoadableImage src={asset(frame.thumb_path)} alt={filename(frame.path)} />
                  {index === 0 && <span className="focus keeper" style={{ position: 'absolute', top: 4, left: 4, zIndex: 2 }}>Best</span>}
                </div>
                <StarPill frame={frame} />
                <small>{filename(frame.path)}</small>
              </button>
            ))}
          </div>
        </div>}
      </Fragment>;
    })}
    {!bursts.length && <Empty icon="images" title="No finished burst stacks." />}
  </div>;
}

type Props = {
  rv: ReviewModel; jobs: Job[]; jobId: number | null; scanning: boolean; status: Status; settings: Settings;
  onPickJob: (id: number) => void; onIdentify: () => void; onCull: () => void;
  onHelp: () => void; onOpenViewer: () => void; onLibrary: () => void;
  onEdit: (detectionId: number, updates: Record<string, string | string[] | null>) => unknown;
  onAcceptAll: () => void; onSkipIdentify: () => void; onWrite: (dryRun: boolean) => void;
};

/** Toolbar (album, view, search, stars, sort, actions), facets, the card grid and the inspector. */
export function ReviewScreen({ rv, jobs, jobId, scanning, status, settings, onPickJob, onIdentify, onCull, onHelp, onOpenViewer, onLibrary, onEdit, onAcceptAll, onSkipIdentify, onWrite }: Props) {
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
  const excluded = review.frames.filter((f) => facts.get(f.id)?.cull).length;
  const pending = review.frames.filter((f) => !facts.get(f.id)?.cull && f.status !== 'done').length;
  const eligible = review.frames.filter((f) => (facts.get(f.id)?.dets ?? []).some((d) => !d.reviewed && !subjectExcluded(f, d))).length;
  const busy = scanning || status.operations.some((key) => key.endsWith(`:${jobId}`));
  return (
    <div className="review">
      <div className="toolbar">
        <select aria-label="Album" value={jobId} onChange={(e) => onPickJob(Number(e.target.value))}>
          {jobs.map((j) => <option key={j.id} value={j.id}>{j.label}</option>)}
        </select>
        {jobs.find((j) => j.id === jobId)?.scan_profile && (
          <span className="pill scan-type" style={{ alignSelf: 'center', pointerEvents: 'none', cursor: 'default' }} title={`Preset: ${profileLabel(jobs.find((j) => j.id === jobId)?.scan_profile ?? '')}`}>
            {profileLabel(jobs.find((j) => j.id === jobId)?.scan_profile ?? '')}
          </span>
        )}
        <input id="search" type="search" aria-label="Search photos" placeholder="Search make, team, plate…" spellCheck={false}
          value={filters.search} onChange={(e) => patchFilters({ search: e.target.value })} />
        <select aria-label="Photo selection" value={filters.view} onChange={(e) => patchFilters({ view: e.target.value as typeof filters.view })}>
          <option value="all">All photos</option><option value="kept">Kept photos</option><option value="rejected">Culled / rejected</option><option value="picks">Burst picks</option><option value="review">Needs review</option><option value="stacks">Burst stacks</option><option value="vehicles">Group by vehicle</option>
        </select>
        <select aria-label="Photo order" value={filters.sort} onChange={(e) => patchFilters({ sort: e.target.value as typeof filters.sort })}>
          <option value="frame">Filename order</option><option value="best">Highest rated first</option><option value="worst">Lowest rated first</option><option value="pick">Burst picks first</option>
        </select>
        <div className="spacer" />
        {!busy && pending > 0 && <button className="ghost" onClick={onCull}>Suggest culls ({pending.toLocaleString()})</button>}
        {!busy && pending === 0 && eligible > 0 && !identifying && <button className="primary" onClick={onIdentify}>Identify kept photos ({eligible.toLocaleString()})</button>}
        {identifying && <button className="ghost" disabled>Identifying…</button>}
        {!identifying && review.detections.some((d) => !d.reviewed) && <button onClick={onAcceptAll}>Accept all</button>}
        <button className="ghost" onClick={() => onWrite(true)}>Dry run</button>
        <button className="primary" onClick={() => onWrite(false)}>Write XMP</button>
        <button className="ghost icon" title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts" onClick={onHelp}><Icon name="keyboard" /></button>
      </div>
      <PipelineStepper job={jobs.find((j) => j.id === jobId)} scanning={scanning} identified={!eligible || review.detections.some((d) => d.reviewed)} identifying={identifying} written={false} />
      <AlbumTools imageIds={[]} jobId={jobId} label={jobs.find((j) => j.id === jobId)?.label ?? ''} refresh={rv.refresh} ids={[]} />
      {identifyTask && <IdentificationBanner task={identifyTask} settings={settings} onSkip={onSkipIdentify} />}
      <div className="review-main">
        <Facets facets={rv.facets} facet={filters.facet} onPick={(facet) => patchFilters({ facet })} />
        {filters.view === 'stacks'
          ? <BurstStacks rv={rv} onOpenViewer={onOpenViewer} />
          : filters.view === 'vehicles' ? <SuspectedCars rv={rv} onOpen={onOpenViewer} identifying={identifying} />
          : <PhotoGrid frames={frames} facts={facts} selected={selected} onSelect={rv.setSelected} onOpen={onOpenViewer}
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
