import { PipelineView } from '../components/PipelineView';
import { WorkerPanel } from '../components/WorkerPanel';
import { Empty, Icon } from '../components/basics';
import { asset } from '../lib/api';
import { etaLabel, etaRemaining, formatRate, orderStages, overallEta, overallFraction, stageStatus, summaryLine } from '../lib/scan.mjs';
import type { Job, ScanFound, ScanRecentFrame, Settings } from '../lib/types';
import type { Runner } from '../state/hooks';
import type { ScanLive } from '../state/useScanProgress';

type Props = {
  job: Job | undefined; live: ScanLive | undefined; running: boolean; paused: boolean;
  run: Runner; onSettingsSaved: (s: Settings) => void;
  onPause: () => void; onResume: () => void; onCancel: () => void; onReview: () => void; onLibrary: () => void;
};

const VERDICT: Record<ScanRecentFrame['verdict'], string> = { sharp: 'Sharp', soft: 'Soft' };
const FOUND: [keyof ScanFound, string][] = [
  ['subjects', 'Subjects'], ['soft', 'Soft'], ['closed_eyes', 'Closed eyes'], ['numbers', 'Numbers'], ['plates', 'Plates'], ['bursts', 'Bursts'], ['keepers', 'Keepers'],
];

function Thumbs({ recent }: { recent: ScanRecentFrame[] }) {
  if (!recent.length) return <p className="muted scan-wait">Frames appear here as they come through.</p>;
  return (
    <div className="scan-thumbs">
      {recent.slice(0, 24).map((f) => (
        <figure className={`scan-thumb ${f.verdict}`} key={f.image_id}>
          <img src={asset(f.thumb_url)} alt="" loading="lazy" draggable={false} />
          <span className="verdict">{(VERDICT[f.verdict] ?? f.verdict) + (f.panning ? ' · pan' : '')}</span>
          {f.label && <span className="label mono">{f.label}</span>}
        </figure>
      ))}
    </div>
  );
}

function Found({ found, identify }: { found: ScanFound; identify: boolean }) {
  return (
    <div className="rail-card found-card">
      <h4>Found so far</h4>
      <dl>
        {FOUND.filter(([k]) => identify || (k !== 'numbers' && k !== 'plates')).map(([k, label]) => (
          <div key={k} className={found[k] ? '' : 'zero'}><dt>{label}</dt><dd className="mono">{(found[k] ?? 0).toLocaleString()}</dd></div>
        ))}
      </dl>
    </div>
  );
}

/** An album while it scans (or just after): header, pipeline flowchart, live frames, what it found. Review is locked until it is done. */
export function ScanScreen({ job, live, running, paused, run, onSettingsSaved, onPause, onResume, onCancel, onReview, onLibrary }: Props) {
  const p = live?.progress;
  const state = p?.state === 'running' && !running && job?.status !== 'scanning' ? 'stopped' : p?.state ?? (running ? 'starting' : job?.status === 'done' ? 'done' : 'idle');
  if (!job || state === 'idle') {
    return (
      <div className="screen"><Empty icon="images" title="No scan is running.">
        <p>Start one from the Library with <b>Import photos</b>, or open an album to review it.</p>
        <button className="primary" onClick={onLibrary}>Back to library</button>
      </Empty></div>
    );
  }
  const stages = p ? orderStages(p.pipeline.stages) : [];
  const head = stages.find((s) => s.id === p?.stage);
  const identify = stages.some((s) => s.id === 'identify' && stageStatus(s) !== 'off');
  const total = stages[0]?.total || p?.total || job.total;
  const frac = p ? (state === 'done' ? 1 : overallFraction(stages, p.overall)) : 0;
  const pct = Math.round(frac * 100);
  const smoothed = live ? etaRemaining(live.history) : null;
  const eta = smoothed ?? (p ? overallEta(stages, p.overall) : null);
  const rate = formatRate(p?.rate_per_s);
  const doneNow = state === 'done';
  return (
    <div className="screen scan-screen">
      <div className="scan-page">
        <header className={`scan-head ${state}`}>
          <div className="sh-title">
            <h2>{job.label}</h2>
            <span className="muted">
              {state === 'starting' && 'Reading the folder…'}
              {state === 'running' && p && `${paused ? 'Paused · ' : ''}Stage ${p.stage_index + 1} of ${p.stage_count} · ${head?.label ?? p.stage}`}
              {doneNow && 'Scan complete'}
              {state === 'stopped' && 'Scan stopped. Progress is saved.'}
              {state === 'error' && 'The scan stopped on an error. Check the activity log in the top bar.'}
            </span>
          </div>
          {state === 'running' && p && (
            <dl className="sh-stats">
              <div><dt>Overall</dt><dd className="mono">{pct}%</dd></div>
              <div><dt>{head?.label ?? p.stage} stage</dt><dd className="mono">{p.done.toLocaleString()} / {p.total.toLocaleString()}</dd></div>
              <div><dt>Rate</dt><dd className="mono">{rate || '–'}</dd></div>
              <div className="eta"><dt>Time left</dt><dd className="mono">{etaLabel(eta, Boolean(live?.stable))}</dd></div>
            </dl>
          )}
          {(state === 'running' || state === 'starting') && (
            <div className="sh-actions">
              <button className="ghost" onClick={paused ? onResume : onPause}>{paused ? 'Resume' : 'Pause'}</button>
              <button className="ghost danger" onClick={onCancel}>Cancel scan</button>
            </div>
          )}
        </header>
        {(state === 'running' || state === 'starting') && (
          <>
            <div className="bar scan-bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label="Scan progress">
              {p ? <div className="fill" style={{ width: `${pct}%` }} /> : <div className="fill indeterminate" />}
            </div>
            <p className="muted scan-lock"><Icon name="eye" size={13} /> Review opens when the scan finishes. Other albums stay open from the Library.</p>
          </>
        )}
        {(doneNow || state === 'stopped') && p && (
          <section className="scan-summary" aria-label="Scan summary">
            <div className="ss-mark"><Icon name={doneNow ? 'check' : 'x'} size={20} /></div>
            <div className="ss-text">
              <b>{doneNow ? summaryLine(total, p.found) : `Stopped at ${pct}% · ${p.found.subjects.toLocaleString()} subjects found so far`}</b>
              <span className="muted">{doneNow ? 'Keepers are suggestions. Nothing is rejected or written until you say so.' : 'Frames that were read can be reviewed now. Resume the scan from the Library to finish grouping and picking.'}</span>
            </div>
            <button className="primary" autoFocus={doneNow} onClick={onReview}>Review</button>
          </section>
        )}
        {state === 'error' && <div className="scan-summary"><div className="ss-text"><b>The scan did not finish.</b></div><button onClick={onLibrary}>Back to library</button></div>}

        {p ? <PipelineView stages={p.pipeline.stages} workers={p.pipeline.workers} running={state === 'running'} /> : (
          <div className="pipe pipe-wait muted">Loading the models and listing the files. The pipeline appears in a moment.</div>
        )}

        <div className="scan-body">
          <section className="scan-live" aria-label="Frames as they finish">
            <div className="section-label"><span>Just finished</span><span>{p?.recent.length ? 'newest first' : ''}</span></div>
            <Thumbs recent={p?.recent ?? []} />
          </section>
          <aside className="scan-rail">
            {p && <Found found={p.found} identify={identify} />}
            <WorkerPanel run={run} onSaved={onSettingsSaved} scanning={state === 'running' || state === 'starting'} />
          </aside>
        </div>
      </div>
    </div>
  );
}
