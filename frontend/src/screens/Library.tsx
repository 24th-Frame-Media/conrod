import { useRun } from '../state/hooks';
import { asset, call } from '../lib/api';
import { useEffect, useState } from 'react';
import { Confirm, Empty } from '../components/basics';
import { GlobalStatsModal } from '../components/GlobalStatsModal';
import { coverTint, etaText, hiddenCopies, jobState, pct, plural, shortDate } from '../lib/format';
import { overallEta, overallFraction } from '../lib/scan.mjs';
import { albumSetting } from '../lib/album.mjs';
import { identifyScopeOf } from '../lib/identify.mjs';
import type { ScanLive } from '../state/useScanProgress';
import { profileParent, type Job, type ModelInfo } from '../lib/types';

type Props = {
  jobs: Job[]; models: ModelInfo[]; scanning: boolean; live: Record<number, ScanLive>; activeJob: number | null;
  onOpen: (job: Job) => void; onNewScan: () => void; onIdentify: (job: Job) => void;
  onResume: (job: Job) => void; onDelete: (job: Job) => void; onToggleIncludePeople: (job: Job) => void;
};

function JobCard({ job, scanning, active, live, onOpen, onIdentify, onResume, onAsk, onToggleIncludePeople }: {
  job: Job; scanning: boolean; active: boolean; live?: ScanLive; onOpen: () => void; onIdentify: () => void; onResume: () => void; onAsk: () => void; onToggleIncludePeople: () => void;
}) {
  // While this album scans the card follows the live pipeline: overall progress, not just the frames written so far.
  const p = active ? live?.progress : undefined;
  const fraction = p ? Math.round(overallFraction(p.pipeline.stages) * 100) : pct(job.done, job.total);
  const stageLabel = p?.pipeline.stages.find((s) => s.id === p.stage)?.label;
  const eta = p && live?.stable ? etaText(overallEta(p.pipeline.stages)) : '';
  const [cover, setCover] = useState<string>();
  useEffect(() => { let live = true; void call<{path: string} | null>('cover', {jobId: job.id}).then(c => { if(live) setCover(asset(c?.path)); }).catch(() => undefined); return () => { live = false; }; }, [job.id, job.done]);
  const left = job.total - job.done;
  const resumable = left > 0 && job.status !== 'scanning' && !scanning;
  // The album's own scope, so the card's button names what the run will read.
  const identifyScope = identifyScopeOf(albumSetting(job, 'identify_scope', undefined));
  return (
    <article className="job-card" tabIndex={0} onClick={onOpen} onKeyDown={(e) => { if (e.key === 'Enter' && e.target === e.currentTarget) onOpen(); }}>
      <div className="shot" style={{ background: coverTint(job.id), backgroundImage: cover ? `url("${cover}")` : undefined, backgroundSize: 'cover', backgroundPosition: 'center' }}>
        {/* Where the photograph would be, as on the Python home screen: said only while there is no cover. */}
        {!cover && <div className="state">{active ? `Scanning ${fraction}%` : jobState(job)}</div>}
        <span className="badge">{plural(job.total, 'photo')}</span>
        {(active || job.status === 'scanning') && <div className="bar cover-bar"><div className="fill" style={{ width: `${fraction}%` }} /></div>}
        {resumable && <button className="resume" onClick={(e) => { e.stopPropagation(); onResume(); }}>{job.status === 'indexed' ? 'Suggest culls' : job.status === 'done_errors' ? 'Retry failed' : 'Resume'}</button>}
        <div className="job-menu">
          <button className="iconbtn danger" title="Forget this album. Your photographs are not touched" onClick={(e) => { e.stopPropagation(); onAsk(); }}>Delete</button>
        </div>
      </div>
      <div className="name" title={job.root}>{job.label}</div>
      <div className="sub">{active
        ? `Scanning · ${fraction}%${stageLabel ? ` · ${stageLabel}` : ''}${eta ? ` · ${eta}` : ''}`
        : `${job.done.toLocaleString()} / ${job.total.toLocaleString()} photos${hiddenCopies(job.copies)} · ${shortDate(job.created_at)}`}</div>
      {profileParent(job.scan_profile ?? '') === 'motorsport' && (
        <label className="sub" onClick={(e) => e.stopPropagation()}>
          <input type="checkbox" checked={Boolean(job.include_people)} onChange={onToggleIncludePeople} /> Includes people
        </label>
      )}
      <div className="steps">
        <button className="step" onClick={(e) => { e.stopPropagation(); onOpen(); }}>{active ? 'Watch scan' : 'Review'}</button>
        {['done', 'done_errors'].includes(job.status) && !active && (
          <button className="step" title={`Name what this album's identify scope covers: ${identifyScope === 'everything' ? 'every subject' : 'the suggested keeper of each burst'}.`} onClick={(e) => { e.stopPropagation(); onIdentify(); }}>
            Identify {identifyScope === 'everything' ? 'everything' : 'keepers'}
          </button>
        )}
      </div>
    </article>
  );
}

