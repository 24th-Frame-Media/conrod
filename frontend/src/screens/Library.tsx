import { useEffect, useRef, useState } from 'react';
import { asset, call } from '../lib/api';
import { Confirm, Empty, Icon } from '../components/basics';
import { GlobalStatsModal } from '../components/GlobalStatsModal';
import { coverTint, etaText, pct } from '../lib/format';
import { overallEta, overallFraction } from '../lib/scan.mjs';
import {
  STATUS_OPTIONS, SORT_OPTIONS, albumCount, cardView, groupAlbums, groupTitle,
  setupLine, stripWork, visibleJobs,
} from '../lib/library.mjs';
import type { ScanLive } from '../state/useScanProgress';
import type { Job } from '../lib/types';
import '../styles/library.css';

/** Model readiness for the strip's "Set up" line, computed once in App. */
type Setup = { needed: boolean; ready: number; total: number };
type Props = {
  jobs: Job[]; scanning: boolean; live: Record<number, ScanLive>; activeJob: number | null;
  /** Albums waiting their turn, first to run first. */
  queue: number[]; setup: Setup;
  onOpen: (job: Job) => void; onNewScan: () => void; onResume: (job: Job) => void;
  onDelete: (job: Job) => void; onUnqueue: (job: Job) => void; onOpenSettings: () => void;
};

/** One album cover card. The same six parts in every state: chip, overflow,
 *  3:2 cover (or placeholder), badge, name + meta, one primary action. */
function JobCard({ job, active, live, queue, scanningElsewhere, onOpen, onResume, onUnqueue, onAsk }: {
  job: Job; active: boolean; live?: ScanLive; queue: number[]; scanningElsewhere: boolean;
  onOpen: () => void; onResume: () => void; onUnqueue: () => void; onAsk: () => void;
}) {
  // While this album scans, follow the live pipeline: overall progress, not just frames written.
  const prog = active ? live?.progress : undefined;
  const percent = prog ? Math.round(overallFraction(prog.pipeline.stages) * 100) : pct(job.done, job.total);
  const stage = prog?.pipeline.stages.find((s) => s.id === prog.stage)?.label;
  const eta = prog && live?.stable ? etaText(overallEta(prog.pipeline.stages)) : '';
  const view = cardView(job, { queue, scanningElsewhere, percent, stage, eta, queuePosition: queue.indexOf(job.id) });

  const [cover, setCover] = useState<string>();
  useEffect(() => {
    let alive = true;
    void call<{ path: string } | null>('cover', { jobId: job.id })
      .then((c) => { if (alive) setCover(asset(c?.path)); })
      .catch(() => undefined);
    return () => { alive = false; };
  }, [job.id, job.done]);

  // The ⋯ overflow is always visible; its one item opens the unchanged Confirm dialog.
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menuOpen) return;
    const away = (e: PointerEvent) => { if (menuRef.current && !menuRef.current.contains(e.target as Node)) setMenuOpen(false); };
    window.addEventListener('pointerdown', away);
    return () => window.removeEventListener('pointerdown', away);
  }, [menuOpen]);

  const runPrimary = () => {
    if (view.primary.action === 'unqueue') onUnqueue();
    else if (view.primary.action === 'resume') onResume();
    else onOpen();
  };
  const showPlaceholder = view.placeholder || !cover;

  return (
    <article className="lib-card" tabIndex={0} onClick={onOpen}
      onKeyDown={(e) => { if (e.key === 'Enter' && e.target === e.currentTarget) onOpen(); }}>
      <div className="lib-shot" style={{ background: coverTint(job.id) }}>
        {showPlaceholder ? (
          <div className="lib-placeholder"><Icon name="folder" size={30} /><span>No preview yet</span></div>
        ) : (
          <img className="lib-cover" src={cover} alt="" />
        )}
        <span className={`lib-chip lib-chip--${view.chip.tone}`}>{view.chip.text}</span>
        <div className="lib-overflow" ref={menuRef}>
          <button className="lib-overflow-btn" aria-label="Album options" aria-haspopup="menu" aria-expanded={menuOpen}
            onClick={(e) => { e.stopPropagation(); setMenuOpen((v) => !v); }}>&#8943;</button>
          {menuOpen && (
            <div className="lib-menu" role="menu">
              <button className="lib-menu-item" role="menuitem"
                onClick={(e) => { e.stopPropagation(); setMenuOpen(false); onAsk(); }}>Forget album</button>
            </div>
          )}
        </div>
        {view.badge && <span className="lib-badge">{view.badge}</span>}
        {view.bar && <div className="lib-bar"><div className="lib-bar-fill" style={{ width: `${view.percent}%` }} /></div>}
      </div>
      <div className="lib-name" title={job.root}>{job.label}</div>
      <div className="lib-meta">{view.meta}</div>
      <button className="lib-primary" title={view.primary.hint || undefined}
        onClick={(e) => { e.stopPropagation(); runPrimary(); }}>{view.primary.label}</button>
      {view.primary.hint && <div className="lib-hint">{view.primary.hint}</div>}
    </article>
  );
}

/** Home: one column, no rail. Start zone → sticky status strip → filter row and
 *  two labelled groups of cover cards. Setup and lifetime totals live in
 *  Settings / the health popover and the GlobalStatsModal. */
