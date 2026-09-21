import { Fragment, useMemo, useState, type CSSProperties } from 'react';
import { AlbumTools } from '../components/AlbumTools';
import { Empty } from '../components/basics';
import { LoadableImage } from '../components/Media';
import { StarPill } from '../components/Stars';
import { asset } from '../lib/api';
import { filename, frameStars } from '../lib/review';
import type { Frame, Job } from '../lib/types';
import type { ReviewModel } from '../state/useReview';

type Burst = { key: string; frames: Frame[]; pick: Frame };
type Props = {
  rv: ReviewModel; jobs: Job[]; jobId: number | null; onPickJob: (id: number) => void;
  onReview: () => void; onIdentify: () => void; onWrite: (dryRun: boolean) => void; onOpenViewer: () => void;
};

export function Album({ rv, jobs, jobId, onPickJob, onReview, onIdentify, onWrite, onOpenViewer }: Props) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [sort, setSort] = useState<'shooting' | 'best' | 'largest'>('shooting');
  const [search, setSearch] = useState('');
  const job = jobs.find((item) => item.id === jobId);
  const bursts = useMemo(() => {
    const map = new Map<string, Frame[]>();
    for (const frame of rv.review.frames.filter((item) => item.status === 'done')) {
      const key = frame.burst_key == null ? `frame-${frame.id}` : `burst-${frame.burst_key}`;
      const list = map.get(key); if (list) list.push(frame); else map.set(key, [frame]);
    }
    const list: Burst[] = [...map].map(([key, frames]) => ({
      key, frames, pick: frames.find((frame) => rv.facts.get(frame.id)?.pick) ?? [...frames].sort((a, b) => frameStars(b) - frameStars(a))[0],
    }));
    const needle = search.trim().toLowerCase();
    const shown = needle ? list.filter((burst) => burst.frames.some((frame) => rv.facts.get(frame.id)?.text.includes(needle))) : list;
    if (sort === 'best') return shown.sort((a, b) => frameStars(b.pick) - frameStars(a.pick));
    if (sort === 'largest') return shown.sort((a, b) => b.frames.length - a.frames.length || a.pick.id - b.pick.id);
    return shown.sort((a, b) => a.pick.id - b.pick.id);
  }, [rv.review.frames, rv.facts, search, sort]);
  if (jobId == null) return <div className="screen"><Empty icon="folder" title="Choose an album from the Library." /></div>;
  const toggle = (id: number) => setSelected((before) => { const next = new Set(before); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  const detectionIds = rv.review.detections.filter((d) => selected.has(d.image_id)).map((d) => d.id);
  return (
    <div className="album-workspace">
      <div className="toolbar album-toolbar">
        <select aria-label="Album" value={jobId} onChange={(e) => onPickJob(Number(e.target.value))}>{jobs.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}</select>
        <input type="search" aria-label="Search album" placeholder="Search metadata…" value={search} onChange={(e) => setSearch(e.target.value)} />
        <select aria-label="Sort burst stacks" value={sort} onChange={(e) => setSort(e.target.value as typeof sort)}><option value="shooting">Shooting order</option><option value="best">Best pick first</option><option value="largest">Largest burst first</option></select>
        <span className="stats"><b>{bursts.length.toLocaleString()}</b> stacks · {selected.size.toLocaleString()} selected</span>
        <div className="spacer" />
        <button onClick={onReview}>Check identification</button>
        <button onClick={onIdentify}>Identify</button>
        <button onClick={() => onWrite(true)}>Dry run</button>
        <button className="primary" onClick={() => onWrite(false)}>Write XMP</button>
      </div>
      <div className="album-actions"><button onClick={() => setSelected(new Set(rv.review.frames.map((frame) => frame.id)))}>Select all</button><button onClick={() => setSelected(new Set())}>Clear</button><AlbumTools imageIds={[...selected]} jobId={jobId} label={job?.label ?? ''} refresh={rv.refresh} ids={detectionIds} /></div>
      <div className="burst-grid">
        {bursts.map((burst) => {
          const facts = rv.facts.get(burst.pick.id);
          const open = expanded === burst.key;
          return <Fragment key={burst.key}>
            <article className={`burst-stack${open ? ' open' : ''}`} onClick={() => { rv.setSelected(burst.pick.id); setExpanded(open ? null : burst.key); }}>
              <div className="burst-layers" style={{ '--layers': Math.min(burst.frames.length, 4) } as CSSProperties}>
                <LoadableImage src={asset(burst.pick.thumb_path)} alt={filename(burst.pick.path)} />
                <StarPill frame={burst.pick} />
                <span className="stack-count">{burst.frames.length}</span>
                {facts?.pick && <span className="focus keeper">pick</span>}
              </div>
              <div className="burst-caption"><b>{facts?.vehicle || facts?.plate || facts?.number && `#${facts.number}` || filename(burst.pick.path)}</b><span>{facts?.plate || ''}{facts?.team ? ` · ${facts.team}` : ''}</span></div>
            </article>
            {open && <div className="burst-expanded">
              <div className="burst-expanded-head"><b>{burst.frames.length} frames</b><button onClick={(e) => { e.stopPropagation(); setSelected(new Set(burst.frames.map((frame) => frame.id))); }}>Select burst</button></div>
              <div className="burst-strip">{burst.frames.map((frame) => <button key={frame.id} className={rv.selected === frame.id ? 'selected' : ''} onClick={() => rv.setSelected(frame.id)} onDoubleClick={onOpenViewer}><input type="checkbox" aria-label={`Select ${filename(frame.path)}`} checked={selected.has(frame.id)} onClick={(e) => e.stopPropagation()} onChange={() => toggle(frame.id)} /><LoadableImage src={asset(frame.thumb_path)} alt={filename(frame.path)} /><StarPill frame={frame} /><small>{filename(frame.path)}</small></button>)}</div>
            </div>}
          </Fragment>;
        })}
        {!bursts.length && <Empty icon="images" title="No finished burst stacks match." />}
      </div>
    </div>
  );
}
