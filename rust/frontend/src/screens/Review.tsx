import { useEffect, useState } from 'react';
import { AlbumTools } from '../components/AlbumTools';
import { Confirm, Empty, Icon } from '../components/basics';
import { Facets } from '../components/Facets';
import { Inspector } from '../components/Inspector';
import { PhotoGrid } from '../components/PhotoGrid';
import { StarFilter } from '../components/Stars';
import { defaultFilters } from '../lib/review';
import type { Job, Sort, View } from '../lib/types';
import type { ReviewModel } from '../state/useReview';

const VIEWS: [View, string, string][] = [
  ['review', 'To review', 'Photographs you have not rated or rejected yet.'],
  ['all', 'All', 'Every photograph in the album.'],
  ['picks', 'Keepers', 'One frame per car per pass: the keeper of each pan, and nothing else.'],
  ['rejected', 'Rejected', 'Photographs you (or the cull) rejected.'],
];
const SORTS: [Sort, string][] = [
  ['frame', 'Shooting order'], ['review', 'Least sure first'], ['best', 'Best rated'], ['worst', 'Worst rated'], ['pick', 'Keeper of each pass first'],
];

type Props = {
  rv: ReviewModel; jobs: Job[]; jobId: number | null; scanning: boolean; scanRunning: boolean;
  onPickJob: (id: number) => void; onIdentify: () => void; onWrite: (dryRun: boolean) => void; overwrites: string[]; onResume: () => void;
  onHelp: () => void; onOpenViewer: () => void; onLibrary: () => void;
  onEdit: (detectionId: number, updates: Record<string, string | string[] | null>) => unknown;
};

/** Toolbar (album, view, search, stars, sort, actions), facets, the card grid and the inspector. */
export function ReviewScreen({ rv, jobs, jobId, scanning, scanRunning, onPickJob, onIdentify, onWrite, overwrites, onResume, onHelp, onOpenViewer, onLibrary, onEdit }: Props) {
  const [bulk, setBulk] = useState<Set<number>>(new Set());
  const [confirming, setConfirming] = useState(false);
  useEffect(() => setBulk(new Set()), [jobId]);
  const { filters, patchFilters, frames, review, facts, selected, frame } = rv;
  const job = jobs.find((j) => j.id === jobId);
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
  const filtered = filters.search !== '' || filters.minStars > 0 || filters.view !== 'all' || filters.facet !== null;
  return (
    <div className="review">
      <div className="toolbar">
        <select aria-label="Album" value={jobId} onChange={(e) => onPickJob(Number(e.target.value))}>
          {jobs.map((j) => <option key={j.id} value={j.id}>{j.label}</option>)}
        </select>
        <div className="tabs sub" role="tablist" aria-label="View">
          {VIEWS.map(([id, label, title]) => (
            <button key={id} role="tab" aria-selected={filters.view === id} title={title} className={filters.view === id ? 'active' : ''}
              onClick={() => patchFilters({ view: id, facet: null })}>{label}</button>
          ))}
        </div>
        <input id="search" type="search" aria-label="Search photos" placeholder="Search make, team, plate…" spellCheck={false}
          value={filters.search} onChange={(e) => patchFilters({ search: e.target.value })} />
        <StarFilter value={filters.minStars} onChange={(n) => patchFilters({ minStars: n })} />
        <select aria-label="Sort" title="Order the grid" value={filters.sort} onChange={(e) => patchFilters({ sort: e.target.value as Sort })}>
          {SORTS.map(([id, label]) => <option key={id} value={id}>{label}</option>)}
        </select>
        <div className="spacer" />
        <span className="stats"><b>{frames.length.toLocaleString()}</b> of {review.frames.length.toLocaleString()}</span>
        <button className="ghost icon" title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts" onClick={onHelp}><Icon name="keyboard" /></button>
        {job && job.status !== 'done' && !scanRunning && <button className="ghost" onClick={onResume}>Resume scan</button>}
        <button className="ghost" title="Name make, model, colour and team for this album's vehicles." onClick={onIdentify}>Identify</button>
        <button className="ghost" title="Work out what would be written. No file is touched." onClick={() => onWrite(true)}>Dry run</button>
        <button className="primary" title="Write ratings, labels and keywords to the sidecars." onClick={() => setConfirming(true)}>Write XMP</button>
      </div>
      {confirming && (
        <Confirm title="Write to your photographs?" action="Write XMP" onCancel={() => setConfirming(false)} onConfirm={() => { setConfirming(false); onWrite(false); }}>
          <p>Conrod will write, to XMP sidecars (RAW) and into the files themselves (JPEG):</p>
          <ul><li>keywords and a caption</li><li>a star rating and a colour label from the cull</li><li>a blue label on the keeper of each pass</li></ul>
          <p>{overwrites.length ? `Existing ${overwrites.join(' and ')} WILL be overwritten (turn off Overwrite in Settings to keep them).` : 'A rating or label you have already set is never overwritten.'}</p>
        </Confirm>
      )}
      <div className="actions-row"><button onClick={() => setBulk(new Set(frames.map(f => f.id)))}>Select filtered photos</button><button onClick={() => setBulk(new Set())}>Clear selection</button><span>{bulk.size} selected</span></div>
      <AlbumTools imageIds={[...bulk]} jobId={jobId} label={job?.label ?? ''} refresh={rv.refresh} ids={review.detections.filter(d => bulk.has(d.image_id)).map(d => d.id)} />
      {scanning && <div className="review-note">This album is still being scanned. Photographs appear as they are measured.</div>}
      <div className="review-main">
        <Facets facets={rv.facets} facet={filters.facet} onPick={(facet) => patchFilters({ facet })} />
        <PhotoGrid frames={frames} facts={facts} selected={selected} onToggle={(id) => setBulk(previous => { const next = new Set(previous); if (next.has(id)) next.delete(id); else next.add(id); return next; })} checked={bulk} onSelect={rv.setSelected} onOpen={onOpenViewer}
          onReject={(id, rejected) => void rv.mark({ rejected }, id)}
          empty={
            <Empty icon="images" title={review.frames.length ? 'Nothing matches these filters.' : scanning ? 'Waiting for the first frames…' : 'No photographs to show yet.'}>
              {filtered && <button className="ghost" onClick={() => patchFilters({ ...defaultFilters })}>Clear filters</button>}
            </Empty>
          } />
        <Inspector frame={frame} detections={frame ? facts.get(frame.id)?.dets ?? [] : []} scanning={scanning}
          onMark={(values) => void rv.mark(values)} onOpen={onOpenViewer} onEdit={onEdit} />
      </div>
    </div>
  );
}