export function Library({ jobs, scanning, live, activeJob, queue, setup, onOpen, onNewScan, onResume, onDelete, onUnqueue, onOpenSettings }: Props) {
  const [doomed, setDoomed] = useState<Job | null>(null);
  const [statsOpen, setStatsOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState('all');
  const [sort, setSort] = useState('newest');

  // One running scan: trust the engine's active slot rather than a stale DB `scanning` status.
  const scanJob = activeJob != null ? (jobs.find((j) => j.id === activeJob) ?? null) : null;
  const aScanIsRunning = scanning || scanJob != null;
  const scanLive = scanJob ? live[scanJob.id] : undefined;
  const scanProg = scanLive?.progress;
  const scanPercent = scanProg ? Math.round(overallFraction(scanProg.pipeline.stages) * 100) : scanJob ? pct(scanJob.done, scanJob.total) : 0;
  const scanStage = scanProg?.pipeline.stages.find((s) => s.id === scanProg.stage)?.label;
  const scanEta = scanProg && scanLive?.stable ? etaText(overallEta(scanProg.pipeline.stages)) : '';
  const scan = scanJob ? { label: scanJob.label, percent: scanPercent, stage: scanStage, eta: scanEta } : null;
  const queueLabels = queue.map((id) => jobs.find((j) => j.id === id)?.label).filter((l): l is string => Boolean(l));
  const errorJob = jobs.find((j) => j.status === 'error');
  const otherUnfinished = jobs.some((j) => j.id !== scanJob?.id && (j.total === 0 || ['stopped', 'incomplete', 'queued'].includes(j.status)));
  const work = stripWork({
    scan, queueLabels,
    error: errorJob ? { label: errorJob.label, kind: 'folder_unreachable' } : null,
    elsewhere: aScanIsRunning && otherUnfinished,
  });
  const setupText = setupLine(setup);

  const visible = visibleJobs(jobs, { search, status, activeJob, queue });
  const groups = groupAlbums(visible, { activeJob, queue, sort });
  const clearFilters = () => { setSearch(''); setStatus('all'); setSort('newest'); };

  return (
    <div className="screen lib">
      <header className="lib-start">
        <div className="lib-start-text">
          <h1>Your albums</h1>
          <p className="lib-helper">Import a shoot: Conrod measures sharpness, groups bursts and suggests keepers.</p>
        </div>
        <button className="primary lib-import" onClick={onNewScan}><Icon name="plus" size={15} /> Import photos</button>
      </header>

      <div className="lib-strip">
        <span className={`lib-work lib-work--${work.tone}`}>
          <span className="lib-work-text">{work.text}</span>
          {work.watch && scanJob && <button className="lib-link" onClick={() => onOpen(scanJob)}>Watch scan</button>}
        </span>
        <div className="lib-strip-right">
          {setupText && <span>{setupText} ·</span>}
          <button className="lib-link" onClick={onOpenSettings}>Set up</button>
          <span className="lib-sep">·</span>
          <button className="lib-link" onClick={() => setStatsOpen(true)}>View statistics</button>
        </div>
      </div>

      {jobs.length === 0 ? (
        <Empty icon="folder" title="No albums yet">
          <p>Drop a folder with photos anywhere on this window, or press <b>Import photos</b>. Conrod measures sharpness, groups bursts and suggests keepers.</p>
        </Empty>
      ) : (
        <div className="lib-zone">
          <div className="lib-filters">
            <input className="lib-search" type="search" placeholder="Search albums" aria-label="Search albums"
              value={search} onChange={(e) => setSearch(e.target.value)} />
            <select value={status} aria-label="Filter by status" onChange={(e) => setStatus(e.target.value)}>
              {STATUS_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
            <select value={sort} aria-label="Sort albums" onChange={(e) => setSort(e.target.value)}>
              {SORT_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
            <span className="lib-count">{albumCount(visible.length)}</span>
          </div>

          {visible.length === 0 ? (
            <Empty icon="images" title="No albums match these filters">
              <button className="ghost" onClick={clearFilters}>Clear filters</button>
            </Empty>
          ) : (
            [
              { key: 'attention', label: 'Needs attention', items: groups.attention },
              { key: 'ready', label: 'Ready to review', items: groups.ready },
            ].map(({ key, label, items }) => items.length ? (
              <section className="lib-group" key={key}>
                <h2 className="lib-group-head" aria-label={groupTitle(label, items.length)}>
                  {label} <span className="lib-group-count">· {items.length}</span>
                </h2>
                <div className="lib-cards">
                  {items.map((job) => (
                    <JobCard key={job.id} job={job} active={job.id === activeJob} live={live[job.id]} queue={queue}
                      scanningElsewhere={aScanIsRunning}
                      onOpen={() => onOpen(job)} onResume={() => onResume(job)} onUnqueue={() => onUnqueue(job)}
                      onAsk={() => setDoomed(job)} />
                  ))}
                </div>
              </section>
            ) : null)
          )}
        </div>
      )}

      {doomed && (
        <Confirm title="Forget this album?" action="Forget album" onCancel={() => setDoomed(null)}
          onConfirm={() => { onDelete(doomed); setDoomed(null); }}>
          <p><b>{doomed.label}</b>: its results and cached crops go. Your photographs and any XMP already written are not touched.</p>
          {(doomed.id === activeJob || queue.includes(doomed.id)) && (
            <p className="warn-note">
              {doomed.id === activeJob
                ? 'This album is scanning right now. Stop the scan first: Conrod will refuse to delete an album that is still being read.'
                : 'This album is waiting to scan. Deleting it also takes it out of the queue.'}
            </p>
          )}
        </Confirm>
      )}
      <GlobalStatsModal visible={statsOpen} onClose={() => setStatsOpen(false)} />
    </div>
  );
}
