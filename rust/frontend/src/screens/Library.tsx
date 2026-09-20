import { useRun } from '../state/hooks';
import { asset, call } from '../lib/api';
import { useEffect, useState } from 'react';
import { Confirm, Empty } from '../components/basics';
import { coverTint, jobState, pct, plural, shortDate } from '../lib/format';
import type { Job, ModelInfo } from '../lib/types';

type Props = {
  jobs: Job[]; models: ModelInfo[]; scanning: boolean;
  onOpen: (job: Job) => void; onNewScan: () => void; onIdentify: (job: Job) => void;
  onResume: (job: Job) => void; onDelete: (job: Job) => void;
};

function JobCard({ job, scanning, onOpen, onIdentify, onResume, onAsk }: {
  job: Job; scanning: boolean; onOpen: () => void; onIdentify: () => void; onResume: () => void; onAsk: () => void;
}) {
  const [cover, setCover] = useState<string>();
  useEffect(() => { let live = true; void call<{path: string} | null>('cover', {jobId: job.id}).then(c => { if(live) setCover(asset(c?.path)); }).catch(() => undefined); return () => { live = false; }; }, [job.id, job.done]);
  const left = job.total - job.done;
  const resumable = left > 0 && job.status !== 'scanning' && !scanning;
  return (
    <article className="job-card" tabIndex={0} onClick={onOpen} onKeyDown={(e) => { if (e.key === 'Enter' && e.target === e.currentTarget) onOpen(); }}>
      <div className="shot" style={{ background: coverTint(job.id), backgroundImage: cover ? `url("${cover}")` : undefined, backgroundSize: 'cover', backgroundPosition: 'center' }}>
        {/* Where the photograph would be, as on the Python home screen: said only while there is no cover. */}
        {!cover && <div className="state">{jobState(job)}</div>}
        <span className="badge">{plural(job.total, 'photo')}</span>
        {job.status === 'scanning' && <div className="bar cover-bar"><div className="fill" style={{ width: `${pct(job.done, job.total)}%` }} /></div>}
        {resumable && <button className="resume" onClick={(e) => { e.stopPropagation(); onResume(); }}>Resume {left.toLocaleString()}</button>}
        <div className="job-menu">
          <button className="iconbtn danger" title="Forget this album. Your photographs are not touched" onClick={(e) => { e.stopPropagation(); onAsk(); }}>Delete</button>
        </div>
      </div>
      <div className="name" title={job.root}>{job.label}</div>
      <div className="sub">{job.done.toLocaleString()} / {job.total.toLocaleString()} photos · {shortDate(job.created_at)}</div>
      <div className="steps">
        <button className="step" onClick={(e) => { e.stopPropagation(); onOpen(); }}>Review</button>
        {job.status === 'done' && (
          <button className="step" title="Name what survived the cull. This is the slow one." onClick={(e) => { e.stopPropagation(); onIdentify(); }}>Identify</button>
        )}
      </div>
    </article>
  );
}

/** Home: the albums as cover cards, with totals and model setup in the rail, as on the Python home screen. */
export function Library({ jobs, models, scanning, onOpen, onNewScan, onIdentify, onResume, onDelete }: Props) {
  const run = useRun();
  const [doomed, setDoomed] = useState<Job | null>(null);
  const scanned = jobs.reduce((n, j) => n + j.done, 0);
  const ready = models.filter((m) => m.ready).length;
  return (
    <div className="screen">
      <div className="home">
        <div className="home-main">
          <div className="new-scan">
            <div className="new-scan-head">
              <div>
                <h4>Scan a shoot</h4>
                <p>Point it at a folder. It measures focus on the subject, finds your keepers, reads numbers and plates, and writes XMP.</p>
              </div>
              <button className="primary" onClick={onNewScan}>+ New scan</button>
            </div>
          </div>
          <div className="section-label"><span>Albums</span><span>{jobs.length || ''}</span></div>
          {jobs.length ? (
            <div className="cards">
              {jobs.map((job) => (
                <JobCard key={job.id} job={job} scanning={scanning} onOpen={() => onOpen(job)} onIdentify={() => onIdentify(job)}
                  onResume={() => onResume(job)} onAsk={() => setDoomed(job)} />
              ))}
            </div>
          ) : (
            <Empty icon="folder" title="No albums yet.">
              <p>Drop a folder anywhere on this window, or press <b>New scan</b>.</p>
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
              <div><div className="n">{jobs.filter((j) => j.status === 'done').length}</div><div className="big-label">Ready to review</div></div>
            </div>
          </div>
          <div className="rail-card">
            <h4>Setup</h4>
            <p>{models.length ? `${ready} of ${models.length} models ready` : 'Checking…'}</p>
            <button onClick={() => void run(async () => { await call('install_models'); })} disabled={ready === models.length}>Install missing</button>
            <ul className="model-list">
              {models.map((m) => (
                <li key={m.file}><span className={`dot ${m.ready ? 'ok' : 'no'}`} />{m.name}<small>{m.ready ? 'Available' : 'Not installed'}</small></li>
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
    </div>
  );
}