/** Home: the albums as cover cards, with totals and model setup in the rail, as on the Python home screen. */
export function Library({ jobs, models, scanning, live, activeJob, onOpen, onNewScan, onIdentify, onResume, onDelete, onToggleIncludePeople }: Props) {
  const run = useRun();
  const [doomed, setDoomed] = useState<Job | null>(null);
  const [statsOpen, setStatsOpen] = useState(false);
  const scanned = jobs.reduce((n, j) => n + j.done, 0);
  const ready = models.filter((m) => m.ready).length;
  return (
    <div className="screen">
      <div className="home">
        <div className="home-main">
          <div className="new-scan">
            <div className="new-scan-head">
              <div>
                <h4>Import a shoot</h4>
                <p>Copy your SD card to your PC, import the folder, then cull before running ML on your keepers.</p>
              </div>
              <button className="primary" onClick={onNewScan}>+ Import photos</button>
            </div>
          </div>
          <div className="section-label"><span>Albums</span><span>{jobs.length || ''}</span></div>
          {jobs.length ? (
            <div className="cards">
              {jobs.map((job) => (
                <JobCard key={job.id} job={job} scanning={scanning} active={job.id === activeJob} live={live[job.id]} onOpen={() => onOpen(job)} onIdentify={() => onIdentify(job)}
                  onResume={() => onResume(job)} onAsk={() => setDoomed(job)} onToggleIncludePeople={() => onToggleIncludePeople(job)} />
              ))}
            </div>
          ) : (
            <Empty icon="folder" title="No albums yet.">
              <p>Drop a folder anywhere on this window, or press <b>Import photos</b>.</p>
            </Empty>
          )}
        </div>
        <aside className="rail">
          <div className="rail-card">
            <h4>Totals</h4>
            <div className="big-stat">{scanned.toLocaleString()}</div>
            <div className="big-label">Frames scanned</div>
            <div className="stat-grid">
              <div><div className="n">{jobs.length}</div><div className="big-label">Albums</div></div>
              <div><div className="n">{jobs.filter((j) => j.status === 'done' || j.status === 'done_errors').length}</div><div className="big-label">Ready to review</div></div>
            </div>
            {jobs.length > 0 && (
              <div className="rail-actions">
                <button className="ghost" onClick={() => setStatsOpen(true)}>View statistics…</button>
              </div>
            )}
          </div>
          <div className="rail-card">
            <h4>Setup</h4>
            <p>{models.length ? `${ready} of ${models.length} models ready` : 'Checking…'}</p>
            <button onClick={() => void run(async () => { await call('install_models'); })} disabled={ready === models.length}>Install missing</button>
            <ul className="model-list">
              {models.map((m) => (
                <li key={m.file ?? m.name}><span className={`dot ${m.ready ? 'ok' : 'no'}`} />{m.name}<small>{m.ready ? 'Available' : 'Not installed'}</small></li>
              ))}
            </ul>
          </div>
        </aside>
      </div>
      {doomed && (
        <Confirm title="Forget this album?" action="Forget album" onCancel={() => setDoomed(null)}
          onConfirm={() => { onDelete(doomed); setDoomed(null); }}>
          <p><b>{doomed.label}</b>: its results and cached crops go. Your photographs and any XMP already written are not touched.</p>
        </Confirm>
      )}
      <GlobalStatsModal visible={statsOpen} onClose={() => setStatsOpen(false)} />
    </div>
  );
}
